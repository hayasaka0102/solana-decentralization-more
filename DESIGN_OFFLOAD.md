docs: add offload design draft
1. 背景とゴール
項目	内容
課題	Bank の verify_transaction() で署名検証がボトルネックになり、単一 CPU コアに依存すると TPS が頭打ちになる。
狙い	1. ローカル CPU スレッドプール
2. ローカル GPU カーネル
3. ネットワーク越しスマホ端末
で署名検証を “バッチ offload” し、検証レイテンシを隠蔽・並列化する。
完了条件 (MVP)	Full Verification 経路を通るトランザクションが必ず OffloadExecutor に流れ、戻り値のハッシュ整合性が保たれる。Validator ログに
offload start: → offload done: が出る。

2. アーキテクチャ全体図
scss
コピーする
編集する
┌────────────┐
│  Bank      │
│ verify_tx  │
└────┬───────┘
     │  (1) FullVerification
     ▼
┌────────────┐
│ OffloadExecutor       │
│  • CPU thread pool    │
│  • (opt) GPU queue    │
│  • (fut) Mobile peers │
└────┬───────┘
     │ (2) distribute_batch()
     ▼
┌──────────────┐
│ Worker Trait │───┐
└──────────────┘   │
   ▲        ▲      │
   │CPU impl│GPU impl …スマホ impl
Bank は FullVerification 経路の場合のみ verify_and_hash_message_offload() を呼ぶ。

OffloadExecutor は受け取った Vec<VersionedTransaction> を Worker 実装へバッチ送出。

Worker が Ok(()) を返したら Bank に戻し、hash_transaction_offload() でハッシュ生成。

3. 主要コンポーネント
名称	役割	重要メソッド
DistributableValidation trait	既存 VersionedTransaction へトレイト実装。オフロード経路用ラッパ。	verify_and_hash_message_offload()
hash_transaction_offload()
OffloadExecutor	バッチ分配器。CPU ThreadPool / GPU / RemotePeer を抽象化。	verify_signatures_threaded()
available_peers()
distribute_batch()
CpuWorker	rayon thread-pool で署名検証を並列実行。	fn verify_batch(&self, txs:&[...])
GpuWorker（Phase-2）	CUDA/OpenCL へバッチ転送。	同上
RemoteMobileWorker（Phase-3）	ActivityPub 経由で端末へ POST。	register_peer() / heartbeat

4. データフロー詳細（MVP: CPU スレッドプール）
Bank::verify_transaction

rust
コピーする
編集する
if verification_mode == FullVerification {
    tx.verify_and_hash_message_offload()?;   // ⚡ CPU スレッドプール
}
let message_hash = tx.hash_transaction_offload();
OffloadExecutor::verify_signatures_threaded

入力をチャンク化（例 64 件）

rayon::scope で並列 tx.verify()

any エラー→即時 Err(TransactionError::SignatureFailure)

戻り値

成功時 Ok(())：Bank はハッシュを生成して SanitizedTransaction を返す。

5. エラーハンドリング
レイヤ	シナリオ	Bank への戻り値	ログ
Executor	ThreadPool panic / GPU OOM	TransactionError::SanitizeFailure	error! offload failed: …
Worker	個別 tx 署名不整合	Err(SignatureFailure)	warn! sigverify fail tx={sig}
Mobile	タイムアウト (>1 s)	Executor でローカルフォールバック再実行	warn! peer {id} timeout, fallback CPU

6. ロギング指針
text
コピーする
編集する
TRACE solana_runtime::bank: 🔶 verify_transaction mode: {mode}
TRACE solana_runtime::offload_executor: 🟢 start batch, size={n}
TRACE solana_runtime::offload_executor: 🔵 done batch, ok={cnt}/{n}
WARN  solana_runtime::offload_executor: ⚠️ fallback to CPU, peer={id}, err={e}
ERROR solana_runtime::offload_executor: 🔴 executor panic: {e}
RUST_LOG=solana_runtime::offload_executor=trace で上記がすべて出る。

7. 開発フェーズとマイルストーン
フェーズ	目標	PR 単位
Phase-0	Trait + Executor スタブ / ログだけ通る	①
Phase-1	CPU ThreadPool 完全動作 (rayon)	② ③
Phase-2	GPU Worker (CUDA via rust-cuda or cust)	④
Phase-3	RemoteMobileWorker モック + ActivityPub API	⑤ ⑥
Phase-4	ベンチ & TPS 計測、Failover チューニング	⑦

8. テスト戦略
Unit: CpuWorker::verify_batch → 署名改ざんで fail を検証

Integration: solana-test-validator + 改造 CLI で送金 → TRACE が出ること

Criterion Bench: Master vs Offload で verify_transaction() throughput 比較

9. 既知の課題 / TODO
GPU カーネルの sigverify 実装は rust-cuda に載せ替える可能性

ActivityPub 経由のスマホ報告はネットワークレイテンシ隠蔽が課題

オフロード先が 0 の時は必ず CPU フォールバック

リプレイ保護のためバッチ順序維持が必要（現在は順序保証なし）

10. 用語集
用語	意味
FullVerification	Bank が署名検証を必ず行うモード
DistributableValidation	署名検証 + メッセージハッシュをオフロード対応に拡張するトレイト
Worker	OffloadExecutor からバッチを受け取って検証する実体 (CPU/GPU/Mobile)
Peer	ネットワーク越しのスマホまたは外部 GPU サービス
