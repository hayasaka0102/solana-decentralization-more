use tracing::trace;
use std::time::Duration;
use serde::{Deserialize, Serialize};
use solana_sdk::{
    signature::Signature,
    transaction::{VersionedTransaction, TransactionError},
    hash::Hash,
};

/// Feature flag to enable/disable smartphone verification globally
pub static SMARTPHONE_VERIFICATION_ENABLED: std::sync::atomic::AtomicBool = 
    std::sync::atomic::AtomicBool::new(false);

/// Enable smartphone verification globally
pub fn enable_smartphone_verification_feature() {
    SMARTPHONE_VERIFICATION_ENABLED.store(true, std::sync::atomic::Ordering::Relaxed);
    trace!("🔧 Smartphone verification feature enabled globally");
}

/// Disable smartphone verification globally
pub fn disable_smartphone_verification_feature() {
    SMARTPHONE_VERIFICATION_ENABLED.store(false, std::sync::atomic::Ordering::Relaxed);
    trace!("🔧 Smartphone verification feature disabled globally");
}

/// Check if smartphone verification feature is enabled globally
pub fn is_smartphone_verification_feature_enabled() -> bool {
    SMARTPHONE_VERIFICATION_ENABLED.load(std::sync::atomic::Ordering::Relaxed)
}

/// Unique ticket to track verification requests sent to smartphones
#[derive(Debug, Clone)]
pub struct VerificationTicket {
    /// Unique request ID
    pub request_id: u64,
    /// Timestamp when request was sent
    pub created_at: std::time::Instant,
    /// Transaction hash for verification
    pub transaction_hash: Hash,
    /// Smartphone device ID that received the request
    pub device_id: Option<String>,
}

impl PartialEq for VerificationTicket {
    fn eq(&self, other: &Self) -> bool {
        self.request_id == other.request_id
            && self.transaction_hash == other.transaction_hash
            && self.device_id == other.device_id
    }
}

impl Eq for VerificationTicket {}

impl std::hash::Hash for VerificationTicket {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.request_id.hash(state);
        self.transaction_hash.hash(state);
        self.device_id.hash(state);
    }
}

impl Default for VerificationTicket {
    fn default() -> Self {
        Self {
            request_id: 0,
            created_at: std::time::Instant::now(),
            transaction_hash: Hash::default(),
            device_id: None,
        }
    }
}

/// Response from smartphone verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartphoneVerificationResponse {
    /// Request ID this response corresponds to
    pub request_id: u64,
    /// Verification result
    pub result: Result<bool, String>,
    /// Smartphone's signature on the response
    pub response_signature: Option<Signature>,
    /// Device ID
    pub device_id: String,
    /// Timestamp of verification
    pub verified_at: u64,
}

/// Manages signature verification requests to smartphones
#[derive(Debug)]
pub struct SmartphoneVerifier {
    /// Base URL for smartphone communication
    pub endpoint_url: String,
    /// HTTP client for communication
    pub client: reqwest::Client,
    /// Request timeout duration
    pub timeout: Duration,
    /// Counter for generating unique request IDs
    request_counter: std::sync::atomic::AtomicU64,
}

impl Clone for SmartphoneVerifier {
    fn clone(&self) -> Self {
        Self {
            endpoint_url: self.endpoint_url.clone(),
            client: reqwest::Client::new(), // Create new client instance
            timeout: self.timeout,
            request_counter: std::sync::atomic::AtomicU64::new(
                self.request_counter.load(std::sync::atomic::Ordering::SeqCst)
            ),
        }
    }
}

