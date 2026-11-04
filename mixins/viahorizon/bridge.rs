// SPDX-License-Identifier: GPL-3.0-or-later
// Packet bodies are rewritten before compression/encryption. Horizon continues
// to own TCP/SRV resolution, authentication, framing, AES/CFB8 and compression.
use super::{ConnReader, ConnWriter, Connection, Counters};
use crate::vfp_core as core;
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex, OnceLock, Weak,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

struct Entry {
    owner: Weak<Counters>,
    session: Arc<Mutex<State>>,
    capabilities: Arc<crate::vfp_session::PlayCapabilities>,
}
use crate::vfp_session::Session;
enum State {
    Translated(Box<Session>),
    Native(Box<crate::vfp_chat_signing::Signing>),
}
impl State {
    fn serverbound(&mut self, body: &[u8]) -> core::Result<Vec<Vec<u8>>> {
        match self {
            Self::Translated(session) => session.serverbound(body),
            Self::Native(signing) => signing.outgoing(body),
        }
    }
    fn poll(&mut self) -> Option<crate::Result<crate::vfp_wire::Packet>> {
        match self {
            Self::Translated(session) => session.poll(),
            Self::Native(_) => None,
        }
    }
    fn needs_event_poll(&self) -> bool {
        matches!(self,Self::Translated(session) if session.needs_event_poll())
    }
    fn ingest(&mut self, id: i32, payload: &[u8]) -> core::Result<Option<crate::vfp_wire::Packet>> {
        match self {
            Self::Translated(session) => session.ingest(id, payload),
            Self::Native(signing) => {
                signing.observe(id, payload)?;
                Ok(Some((id, payload.to_vec())))
            }
        }
    }
}

