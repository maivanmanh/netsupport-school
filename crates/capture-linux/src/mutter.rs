//! Thin wrapper over `org.gnome.Mutter.ScreenCast` (one monitor, one stream).

use std::collections::HashMap;
use zbus::{
    blocking::{Connection, Proxy},
    zvariant::{ObjectPath, OwnedObjectPath, Value},
};

const SERVICE: &str = "org.gnome.Mutter.ScreenCast";
const ROOT: &str = "/org/gnome/Mutter/ScreenCast";

/// Cursor handling requested from Mutter.
#[derive(Clone, Copy)]
pub enum Cursor {
    Hidden = 0,
    Embedded = 1,
}

pub struct Session {
    connection: Connection,
    session: OwnedObjectPath,
    pub node_id: u32,
    stopped: bool,
}

fn fail(context: &str, error: impl std::fmt::Display) -> String {
    format!("Mutter ScreenCast {context}: {error}")
}

impl Session {
    /// Records `connector` (empty selects the primary monitor).
    /// `remote_desktop_session` pairs the stream with a RemoteDesktop session.
    pub fn open(
        connector: &str,
        cursor: Cursor,
        remote_desktop_session: Option<&str>,
    ) -> Result<Self, String> {
        let connection = Connection::session().map_err(|e| fail("session bus", e))?;
        let root = Proxy::new(&connection, SERVICE, ROOT, "org.gnome.Mutter.ScreenCast")
            .map_err(|e| fail("proxy", e))?;
        let mut properties: HashMap<&str, Value<'_>> = HashMap::new();
        if let Some(id) = remote_desktop_session {
            properties.insert("remote-desktop-session-id", Value::from(id));
        }
        let session: OwnedObjectPath = root
            .call("CreateSession", &(properties,))
            .map_err(|e| fail("CreateSession", e))?;
        let session_proxy = Proxy::new(
            &connection,
            SERVICE,
            session.clone(),
            "org.gnome.Mutter.ScreenCast.Session",
        )
        .map_err(|e| fail("session proxy", e))?;
        let mut options: HashMap<&str, Value<'_>> = HashMap::new();
        options.insert("cursor-mode", Value::from(cursor as u32));
        let stream: OwnedObjectPath = session_proxy
            .call("RecordMonitor", &(connector, options))
            .map_err(|e| fail("RecordMonitor", e))?;
        let stream_proxy = Proxy::new(
            &connection,
            SERVICE,
            ObjectPath::from(stream.as_ref()).to_owned(),
            "org.gnome.Mutter.ScreenCast.Stream",
        )
        .map_err(|e| fail("stream proxy", e))?;
        // The node id is announced by a signal after Start; subscribe first.
        let mut added = stream_proxy
            .receive_signal("PipeWireStreamAdded")
            .map_err(|e| fail("signal", e))?;
        if remote_desktop_session.is_none() {
            session_proxy
                .call::<_, _, ()>("Start", &())
                .map_err(|e| fail("Start", e))?;
        }
        let node_id = if remote_desktop_session.is_some() {
            // Started by the RemoteDesktop session owner; see `wait_node`.
            0
        } else {
            let message = added.next().ok_or_else(|| fail("stream", "closed before start"))?;
            message
                .body()
                .deserialize::<u32>()
                .map_err(|e| fail("node id", e))?
        };
        Ok(Self { connection, session, node_id, stopped: false })
    }

    pub fn stop(&mut self) {
        if self.stopped {
            return;
        }
        self.stopped = true;
        if let Ok(proxy) = Proxy::new(
            &self.connection,
            SERVICE,
            self.session.clone(),
            "org.gnome.Mutter.ScreenCast.Session",
        ) {
            let _ = proxy.call::<_, _, ()>("Stop", &());
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.stop();
    }
}
