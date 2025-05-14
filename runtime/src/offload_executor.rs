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
    use tracing_subscriber;  // ← 追加
    #[test]
    fn test_offload_executor_traces() {
        // trace! マクロの出力を有効化
        tracing_subscriber::fmt().with_env_filter("trace").init();
        // これでコンソールに「🟢 offload executor initialized」が出る
        OffloadExecutor::new();
    }
}
