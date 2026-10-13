//! Opt-in real Linux receiver/window integration test. Run under Xvfb or a
//! disposable desktop; the test deliberately opens the presentation surface.

use super::*;
use prost::Message;
use std::sync::Arc;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::TcpListener,
};

const FIXTURES: [&[u8]; 3] = [
    include_bytes!("../../../crates/codec-linux/tests/fixtures/testsrc-64x48-0.h264"),
    include_bytes!("../../../crates/codec-linux/tests/fixtures/testsrc-64x48-1.h264"),
    include_bytes!("../../../crates/codec-linux/tests/fixtures/testsrc-64x48-2.h264"),
];

async fn read_control(reader: &mut (impl AsyncRead + Unpin)) -> Result<protocol::Envelope> {
    let length = reader.read_u32().await? as usize;
    anyhow::ensure!(length <= protocol::MAX_CONTROL_FRAME_BYTES);
    let mut bytes = vec![0; length];
    reader.read_exact(&mut bytes).await?;
    Ok(protocol::Envelope::decode(bytes.as_slice())?)
}

async fn write_control(
    writer: &mut (impl AsyncWrite + Unpin),
    message: protocol::Envelope,
) -> Result<()> {
    let bytes = message.encode_to_vec();
    writer.write_u32(u32::try_from(bytes.len())?).await?;
    writer.write_all(&bytes).await?;
    Ok(())
}

fn presentation(id: u64, media_port: u16) -> protocol::StartPresentation {
    protocol::StartPresentation {
        stream_id: id,
        width: 64,
        height: 48,
        max_fps: 30,
        bitrate_bps: 1_000_000,
        rtp_payload_type: 96,
        rtp_clock_hz: RTP_VIDEO_CLOCK_HZ,
        teacher_media_udp_port: u32::from(media_port),
    }
}

