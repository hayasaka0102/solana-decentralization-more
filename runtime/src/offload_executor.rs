use log::trace;

pub struct OffloadExecutor;

impl OffloadExecutor {
    pub fn new() -> Self {
        trace!("🟢 offload executor initialized");
        Self
    }
}
