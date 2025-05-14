use tracing::trace;

pub struct OffloadExecutor;

impl OffloadExecutor {
    /// Initialize the executor and emit a trace log.
    pub fn new() -> Self {
        trace!("🟢 offload executor initialized");
        Self
    }
}

// runtime/src/offload_executor.rs

// …既存コード…

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_offload_executor_traces() {
        // トレースレベルのログを出力させる
         std::env::set_var("RUST_LOG", "trace");
        tracing_subscriber::fmt::init();
        // これでコンソールに「🟢 offload executor initialized」が出る
        OffloadExecutor::new();
    }
}