impl SmartphoneVerifier {
    /// Create a new smartphone verifier
    pub fn new(endpoint_url: String) -> Self {
        Self {
            endpoint_url,
            client: reqwest::Client::new(),
            timeout: Duration::from_secs(5), // 5 second timeout
            request_counter: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Send a transaction to smartphone for verification
    pub async fn send_for_verification(&self, tx: &VersionedTransaction) -> Result<VerificationTicket, String> {
        let request_id = self.request_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let transaction_hash = tx.message.hash();
        
        trace!("📱 sending verification request {} to smartphone", request_id);
        
        let ticket = VerificationTicket {
            request_id,
            created_at: std::time::Instant::now(),
            transaction_hash,
            device_id: None, // Will be filled when we know which device responds
        };

        // For now, simulate sending the request
        // TODO: Implement actual HTTP request to smartphone
        let _request_payload = serde_json::json!({
            "request_id": request_id,
            "transaction": tx,
            "timestamp": ticket.created_at.elapsed().as_millis()
        });

        // TODO: Implement actual HTTP POST to smartphone
        // let response = self.client
        //     .post(&format!("{}/verify", self.endpoint_url))
        //     .json(&request_payload)
        //     .timeout(self.timeout)
        //     .send()
        //     .await
        //     .map_err(|e| format!("Failed to send request: {}", e))?;

        Ok(ticket)
    }

    /// Poll for verification result from smartphone
    pub async fn poll_result(&self, ticket: &VerificationTicket) -> Option<Result<bool, TransactionError>> {
        trace!("📱 polling result for request {}", ticket.request_id);
        
        // Check if request has timed out
        if ticket.created_at.elapsed() > self.timeout {
            trace!("⏰ verification request {} timed out", ticket.request_id);
            return Some(Err(TransactionError::SignatureFailure));
        }

        // TODO: Implement actual HTTP polling
        // For now, simulate a response after 1 second
        if ticket.created_at.elapsed() > Duration::from_millis(1000) {
            // Simulate successful verification
            trace!("✅ smartphone verification successful for request {}", ticket.request_id);
            return Some(Ok(true));
        }

        None // Still pending
    }

    /// Check multiple verification tickets at once
    pub async fn poll_multiple(&self, tickets: &[VerificationTicket]) -> Vec<(usize, Result<bool, TransactionError>)> {
        let mut results = Vec::new();
        
        for (index, ticket) in tickets.iter().enumerate() {
            if let Some(result) = self.poll_result(ticket).await {
                results.push((index, result));
            }
        }
        
        results
    }
}

#[derive(Debug, Clone)]
pub struct OffloadExecutor {
    /// Smartphone verifier for offloading signature verification
    pub smartphone_verifier: Option<SmartphoneVerifier>,
}

impl OffloadExecutor {
    /// Initialize the executor and emit a trace log.
    pub fn new() -> Self {
        trace!("🟢 offload executor initialized");
        Self {
            smartphone_verifier: None,
        }
    }

    /// Create executor with smartphone verification capability
    pub fn with_smartphone_verifier(smartphone_endpoint: String) -> Self {
        trace!("🟢 offload executor initialized with smartphone verifier");
        Self {
            smartphone_verifier: Some(SmartphoneVerifier::new(smartphone_endpoint)),
        }
    }

    /// Check if smartphone verification is available
    pub fn has_smartphone_verifier(&self) -> bool {
        self.smartphone_verifier.is_some()
    }

    /// Verify transaction using smartphone, with CPU fallback
    pub fn verify_transaction_with_smartphone(&self, tx: &VersionedTransaction) -> Result<Hash, TransactionError> {
        if let Some(ref verifier) = self.smartphone_verifier {
            // Try smartphone verification with timeout
            match self.try_smartphone_verification(verifier, tx) {
                Ok(hash) => {
                    trace!("📱 smartphone verification successful");
                    Ok(hash)
                }
                Err(e) => {
                    trace!("📱 smartphone verification failed: {:?}, falling back to CPU", e);
                    // Fallback to CPU verification
                    tx.verify_and_hash_message()
                }
            }
        } else {
            // No smartphone verifier, use CPU verification directly
            tx.verify_and_hash_message()
        }
    }

    /// Attempt smartphone verification with timeout
    fn try_smartphone_verification(&self, _verifier: &SmartphoneVerifier, tx: &VersionedTransaction) -> Result<Hash, String> {
        // For now, simulate smartphone verification
        // In a real implementation, this would:
        // 1. Send verification request to smartphone
        // 2. Wait for response with timeout
        // 3. Return verified hash or error
        
        // Mock implementation: simulate 50ms delay and success
        std::thread::sleep(Duration::from_millis(50));
        
        // Check transaction format is valid
        if tx.signatures.is_empty() {
            return Err("No signatures in transaction".to_string());
        }
        
        trace!("📱 smartphone mock verification completed successfully");
        Ok(tx.message.hash())
    }
}

// runtime/src/offload_executor.rs

// …既存コード…

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio;

    #[test]
    fn test_offload_executor_traces() {
        // トレースレベルのログを出力させる
        std::env::set_var("RUST_LOG", "trace");
        tracing_subscriber::fmt::init();
        // これでコンソールに「🟢 offload executor initialized」が出る
        OffloadExecutor::new();
    }

    #[test]
    fn test_offload_executor_with_smartphone_verifier() {
        // スマホ検証機能付きのExecutorを作成
        let executor = OffloadExecutor::with_smartphone_verifier("http://localhost:8080".to_string());
        assert!(executor.has_smartphone_verifier());
    }

    #[test]
    fn test_verification_ticket_creation() {
        let ticket = VerificationTicket {
            request_id: 1,
            created_at: std::time::Instant::now(),
            transaction_hash: Hash::new_unique(),
            device_id: Some("test-device".to_string()),
        };
        
        assert_eq!(ticket.request_id, 1);
        assert!(ticket.device_id.is_some());
        assert_eq!(ticket.device_id.unwrap(), "test-device");
    }

    #[tokio::test]
    async fn test_smartphone_verifier_basic() {
        let verifier = SmartphoneVerifier::new("http://localhost:8080".to_string());
        
        // テスト用のVersionedTransactionを作成
        use solana_sdk::{
            message::{Message},
            transaction::{Transaction, VersionedTransaction},
            signature::{Keypair, Signer},
            system_instruction,
            pubkey::Pubkey,
        };
        
        let from_keypair = Keypair::new();
        let to_pubkey = Pubkey::new_unique();
        let instruction = system_instruction::transfer(&from_keypair.pubkey(), &to_pubkey, 1000);
        let message = Message::new(&[instruction], Some(&from_keypair.pubkey()));
        let transaction = Transaction::new(&[&from_keypair], message, Hash::new_unique());
        let versioned_tx = VersionedTransaction::from(transaction);
        
        // 検証リクエストを送信
        let ticket = verifier.send_for_verification(&versioned_tx).await;
        assert!(ticket.is_ok());
        
        let ticket = ticket.unwrap();
        assert_eq!(ticket.request_id, 0); // First request should have ID 0
        
        // 結果をポーリング（まだ結果が無いはず）
        let result = verifier.poll_result(&ticket).await;
        assert!(result.is_none()); // Still pending
        
        // 1秒待って再度ポーリング（モック実装では1秒後に成功レスポンス）
        tokio::time::sleep(Duration::from_millis(1100)).await;
        let result = verifier.poll_result(&ticket).await;
        assert!(result.is_some());
        let result = result.unwrap();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), true);
    }
}
