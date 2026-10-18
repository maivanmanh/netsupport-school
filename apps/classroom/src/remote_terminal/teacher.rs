use super::{
    trust,
    wire::{self, Frame},
};
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, VecDeque},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use subtle::ConstantTimeEq;
use tokio::{
    net::TcpStream,
    sync::{mpsc, Mutex as AsyncMutex},
};
use tokio_rustls::TlsAcceptor;

pub struct Session {
    pub student: String,
    root: PathBuf,
    secret: String,
    input: mpsc::Sender<Frame>,
    input_rx: AsyncMutex<mpsc::Receiver<Frame>>,
    output: mpsc::Sender<Frame>,
    output_rx: AsyncMutex<mpsc::Receiver<Frame>>,
    pending: Mutex<VecDeque<Frame>>,
    ui_pending: Mutex<Option<Frame>>,
    output_seq: AtomicU64,
    input_seq: AtomicU64,
    input_bytes: AtomicU64,
    output_bytes: AtomicU64,
    network: AtomicBool,
    ui: AtomicBool,
    closed: AtomicBool,
    touched: Mutex<Instant>,
    created: Instant,
    status: Mutex<Value>,
}
#[derive(Default)]
pub struct Manager {
    sessions: Mutex<HashMap<String, Arc<Session>>>,
}
struct Connection<'a>(&'a AtomicBool);
impl Drop for Connection<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}
impl Manager {
    pub fn open(&self, root: &Path, student: &str) -> Result<String> {
        use base64::Engine as _;
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).map_err(|e| anyhow::anyhow!("OS random unavailable: {e}"))?;
        let secret = base64::engine::general_purpose::STANDARD.encode(bytes);
        let mut sessions = self.sessions.lock().unwrap();
        sessions.retain(|_, s| {
            if !s.ui.load(Ordering::Acquire) && s.touched.lock().unwrap().elapsed() > wire::GRACE {
                s.closed.store(true, Ordering::Release);
            }
            !s.closed.load(Ordering::Acquire)
        });
        ensure!(
            sessions.len() < 16 && sessions.values().filter(|s| s.student == student).count() < 4,
            "Terminal session limit reached"
        );
        let id = crate::classroom_tools::nonce()?;
        trust::audit(
            root,
            &id,
            "open",
            json!({"student":student,"operator":std::env::var("USER").or_else(|_|std::env::var("USERNAME")).unwrap_or_default()}),
        )?;
        let (input, input_rx) = mpsc::channel(32);
        let (output, output_rx) = mpsc::channel(32);
        sessions.insert(
            id.clone(),
            Arc::new(Session {
                student: student.into(),
                root: root.into(),
                secret,
                input,
                input_rx: AsyncMutex::new(input_rx),
                output,
                output_rx: AsyncMutex::new(output_rx),
                pending: Mutex::new(VecDeque::new()),
                ui_pending: Mutex::new(None),
                output_seq: AtomicU64::new(0),
                input_seq: AtomicU64::new(0),
                input_bytes: AtomicU64::new(0),
                output_bytes: AtomicU64::new(0),
                network: AtomicBool::new(false),
                ui: AtomicBool::new(false),
                closed: AtomicBool::new(false),
                touched: Mutex::new(Instant::now()),
                created: Instant::now(),
                status: Mutex::new(json!({"state":"connecting"})),
            }),
        );
        Ok(id)
    }
    pub fn authorization(&self, id: &str, teacher_ip: std::net::Ipv4Addr) -> Result<String> {
        let session = self.session(id)?;
        ensure!(
            !session.closed.load(Ordering::Acquire),
            "Terminal session ended"
        );
        let identity = trust::Identity::load(&session.root)?;
        Ok(serde_json::to_string(&trust::Policy {
            enabled: true,
            allow_desktop_open: false,
            teacher_ip,
            teacher_certificate: identity.certificate,
            secret: session.secret.clone(),
        })?)
    }
    fn session(&self, id: &str) -> Result<Arc<Session>> {
        self.sessions
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .context("Terminal session ended")
    }
    pub fn close(&self, id: &str) {
        if let Ok(s) = self.session(id) {
            if s.closed.swap(true, Ordering::AcqRel) {
                return;
            }
            let _ = trust::audit(
                &s.root,
                id,
                "close",
                json!({"student":s.student,"input_bytes":s.input_bytes.load(Ordering::Acquire),"output_bytes":s.output_bytes.load(Ordering::Acquire)}),
            );
        }
    }
    pub fn failed(&self, id: &str, student: &str, error: &str) -> bool {
        let Ok(s) = self.session(id) else {
            return false;
        };
        if s.student != student {
            return false;
        }
        *s.status.lock().unwrap() = json!({"state":"error","error":error});
        let _ = s.output.try_send(Frame::new(
            wire::EXIT,
            0,
            error
                .as_bytes()
                .iter()
                .copied()
                .take(2000)
                .collect::<Vec<_>>(),
        ));
        true
    }
    pub fn serve(&self, socket: std::net::TcpStream, id: &str, student: &str) -> Result<()> {
        let s = self.session(id)?;
        ensure!(
            s.student == student && !s.closed.load(Ordering::Acquire),
            "Terminal session unauthorized"
        );
        ensure!(
            !s.network.swap(true, Ordering::AcqRel),
            "Terminal already connected"
        );
        let _connection = Connection(&s.network);
        let tls = trust::Identity::load(&s.root)?.server()?;
        socket.set_read_timeout(None)?;
        socket.set_write_timeout(None)?;
        socket.set_nonblocking(true)?;
        tokio::runtime::Builder::new_current_thread().enable_all().build()?.block_on(async {
            let stream=TcpStream::from_std(socket)?;
            let mut tls=tokio::time::timeout(Duration::from_secs(5),TlsAcceptor::from(Arc::new(tls)).accept(stream)).await??;
            let auth=tokio::time::timeout(Duration::from_secs(5),wire::read(&mut tls)).await??;
            ensure!(auth.kind==wire::AUTH && bool::from(auth.data.as_slice().ct_eq(s.secret.as_bytes())),"Terminal authentication rejected");
            wire::write(&mut tls,&Frame::new(wire::AUTH,0,vec![])).await?;
            trust::audit(&s.root,id,"connected",json!({"student":student}))?;
            let (mut reader,mut writer)=tokio::io::split(tls);
            // A dedicated reader preserves partial binary frames across select!.
            let (tx,mut rx)=mpsc::channel(8);
            let read_task=tokio::spawn(async move { while let Ok(f)=wire::read(&mut reader).await { if tx.send(f).await.is_err(){break;} } });
            let result=async {
                let mut input=s.input_rx.lock().await;
                let replay=s.pending.lock().unwrap().clone();
                for f in replay {wire::write(&mut writer,&f).await?;}
                let mut timer=tokio::time::interval(Duration::from_secs(1));
                loop {
                    tokio::select! {
                        f=rx.recv()=> {
                            let f=f.context("Terminal Student disconnected")?;
                            match f.kind {
                                wire::DATA=> {
                                    let previous=s.output_seq.load(Ordering::Acquire);
                                    if f.seq>previous {
                                        ensure!(f.seq==previous+1,"Terminal output sequence gap");
                                        tokio::time::timeout(Duration::from_secs(2),s.output.send(f.clone())).await?.context("Terminal UI gone")?;
                                        s.output_bytes.fetch_add(f.data.len() as u64,Ordering::Relaxed);
                                        s.output_seq.store(f.seq,Ordering::Release);
                                    }
                                    wire::write(&mut writer,&Frame::new(wire::ACK,f.seq,vec![])).await?;
                                }
                                wire::ACK=>s.pending.lock().unwrap().retain(|p|p.seq>f.seq),
                                wire::READY=>{
                                    *s.status.lock().unwrap()=serde_json::from_slice(&f.data)?;
                                }
                                wire::EXIT=>{
                                    tokio::time::timeout(Duration::from_secs(2),s.output.send(f)).await??;
                                    *s.status.lock().unwrap()=json!({"state":"ended"});
                                    break;
                                }
                                wire::PING=>{},
                                _=>anyhow::bail!("Invalid Student terminal frame"),
                            }
                        }
                        f=input.recv(), if s.pending.lock().unwrap().len()<32 => {
                            let f=f.context("Terminal input closed")?;
                            s.pending.lock().unwrap().push_back(f.clone());
                            wire::write(&mut writer,&f).await?;
                        }
                        _=timer.tick()=> {
                            if s.closed.load(Ordering::Acquire) || s.touched.lock().unwrap().elapsed()>wire::GRACE {
                                let _=wire::write(&mut writer,&Frame::new(wire::CLOSE,0,vec![])).await;
                                s.closed.store(true,Ordering::Release);break;
                            }
                            wire::write(&mut writer,&Frame::new(wire::PING,0,vec![])).await?;
                        }
                    }
                }
                Ok::<(),anyhow::Error>(())
            }.await;
            read_task.abort();
            let _=trust::audit(&s.root,id,"detached",json!({"student":student,"error":result.as_ref().err().map(ToString::to_string)}));
            result
        })
    }
    pub async fn attach(&self, socket: TcpStream, id: &str, last_seq: u64) -> Result<()> {
        let s = self.session(id)?;
        ensure!(!s.closed.load(Ordering::Acquire), "Terminal session ended");
        ensure!(
            !s.ui.swap(true, Ordering::AcqRel),
            "Terminal UI already attached"
        );
        let _connection = Connection(&s.ui);
        *s.touched.lock().unwrap() = Instant::now();
        let (mut reader, mut writer) = socket.into_split();
        let (tx, mut frames) = mpsc::channel(8);
        let reader_task = tokio::spawn(async move {
            while let Ok(f) = wire::read(&mut reader).await {
                if tx.send(f).await.is_err() {
                    break;
                }
            }
        });
        struct Reader(tokio::task::JoinHandle<()>);
        impl Drop for Reader {
            fn drop(&mut self) {
                self.0.abort();
            }
        }
        let _reader = Reader(reader_task);
        let mut output = s.output_rx.lock().await;
        {
            let mut pending = s.ui_pending.lock().unwrap();
            if pending.as_ref().is_some_and(|f| f.seq <= last_seq) {
                *pending = None;
            }
        }
        let initial = s.ui_pending.lock().unwrap().clone();
        let mut waiting = initial.is_some();
        if let Some(f) = initial {
            wire::write(&mut writer, &f).await?;
        }
        // Status is pushed the moment it changes (checked every 50 ms) so the UI
        // never waits out a heartbeat to learn the shell is connected; an
        // unchanged status is still repeated every second as a liveness signal.
        let mut timer = tokio::time::interval(Duration::from_millis(50));
        let mut last_status: Option<(Vec<u8>, Instant)> = None;
        loop {
            tokio::select! {
                f=frames.recv()=>{
                    let f=f.context("Terminal UI disconnected")?;
                    *s.touched.lock().unwrap()=Instant::now();
                    match f.kind {
                        wire::INPUT|wire::RESIZE=> {
                            ensure!(!s.closed.load(Ordering::Acquire),"Terminal ended");
                            let seq=s.input_seq.load(Ordering::Acquire)+1;
                            if f.kind==wire::INPUT{s.input_bytes.fetch_add(f.data.len() as u64,Ordering::Relaxed);}
                            if f.kind==wire::RESIZE{trust::audit(&s.root,id,"resize",json!({"size":f.data}))?;}
                            tokio::time::timeout(Duration::from_secs(2),s.input.send(Frame::new(f.kind,seq,f.data))).await??;
                            s.input_seq.store(seq,Ordering::Release);
                        }
                        wire::ACK=>{
                            let mut pending=s.ui_pending.lock().unwrap();
                            if pending.as_ref().is_some_and(|p|p.seq==f.seq) {*pending=None;waiting=false;}
                        }
                        wire::PING=>{},
                        wire::CLOSE=>{self.close(id);break;}
                        _=>anyhow::bail!("Invalid terminal UI frame"),
                    }
                }
                f=output.recv(),if !waiting=>{
                    let f=f.context("Terminal output ended")?;
                    *s.ui_pending.lock().unwrap()=Some(f.clone());waiting=true;
                    wire::write(&mut writer,&f).await?;
                    if f.kind==wire::EXIT {self.close(id);break;}
                }
                _=timer.tick()=>{
                    if s.closed.load(Ordering::Acquire){break;}
                    let mut status=s.status.lock().unwrap().clone();
                    if status["state"]=="connecting" && s.created.elapsed()>wire::CONNECT_GRACE {
                        wire::write(&mut writer,&Frame::new(wire::EXIT,0,"Máy học viên không khởi động được terminal sau 30 giây. Hãy kiểm tra máy đang bật, đã kết nối mạng và đúng phiên bản, rồi mở phiên mới.".as_bytes().to_vec())).await?;
                        self.close(id);break;
                    }
                    status["connected"]=json!(s.network.load(Ordering::Acquire));
                    let encoded=serde_json::to_vec(&status)?;
                    if last_status.as_ref().is_none_or(|(sent,at)|*sent!=encoded||at.elapsed()>=Duration::from_secs(1)) {
                        wire::write(&mut writer,&Frame::new(wire::READY,0,encoded.clone())).await?;
                        last_status=Some((encoded,Instant::now()));
                    }
                }
            }
        }
        Ok(())
    }
}
impl Drop for Manager {
    fn drop(&mut self) {
        for s in self.sessions.lock().unwrap().values() {
            s.closed.store(true, Ordering::Release);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sessions_need_no_pairing_and_have_independent_authorization() {
        let root = std::env::temp_dir().join(format!(
            "classroom-terminal-test-{}",
            crate::classroom_tools::nonce().unwrap()
        ));
        let manager = Manager::default();
        let teacher = std::net::Ipv4Addr::LOCALHOST;
        let first = manager.open(&root, "unpaired-student").unwrap();
        let second = manager.open(&root, "unpaired-student").unwrap();
        let grant: trust::Policy =
            serde_json::from_str(&manager.authorization(&first, teacher).unwrap()).unwrap();
        let other: trust::Policy =
            serde_json::from_str(&manager.authorization(&second, teacher).unwrap()).unwrap();
        assert!(grant.validate(&teacher).is_ok());
        assert!(grant
            .validate(&std::net::Ipv4Addr::new(10, 0, 0, 2))
            .is_err());
        assert_ne!(grant.secret, other.secret);
        assert_eq!(grant.teacher_certificate, other.teacher_certificate);
        assert!(!root.join("remote-terminal.json").exists());
        let mut malformed = grant.clone();
        malformed.secret = "not-a-session-secret".into();
        assert!(malformed.validate(&teacher).is_err());
        manager.close(&first);
        assert!(manager.authorization(&first, teacher).is_err());
        drop(manager);
        std::fs::remove_dir_all(root).unwrap();
    }
}
