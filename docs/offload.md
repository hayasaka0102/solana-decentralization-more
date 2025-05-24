# OffloadExecutor: Solana署名検証オフロード機構

## 概要

`OffloadExecutor`は、Solanaのトランザクション署名検証処理をバックグラウンドスレッドにオフロードし、バッチ検証によるパフォーマンス向上やメインスレッド負荷分散を実現するための仕組みです。

- **主な用途**: トランザクションバッチの署名検証を非同期で処理し、検証済み結果をBankへ返却
- **feature-gate**: `offload_signature_verification` featureで有効/無効を切り替え可能
- **フォールバック**: チャネル満杯やfeature無効時は従来の同期検証に自動フォールバック

---

## 構造とAPI

### 構造体
```rust
pub struct OffloadExecutor<'a, 'b> {
    verification_tx: Sender<TransactionBatch<'a, 'b>>,
    verified_rx: Receiver<VerifiedResult>,
    stop_flag: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}
```

### 主なAPI
- `new(...)` : バックグラウンド検証スレッドを起動し、チャネルを初期化
- `queue_batch_for_verification(batch)` : トランザクションバッチを検証キューに送信。チャネル満杯時はErrを返す
- `poll_verified_transactions(max_count, timeout)` : 検証済み結果を最大max_count件、タイムアウト付きで受信
- `Drop` : スレッドの安全な停止・クリーンアップ

#### VerifiedResult型
```rust
pub struct VerifiedResult {
    pub index: usize,
    pub signature: Signature,
    pub result: Result<(), TransactionError>,
}
```

---

## スレッドモデル

- `OffloadExecutor::new` で検証スレッドをspawn
- メインスレッドは `queue_batch_for_verification` でバッチを送信
- バックグラウンドスレッドは `verify_transactions` (Solana標準) で一括検証し、結果を `verified_rx` チャネルへ送信
- `poll_verified_transactions` でBank側が結果を回収
- Drop時に `stop_flag` を立ててスレッドを安全にJoin

---

## feature-gateによる制御

- `runtime/src/feature_set.rs` でfeatureを定義
    ```rust
    solana_sdk::declare_feature!(
        "12345678-1234-1234-1234-123456789abc", // 仮のPubkey
        offload_signature_verification
    );
    ```
- `runtime/src/bank.rs` でfeature有効時のみ `OffloadExecutor` を利用
- チャネル満杯やfeature無効時は自動的に同期検証へフォールバック

---

## 利用例

```rust
// OffloadExecutorの初期化
let (verification_tx, verification_rx) = std::sync::mpsc::channel();
let (verified_tx, verified_rx) = std::sync::mpsc::channel();
let executor = OffloadExecutor::new(verification_tx, verification_rx, verified_tx, verified_rx);

// バッチをキューイング
executor.queue_batch_for_verification(batch)?;

// 検証済み結果を取得
let results = executor.poll_verified_transactions(batch.sanitized_transactions().len(), std::time::Duration::from_millis(100));
```

---

## 設計上の注意点・判断理由

- **バッチ検証**: `verify_transactions` (Solana公式) を利用し、個別ループより高速
- **チャネル容量**: 満杯時は即座にErrを返し、Bank側で同期検証に切り替え
- **スレッド安全性**: Dropで停止フラグとJoinを徹底し、リソースリークを防止
- **APIのResult型**: エラー時のフォールバックや障害検知を容易に
- **ユニットテスト**: 独立したチャネル・ダミーバッチで正常系を網羅

---

## 他の開発者への注意点

- チャネルサイズやスレッド数は用途・負荷に応じて調整してください
- feature-gateの有効化には正しいfeatureセット・Pubkeyが必要です
- Drop実装により、明示的なクリーンアップ不要ですが、Bank等のライフサイクルに注意
- バッチサイズやタイムアウト値はワークロードに応じて最適化してください

---

## 参考: ユニットテスト例

`runtime/src/offload_executor.rs` 末尾の `#[cfg(test)] mod tests` を参照してください。

---

## 今後の拡張案
- スレッドプール化や非同期チャネルへの置き換え
- 検証失敗時の詳細なエラー通知
- メトリクス・トレースの強化 