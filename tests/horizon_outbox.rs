// Compile the actual injected net helper against a minimal Session shape;
// its worker uses the real protocol writer, framing and memory transport.
#[allow(dead_code)]
mod net {
    use horizon_protocol::connection::{ConnWriter, Counters};
    use std::sync::{Arc, Mutex, atomic::AtomicBool};
    pub struct Session {
        writer: Arc<Mutex<Option<ConnWriter>>>,
        counters: Arc<Mutex<Option<Arc<Counters>>>>,
        configuring: Arc<AtomicBool>,
    }
    pub mod vfp_capture_session {
        include!("data/vfp_capture_session.rs");
    }
}