/// Exercises registration, late join, RTP/FU-A ingress, jitter/depacketize,
/// software decode, native rendering, Stop/Start and TCP disconnect teardown.
/// No configured classroom endpoints or real Teacher/capture backend are used.
#[test]
#[ignore = "requires a Linux graphical session; run with xvfb-run -a"]
fn linux_student_smoke_receives_renders_restarts_and_disconnects() -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_test_writer()
        .try_init();
    anyhow::ensure!(
        std::env::var_os("DISPLAY").is_some() || std::env::var_os("WAYLAND_DISPLAY").is_some(),
        "run under xvfb-run -a or a disposable graphical desktop"
    );
    // Match normal Student startup: establish the hidden native host before
    // registering, and retain its display connection across presentation cycles.
    let mut desktop = Some(desktop_tray::StudentDesktop::new()?);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let listener = runtime.block_on(TcpListener::bind((Ipv4Addr::LOCALHOST, 0)))?;
    let SocketAddr::V4(teacher_address) = listener.local_addr()? else {
        unreachable!()
    };
    let teacher_udp = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))?;
    let SocketAddr::V4(teacher_media) = teacher_udp.local_addr()? else {
        unreachable!()
    };
    let student_udp = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))?;
    let student_port = student_udp.local_addr()?.port();
    let reports = Arc::new(Mutex::new(Vec::<protocol::ReceiverReport>::new()));
    let teacher_reports = Arc::clone(&reports);
    let (restart_tx, mut restart_rx) = tokio::sync::mpsc::channel::<()>(1);
    let teacher = runtime.spawn(async move {
        let (mut connection, _) = listener.accept().await?;
        let hello = read_control(&mut connection).await?;
        let Some(protocol::envelope::Payload::HelloStudent(hello)) = hello.payload else {
            anyhow::bail!("expected Student registration");
        };
        anyhow::ensure!(hello.media_udp_port == u32::from(student_port));
        // Simulate joining a Teacher that is already presenting.
        let mut welcome = protocol::Envelope::welcome(1);
        if let Some(protocol::envelope::Payload::WelcomeStudent(value)) = &mut welcome.payload {
            value.presentation_active = true;
        }
        write_control(&mut connection, welcome).await?;
        write_control(
            &mut connection,
            protocol::Envelope::start_presentation(presentation(101, teacher_media.port())),
        )
        .await?;
        let (mut reader, mut writer) = connection.into_split();
        let (messages_tx, mut messages_rx) = tokio::sync::mpsc::channel(64);
        let control_reader = tokio::spawn(async move {
            while let Ok(message) = read_control(&mut reader).await {
                if messages_tx.send(message).await.is_err() {
                    break;
                }
            }
        });
        let result = async {
            for stream_id in [101, 102] {
                if stream_id == 102 {
                    restart_rx.recv().await.context("restart command missing")?;
                    write_control(
                        &mut writer,
                        protocol::Envelope::start_presentation(presentation(102, teacher_media.port())),
                    )
                    .await?;
                }
                let started = Instant::now();
                let mut tick = tokio::time::interval(Duration::from_millis(33));
                tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                let mut packetizer = H264Packetizer::new(protocol::presentation_ssrc(stream_id));
                let mut index = 0_u64;
                let mut first_render_report_at: Option<Instant> = None;
                // Window creation in a shared VM can take several seconds. Keep
                // sending until the receiver proves native rendering, then send
                // for another second before exercising Stop/disconnect. This is
                // a functional check, not a startup-latency benchmark.
                while started.elapsed() < Duration::from_secs(12)
                    && !first_render_report_at
                        .is_some_and(|at| at.elapsed() >= Duration::from_secs(1))
                {
                    tokio::select! {
                        _ = tick.tick() => {
                            let fixture = usize::try_from(index % 3)?;
                            for packet in packetizer.packetize(EncodedAccessUnitRef {
                                bytes: FIXTURES[fixture],
                                timestamp: Duration::from_millis(index * 33),
                                is_keyframe: fixture == 0,
                            }) {
                                teacher_udp.send_to(&packet, (Ipv4Addr::LOCALHOST, student_port))?;
                            }
                            index += 1;
                        }
                        Some(message) = messages_rx.recv() => {
                            match message.payload {
                                Some(protocol::envelope::Payload::ReceiverReport(report)) => {
                                    if report.stream_id == stream_id
                                        && report.received_packets > 0
                                        && report.decoded_frames > 0
                                        && report.rendered_frames > 0
                                        && report.rendering_enabled
                                    {
                                        first_render_report_at.get_or_insert_with(Instant::now);
                                    }
                                    teacher_reports.lock().unwrap().push(report);
                                }
                                Some(protocol::envelope::Payload::ControlPing(ping)) => {
                                    let now = media_core::unix_us();
                                    write_control(&mut writer, protocol::Envelope {
                                        protocol_version: protocol::PROTOCOL_VERSION,
                                        payload: Some(protocol::envelope::Payload::ControlPong(protocol::ControlPong {
                                            nonce: ping.nonce,
                                            sender_unix_us: ping.sender_unix_us,
                                            teacher_received_unix_us: now,
                                            teacher_sent_unix_us: now,
                                        })),
                                    }).await?;
                                }
                                _ => {}
                            }
                        }
                    }
                }
                anyhow::ensure!(
                    first_render_report_at.is_some(),
                    "stream {stream_id} did not report native rendering within 12 seconds"
                );
                if stream_id == 101 {
                    write_control(&mut writer, protocol::Envelope::stop_presentation(101)).await?;
                }
            }
            Ok::<_, anyhow::Error>(())
        }.await;
        control_reader.abort();
        result
    });
    let (control, client_id, first) = runtime.block_on(async {
        tokio::time::timeout(
            Duration::from_secs(3),
            register_student_for_presentation(
                teacher_address,
                StudentRegistration {
                    hostname: "ubuntu-smoke".to_owned(),
                    instance_id: "isolated-loopback-smoke".to_owned(),
                    selected_local_ip: Ipv4Addr::LOCALHOST,
                    media_udp_port: student_port,
                    feedback_udp_port: student_port,
                    pointer_overlay: true,
                },
            ),
        )
        .await?
        .map_err(anyhow::Error::from)
    })?;
    let (mut config, _) = Config::load(None)?;
    config.diagnostics.student_fps_badge = false;
    for (stream_id, expected) in [
        (101, StudentSessionOutcome::PresentationStopped),
        (102, StudentSessionOutcome::Reconnect),
    ] {
        let active = if stream_id == 101 {
            first
                .clone()
                .context("active late join omitted presentation")?
        } else {
            runtime.block_on(async {
                restart_tx.send(()).await?;
                tokio::time::timeout(Duration::from_secs(2), async {
                    loop {
                        if let Some(active) = control.presentation().filter(|p| p.stream_id == 102)
                        {
                            return active;
                        }
                        tokio::time::sleep(Duration::from_millis(5)).await;
                    }
                })
                .await
                .map_err(anyhow::Error::from)
            })?
        };
        let started = Instant::now();
        let outcome = run_student_media_loop(
            &config,
            &active,
            &control,
            client_id,
            teacher_media,
            Some(started + Duration::from_secs(15)),
            &student_udp,
            false,
            &mut desktop,
            None,
            None,
        )?;
        assert_eq!(outcome, expected);
        assert!(
            started.elapsed() < Duration::from_secs(15),
            "teardown exceeded deadline"
        );
        let reports = reports.lock().unwrap();
        assert!(
            reports.iter().any(|report| {
                report.stream_id == stream_id
                    && report.received_packets > 0
                    && report.decoded_frames > 0
                    && report.rendered_frames > 0
                    && report.rendering_enabled
            }),
            "stream {stream_id} did not prove UDP receive/decode/native render: {reports:?}"
        );
    }
    runtime.block_on(teacher)??;
    Ok(())
}
