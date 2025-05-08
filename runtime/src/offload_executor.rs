use tracing::trace;

pub struct OffloadExecutor;

impl OffloadExecutor {
    /// Initialize the executor and emit a trace log.
    pub fn new() -> Self {
        trace!("🟢 offload executor initialized");
        Self
    }
}