#[cfg(test)]
mod capability_tests {
    use super::*;
    #[test]
    fn gameplay_queries_do_not_wait_for_chunk_translation() {
        let (pipe, _peer) = crate::connection::Pipe::pair();
        let connection = Connection::local(pipe);
        install_protocol_fixture(
            &connection.reader,
            Session::for_release_fixture("1.7.2").unwrap(),
        );
        let state = session(&connection.reader.counters).unwrap();
        let guard = state.lock().unwrap();
        let (send, receive) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let values = (
                full_item_predictions(&connection.writer),
                legacy_play_protocol(&connection.writer),
                unsequenced_blocks(&connection.writer),
            );
            send.send(values).unwrap();
        });
        let result = receive.recv_timeout(Duration::from_secs(1));
        drop(guard);
        worker.join().unwrap();
        assert_eq!(result.unwrap(), (false, None, false));
    }
}
fn install_native_chat(writer: &ConnWriter) {
    if let Some(signing) = crate::vfp_chat_signing::take_prepared() {
        let mut map = sessions().lock().unwrap_or_else(|e| e.into_inner());
        map.insert(
            Arc::as_ptr(&writer.counters) as usize,
            Entry {
                owner: Arc::downgrade(&writer.counters),
                session: Arc::new(Mutex::new(State::Native(Box::new(signing)))),
                capabilities: Arc::default(),
            },
        );
        ACTIVE.store(true, Ordering::Release);
    }
}
// Only used by the protocol harness. Player settings use the guarded gameplay
// constructor, or the explicit configuration-preview constructor.
pub fn install_protocol_fixture(reader: &ConnReader, state: Session) {
    let mut map = sessions().lock().unwrap_or_else(|e| e.into_inner());
    map.insert(
        Arc::as_ptr(&reader.counters) as usize,
        Entry {
            owner: Arc::downgrade(&reader.counters),
            capabilities: state.capabilities(),
            session: Arc::new(Mutex::new(State::Translated(Box::new(state)))),
        },
    );
    ACTIVE.store(true, Ordering::Release);
}
static SESSIONS: OnceLock<Mutex<HashMap<usize, Entry>>> = OnceLock::new();
static ACTIVE: AtomicBool = AtomicBool::new(false);
fn sessions() -> &'static Mutex<HashMap<usize, Entry>> {
    SESSIONS.get_or_init(Mutex::default)
}
fn session(counters: &Arc<Counters>) -> Option<Arc<Mutex<State>>> {
    if !ACTIVE.load(Ordering::Acquire) {
        return None;
    }
    let key = Arc::as_ptr(counters) as usize;
    let mut map = sessions().lock().unwrap_or_else(|e| e.into_inner());
    map.retain(|_, e| e.owner.strong_count() > 0);
    ACTIVE.store(!map.is_empty(), Ordering::Release);
    let entry = map.get(&key)?;
    entry
        .owner
        .upgrade()
        .filter(|owner| Arc::ptr_eq(owner, counters))
        .map(|_| entry.session.clone())
}
pub fn full_item_predictions(writer: &ConnWriter) -> bool {
    capabilities(&writer.counters).is_some_and(|state| state.full_items())
}
pub fn legacy_play_protocol(writer: &ConnWriter) -> Option<i32> {
    capabilities(&writer.counters)?.legacy_protocol()
}
pub fn unsequenced_blocks(writer: &ConnWriter) -> bool {
    capabilities(&writer.counters).is_some_and(|state| state.unsequenced_blocks())
}
pub fn capabilities(counters: &Arc<Counters>) -> Option<Arc<crate::vfp_session::PlayCapabilities>> {
    if !ACTIVE.load(Ordering::Acquire) {
        return None;
    }
    let map = sessions().lock().unwrap_or_else(|error| error.into_inner());
    let entry = map.get(&(Arc::as_ptr(counters) as usize))?;
    entry
        .owner
        .upgrade()
        .filter(|owner| Arc::ptr_eq(owner, counters))?;
    Some(entry.capabilities.clone())
}
pub fn owns(writer: &ConnWriter, counters: &Arc<Counters>) -> bool {
    Arc::ptr_eq(&writer.counters, counters)
}
pub fn probe_version(host: &str, port: u16) -> core::Result<i32> {
    use crate::packets::{Empty, handshake, status};
    let timeout = Duration::from_secs(3);
    let mut c = Connection::connect_server(host, port, timeout).map_err(|e| e.to_string())?;
    c.reader
        .set_read_timeout(Some(timeout))
        .map_err(|e| e.to_string())?;
    // Intent 1 cannot install a translation session: probes never recurse.
    c.send(
        &handshake::Serverbound::Intention(handshake::Intention {
            protocol: core::CLIENT_PROTOCOL,
            host: host.into(),
            port,
            intent: handshake::STATUS,
        })
        .encode(),
    )
    .map_err(|e| e.to_string())?;
    c.send(&status::Serverbound::StatusRequest(Empty).encode())
        .map_err(|e| e.to_string())?;
    let (id, payload) = c.recv().map_err(|e| e.to_string())?;
    match status::Clientbound::decode(id, &payload).map_err(|e| e.to_string())? {
        status::Clientbound::StatusResponse(response) => Ok(core::status_json(&response.json)?.1),
        _ => Err("Unexpected server status packet".into()),
    }
}
pub fn send(writer: &mut ConnWriter, body: &[u8]) -> Option<crate::Result<()>> {
    let state = session(&writer.counters);
    if state.is_none()
        && core::Cursor::new(body).varint().ok() == Some(crate::vfp_full_click::INTERNAL_ID)
    {
        return Some(Err(crate::Error::new(
            "Legacy inventory prediction has no translation session",
        )));
    }
    let result = if let Some(state) = state {
        state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .serverbound(body)
    } else {
        // Only a connection's FIRST outgoing frame can be a handshake. A
        // settings change during play must never reinterpret a play packet.
        if (!core::needs_routing() && !crate::vfp_chat_signing::has_prepared())
            || writer.counters.sent.load(Ordering::Relaxed) != 0
        {
            return None;
        }
        let config = core::configuration();
        if !config.enabled {
            return None;
        }
        let handshake = core::Handshake::parse(body).ok()?;
        if handshake.protocol != core::CLIENT_PROTOCOL || handshake.intent == 1 {
            return None;
        }
        // Modded forwarding payloads are not rewritten.
        // Integrated/LAN server traffic is also excluded: it uses memory pipes.
        if matches!(writer.stream, super::Stream::Memory(_)) || handshake.host.contains('\0') {
            return None;
        }
        let target = config.selection(&handshake.host, handshake.port);
        if target == core::Selection::Native {
            install_native_chat(writer);
            return None;
        }
        let detected = if target == core::Selection::Auto {
            match probe_version(&handshake.host, handshake.port) {
                Ok(v) => Some(v),
                Err(e) => {
                    return Some(Err(crate::Error::new(format!(
                        "ViaHorizon: version detection failed: {e}"
                    ))));
                }
            }
        } else {
            None
        };
        let release = match core::choose(&target, detected, config.login_preview) {
            Ok(Some(r)) => r,
            Ok(None) => {
                install_native_chat(writer);
                return None;
            }
            Err(e) => return Some(Err(crate::Error::new(e))),
        };
        let session = match if config.login_preview {
            Session::preview(release)
        } else {
            Session::gameplay(release)
        } {
            Ok(s) => s,
            Err(e) => return Some(Err(crate::Error::new(e))),
        };
        let mut map = sessions().lock().unwrap_or_else(|e| e.into_inner());
        map.insert(
            Arc::as_ptr(&writer.counters) as usize,
            Entry {
                owner: Arc::downgrade(&writer.counters),
                capabilities: session.capabilities(),
                session: Arc::new(Mutex::new(State::Translated(Box::new(session)))),
            },
        );
        ACTIVE.store(true, Ordering::Release);
        drop(map);
        Ok(vec![handshake.retarget(release.protocol).body()])
    };
    Some(result.map_err(crate::Error::new).and_then(|bodies| {
        for body in bodies {
            let contents = crate::frame::contents(&body, writer.threshold)?;
            writer.send_contents(&contents)?;
        }
        Ok(())
    }))
}
thread_local! { static RAW_READ: std::cell::Cell<bool> = const {std::cell::Cell::new(false)}; }
thread_local! {
    static POLL_STATE: std::cell::RefCell<Option<Arc<Mutex<State>>>> = const {std::cell::RefCell::new(None)};
    static SYNTHETIC_PACKET: std::cell::RefCell<Option<crate::Result<(i32,Vec<u8>)>>> = const {std::cell::RefCell::new(None)};
}
struct RawReadGuard;
impl Drop for RawReadGuard {
    fn drop(&mut self) {
        RAW_READ.set(false);
        POLL_STATE.with_borrow_mut(|v| *v = None);
        SYNTHETIC_PACKET.with_borrow_mut(|v| *v = None);
    }
}
pub fn recv(reader: &mut ConnReader) -> Option<crate::Result<(i32, Vec<u8>)>> {
    if RAW_READ.get() {
        return None;
    }
    let session = session(&reader.counters)?;
    loop {
        if let Some(packet) = session.lock().unwrap_or_else(|e| e.into_inner()).poll() {
            return Some(packet);
        }
        // Re-enter the original reader once with this hook bypassed. This preserves
        // Horizon's framing, counters, buffered decryption and error handling, and
        // continues to work after Connection::split moves the reader to a thread.
        RAW_READ.set(true);
        if session
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .needs_event_poll()
        {
            POLL_STATE.with_borrow_mut(|v| *v = Some(session.clone()));
        }
        let guard = RawReadGuard;
        let packet = reader.recv();
        let synthetic = SYNTHETIC_PACKET.with_borrow_mut(Option::take);
        drop(guard);
        if let Some(packet) = synthetic {
            return Some(packet);
        }
        let (id, payload) = match packet {
            Ok(packet) => packet,
            Err(error) => return Some(Err(error)),
        };
        let translated = session
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .ingest(id, &payload);
        match translated {
            Ok(Some(packet)) => return Some(Ok(packet)),
            Ok(None) => continue,
            Err(error) => return Some(Err(crate::Error::new(error))),
        }
    }
}

