use std::time::Duration;
use solana_sdk::{
    signature::{Keypair, Signer},
    transaction::{Transaction, SanitizedTransaction},
    hash::Hash,
    system_instruction,
};
use solana_runtime::offload_executor::OffloadExecutor;

fn make_test_transaction() -> SanitizedTransaction {
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

fn make_batch(n: usize) -> Vec<SanitizedTransaction> {
    (0..n).map(|_| make_test_transaction()).collect()
}

#[test]
fn basic_roundtrip_async() {
    let exec = OffloadExecutor::new_with_capacity(8);
    let batch = make_batch(3);
    
    exec.queue_batch_for_verification(batch).unwrap();
    let results = exec.poll_verified_transactions(3, Duration::from_secs(1));
    assert_eq!(results.len(), 3);
}

#[test]
fn fallback_on_queue_full() {
    let exec = OffloadExecutor::new_with_capacity(1);
    
    // Fill the queue
    exec.queue_batch_for_verification(make_batch(1)).unwrap();
    
    // This should fail and return the transactions for fallback
    let res = exec.queue_batch_for_verification(make_batch(1));
    assert!(res.is_err());
    
    // Poll the queued transaction
    let results = exec.poll_verified_transactions(1, Duration::from_secs(1));
    assert_eq!(results.len(), 1);
}

#[test]
fn multiple_batches() {
    let exec = OffloadExecutor::new_with_capacity(10);
    
    // Queue multiple batches
    exec.queue_batch_for_verification(make_batch(2)).unwrap();
    exec.queue_batch_for_verification(make_batch(3)).unwrap();
    
    // Poll all results
    let results = exec.poll_verified_transactions(5, Duration::from_secs(1));
    assert_eq!(results.len(), 5);
}

#[test]
fn timeout_behavior() {
    let exec = OffloadExecutor::new_with_capacity(10);
    
    // Poll without queuing anything - should timeout immediately
    let start = std::time::Instant::now();
    let results = exec.poll_verified_transactions(1, Duration::from_millis(100));
    let elapsed = start.elapsed();
    
    assert_eq!(results.len(), 0);
    assert!(elapsed >= Duration::from_millis(90)); // Allow some tolerance
    assert!(elapsed <= Duration::from_millis(200)); // But not too much
}
} 