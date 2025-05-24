use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, Sender, SyncSender},
    Arc,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use solana_sdk::{
    signature::Signature,
    transaction::{SanitizedTransaction, TransactionError},
};
use tracing::{debug, error, trace, warn};

/// Verification result for a single transaction
#[derive(Debug, Clone)]
pub struct VerifiedResult {
    pub index: usize,
    pub signature: Signature,
    pub result: Result<(), TransactionError>,
}

/// Transaction batch to be verified
#[derive(Debug)]
pub struct TransactionBatch {
    pub transactions: Vec<SanitizedTransaction>,
    pub batch_id: u64,
}

/// Offload executor for asynchronous transaction signature verification
pub struct OffloadExecutor {
    verification_tx: SyncSender<TransactionBatch>,
    verified_rx: Receiver<VerifiedResult>,
    stop_flag: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
    next_batch_id: std::sync::atomic::AtomicU64,
}

impl OffloadExecutor {
    /// Initialize the executor with default channel capacity
    pub fn new() -> Self {
        Self::new_with_capacity(1000)
    }

    /// Initialize the executor with specified channel capacity
    pub fn new_with_capacity(capacity: usize) -> Self {
        trace!("🟢 offload executor initialized with capacity: {}", capacity);
        
        let (verification_tx, verification_rx) = mpsc::sync_channel(capacity);
        let (verified_tx, verified_rx) = mpsc::channel();
        let stop_flag = Arc::new(AtomicBool::new(false));
        
        let verification_thread = Self::start_verification_thread(
            verification_rx,
            verified_tx,
            stop_flag.clone(),
        );

        Self {
            verification_tx,
            verified_rx,
            stop_flag,
            handle: Some(verification_thread),
            next_batch_id: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Queue a batch of transactions for verification
    /// Returns Err if the channel is full (caller should fall back to sync verification)
    pub fn queue_batch_for_verification(&self, transactions: Vec<SanitizedTransaction>) -> Result<(), Vec<SanitizedTransaction>> {
        let batch_id = self.next_batch_id.fetch_add(1, Ordering::Relaxed);
        let batch = TransactionBatch {
            transactions,
            batch_id,
        };

        debug!("Queuing batch {} with {} transactions", batch_id, batch.transactions.len());

        match self.verification_tx.try_send(batch) {
            Ok(()) => {
                trace!("Successfully queued batch {} for verification", batch_id);
                Ok(())
            }
            Err(mpsc::TrySendError::Full(batch)) => {
                warn!("Verification queue full, falling back to sync verification for batch {}", batch.batch_id);
                Err(batch.transactions)
            }
            Err(mpsc::TrySendError::Disconnected(_)) => {
                error!("Verification thread disconnected");
                Err(vec![]) // Return empty vec as transactions are lost
            }
        }
    }

    /// Poll for verified transaction results
    /// Returns up to max_count results within the timeout period
    pub fn poll_verified_transactions(&self, max_count: usize, timeout: Duration) -> Vec<VerifiedResult> {
        let mut results = Vec::new();
        let start = Instant::now();

        while results.len() < max_count && start.elapsed() < timeout {
            let remaining_timeout = timeout.saturating_sub(start.elapsed());
            
            match self.verified_rx.recv_timeout(remaining_timeout) {
                Ok(result) => {
                    trace!("Received verified result for transaction {}", result.signature);
                    results.push(result);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    debug!("Timeout waiting for verification results after {:?}", start.elapsed());
                    break;
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    error!("Verification result channel disconnected");
                    break;
                }
            }
        }

        debug!("Polled {} verification results in {:?}", results.len(), start.elapsed());
        results
    }

    /// Start the background verification thread
    fn start_verification_thread(
        verification_rx: mpsc::Receiver<TransactionBatch>,
        verified_tx: Sender<VerifiedResult>,
        stop_flag: Arc<AtomicBool>,
    ) -> JoinHandle<()> {
        thread::Builder::new()
            .name("solOffloadVerify".to_string())
            .spawn(move || {
                debug!("🚀 Verification thread started");
                
                while !stop_flag.load(Ordering::Relaxed) {
                    match verification_rx.recv_timeout(Duration::from_millis(100)) {
                        Ok(batch) => {
                            Self::process_verification_batch(batch, &verified_tx);
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            // Continue checking stop flag
                            continue;
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => {
                            debug!("Verification channel disconnected, shutting down thread");
                            break;
                        }
                    }
                }
                
                debug!("🛑 Verification thread stopped");
            })
            .expect("Failed to spawn verification thread")
    }

    /// Process a batch of transactions for verification
    fn process_verification_batch(batch: TransactionBatch, verified_tx: &Sender<VerifiedResult>) {
        let start = Instant::now();
        debug!("Processing verification batch {} with {} transactions", 
               batch.batch_id, batch.transactions.len());

        for (index, transaction) in batch.transactions.into_iter().enumerate() {
            let signature = *transaction.signature();
            
            // Perform signature verification
            let result = transaction.verify();
            
            let verified_result = VerifiedResult {
                index,
                signature,
                result,
            };

            if let Err(e) = verified_tx.send(verified_result) {
                error!("Failed to send verification result: {}", e);
                break;
            }
        }

        debug!("Completed verification batch {} in {:?}", 
               batch.batch_id, start.elapsed());
    }
}

impl Drop for OffloadExecutor {
    fn drop(&mut self) {
        debug!("Shutting down OffloadExecutor");
        
        // Signal the verification thread to stop
        self.stop_flag.store(true, Ordering::Relaxed);
        
        // Join the verification thread
        if let Some(handle) = self.handle.take() {
            if let Err(e) = handle.join() {
                error!("Failed to join verification thread: {:?}", e);
            } else {
                debug!("Verification thread shut down successfully");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_sdk::{
        hash::Hash,
        signature::{Keypair, Signer},
        system_instruction,
        transaction::Transaction,
    };
    use std::time::Duration;

    fn create_test_transaction() -> SanitizedTransaction {
        let keypair = Keypair::new();
        let to = Keypair::new();
        let instruction = system_instruction::transfer(&keypair.pubkey(), &to.pubkey(), 1);
        let transaction = Transaction::new_signed_with_payer(
            &[instruction],
            Some(&keypair.pubkey()),
            &[&keypair],
            Hash::default(),
        );
        SanitizedTransaction::try_from_legacy_transaction(transaction).unwrap()
    }

    #[test]
    fn test_offload_executor_new() {
        let _executor = OffloadExecutor::new();
        // Should not panic and should initialize successfully
    }

    #[test]
    fn test_queue_batch_for_verification() {
        let executor = OffloadExecutor::new_with_capacity(10);
        let transactions = vec![create_test_transaction()];
        
        let result = executor.queue_batch_for_verification(transactions);
        assert!(result.is_ok());
    }

    #[test]
    fn test_queue_full_fallback() {
        let executor = OffloadExecutor::new_with_capacity(1);
        
        // Fill the queue
        let tx1 = vec![create_test_transaction()];
        assert!(executor.queue_batch_for_verification(tx1).is_ok());
        
        // This should fail due to full queue
        let tx2 = vec![create_test_transaction()];
        let result = executor.queue_batch_for_verification(tx2);
        assert!(result.is_err());
    }

    #[test]
    fn test_poll_verified_transactions() {
        let executor = OffloadExecutor::new_with_capacity(10);
        let transactions = vec![create_test_transaction()];
        
        // Queue a transaction
        executor.queue_batch_for_verification(transactions).unwrap();
        
        // Poll for results
        let results = executor.poll_verified_transactions(1, Duration::from_millis(500));
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_poll_timeout() {
        let executor = OffloadExecutor::new_with_capacity(10);
        
        // Poll without queuing anything - should timeout
        let results = executor.poll_verified_transactions(1, Duration::from_millis(100));
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_verification_thread_lifecycle() {
        // Test that thread starts and stops properly
        {
            let executor = OffloadExecutor::new();
            // executor goes out of scope and Drop should be called
        }
        // If we reach here without hanging, Drop worked correctly
    }
}
