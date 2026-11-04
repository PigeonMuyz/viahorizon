// SPDX-License-Identifier: GPL-3.0-or-later
// Child of net: connection-owned capabilities, without the translator lock.
use horizon_protocol::connection::{ConnWriter, Counters};
use std::sync::{
    Arc, Mutex, Weak,
    atomic::{AtomicBool, AtomicUsize, Ordering},
    mpsc,
};
std::thread_local! {
    pub static SWORD_BLOCK_RENDER: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub fn full_items(session: &super::Session) -> bool {
    connection(session).is_some_and(|(_, state)| state.full_items())
}

pub fn legacy_protocol(session: &super::Session) -> Option<i32> {
    connection(session).and_then(|(_, state)| state.legacy_protocol())
}

pub fn unsequenced_blocks(session: &super::Session) -> bool {
    connection(session).is_some_and(|(_, state)| state.unsequenced_blocks())
}

fn connection(
    session: &super::Session,
) -> Option<(
    Arc<Counters>,
    Arc<horizon_protocol::vfp_session::PlayCapabilities>,
)> {
    if session
        .configuring
        .load(std::sync::atomic::Ordering::Relaxed)
    {
        return None;
    }
    let counters = session
        .counters
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone()?;
    let state = horizon_protocol::connection::vfp_bridge::capabilities(&counters)?;
    Some((counters, state))
}

const MAX_QUEUED_BYTES: usize = 4 * 1024 * 1024;
struct Outbox {
    owner: Weak<Mutex<Option<ConnWriter>>>,
    counters: Weak<Counters>,
    sender: mpsc::SyncSender<Vec<u8>>,
    queued_bytes: Arc<AtomicUsize>,
    failed: Arc<AtomicBool>,
}
impl Outbox {
    fn new(
        writer: &Arc<Mutex<Option<ConnWriter>>>,
        counters: &Arc<Counters>,
        configuring: Arc<AtomicBool>,
    ) -> std::io::Result<Self> {
        let (sender, receiver) = mpsc::sync_channel::<Vec<u8>>(1024);
        let owner = Arc::downgrade(writer);
        let identity = Arc::downgrade(counters);
        let queued_bytes = Arc::new(AtomicUsize::new(0));
        let failed = Arc::new(AtomicBool::new(false));
        let (worker_owner, worker_identity, bytes, fault) = (
            owner.clone(),
            identity.clone(),
            queued_bytes.clone(),
            failed.clone(),
        );
        std::thread::Builder::new()
            .name("vfp-outgoing".into())
            .spawn(move || {
                loop {
                    let body = match receiver.recv_timeout(std::time::Duration::from_millis(50)) {
                        Ok(body) => body,
                        Err(mpsc::RecvTimeoutError::Timeout) if worker_owner.strong_count() > 0 => {
                            if fault.load(Ordering::Acquire) {
                                Vec::new()
                            } else {
                                continue;
                            }
                        }
                        Err(_) => break,
                    };
                    bytes.fetch_sub(body.len(), Ordering::Relaxed);
                    let (Some(owner), Some(counters)) =
                        (worker_owner.upgrade(), worker_identity.upgrade())
                    else {
                        break;
                    };
                    let mut guard = owner.lock().unwrap_or_else(|error| error.into_inner());
                    let Some(writer) = guard.as_mut() else { break };
                    if !horizon_protocol::connection::vfp_bridge::owns(writer, &counters) {
                        break; // A transfer/reconnect must never receive an old queue.
                    }
                    if fault.load(Ordering::Acquire) {
                        writer.shutdown();
                        break;
                    }
                    if configuring.load(Ordering::Acquire) {
                        continue;
                    }
                    if writer.send(&body).is_err() {
                        writer.shutdown();
                        break;
                    }
                }
            })?;
        Ok(Self {
            owner,
            counters: identity,
            sender,
            queued_bytes,
            failed,
        })
    }
    fn push(&self, body: Vec<u8>) {
        let size = body.len();
        if self
            .queued_bytes
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |bytes| {
                bytes
                    .checked_add(size)
                    .filter(|&bytes| bytes <= MAX_QUEUED_BYTES)
            })
            .is_err()
        {
            self.failed.store(true, Ordering::Release);
            eprintln!("ViaHorizon: outgoing queue exceeded its byte bound");
            return;
        }
        if self.sender.try_send(body).is_err() {
            self.queued_bytes.fetch_sub(size, Ordering::Relaxed);
            self.failed.store(true, Ordering::Release);
            eprintln!("ViaHorizon: outgoing queue is full or disconnected");
        }
    }
}