// Only the translated receive call installs POLL_STATE. Interrupt the stream
// read via a thread-local packet slot; the original frame buffer and cipher
// remain untouched. All other reads, including native connections, fall through.
pub(super) fn read_with_events(
    stream: &mut super::Stream,
    output: &mut [u8],
) -> Option<std::io::Result<usize>> {
    let state = POLL_STATE.with_borrow(Clone::clone)?;
    let super::Stream::Tcp(socket) = stream else {
        return None;
    };
    Some((|| {
        use std::io::ErrorKind;
        let original = socket.read_timeout()?;
        struct Restore<'a>(&'a std::net::TcpStream, Option<Duration>);
        impl Drop for Restore<'_> {
            fn drop(&mut self) {
                let _ = self.0.set_read_timeout(self.1);
            }
        }
        let _restore = Restore(socket, original);
        let start = std::time::Instant::now();
        loop {
            if let Some(packet) = state.lock().unwrap_or_else(|e| e.into_inner()).poll() {
                SYNTHETIC_PACKET.with_borrow_mut(|v| *v = Some(packet));
                return Err(std::io::Error::from(ErrorKind::Interrupted));
            }
            let duration = match original {
                Some(timeout) => {
                    let remaining = timeout.saturating_sub(start.elapsed());
                    if remaining.is_zero() {
                        return Err(std::io::Error::from(ErrorKind::TimedOut));
                    }
                    remaining.min(Duration::from_millis(20))
                }
                None => Duration::from_millis(20),
            };
            socket.set_read_timeout(Some(duration))?;
            match std::io::Read::read(&mut &*socket, output) {
                Ok(size) => return Ok(size),
                Err(e)
                    if matches!(
                        e.kind(),
                        ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted
                    ) =>
                {
                    ()
                }
                Err(e) => return Err(e),
            }
        }
    })())
}