#[cfg(test)]
mod outbox_tests {
    use super::*;
    use horizon_protocol::connection::{Connection, Pipe};
    #[test]
    fn a_busy_writer_does_not_block_input_and_queued_packets_keep_their_order() {
        let (a, b) = Pipe::pair();
        let connection = Connection::local(a);
        let counters = connection.reader.counters.clone();
        let writer = Arc::new(Mutex::new(Some(connection.writer)));
        let held = writer.lock().unwrap();
        let outbox = Outbox::new(&writer, &counters, Arc::new(AtomicBool::new(false))).unwrap();
        outbox.push(vec![42, 1]);
        outbox.push(vec![42, 2]);
        let mut peer = Connection::local(b);
        peer.reader
            .set_read_timeout(Some(std::time::Duration::from_millis(20)))
            .unwrap();
        assert!(peer.recv().is_err()); // Neither packet can pass the held writer.
        drop(held);
        peer.reader
            .set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .unwrap();
        assert_eq!(peer.recv().unwrap(), (42, vec![1]));
        assert_eq!(peer.recv().unwrap(), (42, vec![2]));
    }
    #[test]
    fn a_replaced_connection_never_receives_the_previous_connections_queue() {
        let (a, _old_peer) = Pipe::pair();
        let connection = Connection::local(a);
        let counters = connection.reader.counters.clone();
        let writer = Arc::new(Mutex::new(Some(connection.writer)));
        let mut held = writer.lock().unwrap();
        let outbox = Outbox::new(&writer, &counters, Arc::new(AtomicBool::new(false))).unwrap();
        outbox.push(vec![42, 1]);
        let (new_pipe, peer_pipe) = Pipe::pair();
        let replacement = Connection::local(new_pipe);
        *held = Some(replacement.writer);
        drop(held);
        let mut peer = Connection::local(peer_pipe);
        peer.reader
            .set_read_timeout(Some(std::time::Duration::from_millis(100)))
            .unwrap();
        assert!(peer.recv().is_err());
    }
    #[test]
    fn an_overloaded_queue_is_bounded_and_closes_instead_of_dropping_actions_silently() {
        let (a, b) = Pipe::pair();
        let connection = Connection::local(a);
        let counters = connection.reader.counters.clone();
        let writer = Arc::new(Mutex::new(Some(connection.writer)));
        let outbox = Outbox::new(&writer, &counters, Arc::new(AtomicBool::new(false))).unwrap();
        outbox.push(vec![42; MAX_QUEUED_BYTES + 1]);
        assert!(outbox.failed.load(Ordering::Acquire));
        assert_eq!(outbox.queued_bytes.load(Ordering::Relaxed), 0);
        let mut peer = Connection::local(b);
        peer.reader
            .set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .unwrap();
        let error = peer.recv().unwrap_err().to_string();
        assert!(!error.to_lowercase().contains("timed out"), "{error}");
    }
}
std::thread_local! {
    static OUTBOX: std::cell::RefCell<Option<Outbox>> = const { std::cell::RefCell::new(None) };
}
pub fn enqueue_legacy(
    session: &super::Session,
    packet: &horizon_protocol::packets::play::Serverbound,
) -> bool {
    let Some((counters, state)) = connection(session) else {
        return false;
    };
    if !state.unsequenced_blocks() {
        return false;
    }
    // Chunk translation can hold the protocol mutex for hundreds of ms.
    // The render thread only queues bytes; this worker owns ordered sends.
    OUTBOX.with_borrow_mut(|slot| {
        let same = slot.as_ref().is_some_and(|outbox| {
            outbox.owner.ptr_eq(&Arc::downgrade(&session.writer))
                && outbox.counters.ptr_eq(&Arc::downgrade(&counters))
        });
        if !same {
            match Outbox::new(&session.writer, &counters, session.configuring.clone()) {
                Ok(outbox) => *slot = Some(outbox),
                Err(error) => {
                    eprintln!("ViaHorizon: could not start outgoing worker: {error}");
                    return false;
                }
            }
        }
        slot.as_ref().unwrap().push(packet.encode());
        true
    })
}

pub fn legacy_sword(item: &str) -> bool {
    matches!(
        item,
        "minecraft:wooden_sword"
            | "minecraft:stone_sword"
            | "minecraft:iron_sword"
            | "minecraft:golden_sword"
            | "minecraft:diamond_sword"
    )
}
