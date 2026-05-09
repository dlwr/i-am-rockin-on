# 残タスク

このセッション（〜2026-05-09）でやり残したものの記録。最終 PR レビューで指摘された v1.5 4項目（per-article エラー許容 / fetch_and_extract 一貫化 / graceful shutdown / health endpoint）と Spotify URI / Bandcamp 正規化 / RSS 切替は実施済み。以下は未着手。

## 中優先度（運用上、いずれ噛む）

- [ ] **Spotify Search のクエリエスケープ**
  - `format!("artist:\"{}\" album:\"{}\"", artist, album)` で `"` や `\` が混じると Spotify 側でクエリ崩壊し silent miss。アーティスト名に `"` を含むケースは稀だが Bandcamp タイトル等で起こりうる。
  - 場所: `src/server/resolver/spotify.rs:71, 92`
  - 対処: 入力から `"` `\` を除去するか、`%22` などにエスケープ

- [ ] **スクレイプで 0件抽出時の警告**
  - 候補は N件あるのに oshi が 0件のままなら、Ameblo の RSS 構造変化を疑う早期警告ログを出す。
  - 場所: `src/server/scrape.rs run_inner` 後段
  - 対処: `if outcome.items_added + outcome.items_updated == 0 && candidates_count > 5 { tracing::error!(...) }`

- [ ] **SQLite バックアップ自動化**
  - 現状 fly volume の単一スナップショット（fly が標準で日次取得）以外なし。
  - 対処候補: `litestream` で S3/R2 へ継続レプリケーション、または cron で `sqlite3 .backup` → 外部ストレージ転送
  - リスク: volume 障害時の RPO が大きい（最悪1日分のデータ消失）

- [ ] **`/readyz` を `/healthz` と分離**
  - 現状 `/healthz` は固定で `ok` を返すだけ。プロセス生存しか確認できん。
  - 対処: `/readyz` で DB ping (`SELECT 1`) を打つ。fly check は引き続き `/healthz`。`/readyz` は外部監視用。

## 低優先度（コード品質・整備）

- [ ] **800ms スリープを Config field 化**
  - 場所: `src/server/scrape.rs:66` ハードコード
  - 対処: `Config` に `scrape_throttle_ms: u64` を追加、`run_inner` で参照
  - メリット: 検証時に短縮可、メディアごとに調整可

- [ ] **`Regex::new(...)` の毎回コンパイル削減**
  - 場所: `rokinon.rs::detect_oshi`、`entry_id_from_link`
  - 対処: `std::sync::LazyLock<Regex>` でモジュールトップに上げる

- [ ] **`ScrapePipeline::new()` コンストラクタ追加**
  - 場所: `src/server/scrape.rs`
  - 現状: `pub` field を直接指定する struct literal で構築。リファクタ耐性が弱い。
  - 対処: `pub fn new(source, resolver, repo, log) -> Self` を切る

- [ ] **cron への `CancellationToken` 伝播**
  - 現状: `axum::serve` は graceful shutdown 配線済みだが、`tokio_cron_scheduler` 起動した裏タスクは runtime drop で abort。スクレイプ中の SIGTERM でトランザクション中断の可能性。
  - 場所: `src/server/scheduler.rs`、`src/server/scrape.rs`
  - 対処: `tokio_util::sync::CancellationToken` を pipeline に渡し、各 candidate の前後で `is_cancelled()` チェック

- [ ] **SQLx offline metadata の CI チェック**
  - 現状: `.sqlx/` を手動で `cargo sqlx prepare` 更新。クエリ変更時の更新忘れに気付けん。
  - 対処: GitHub Actions で `cargo sqlx prepare --check` を走らせる

## テスト追加

- [ ] **`RecommendationRepo::upsert` の `manual_override` 保護パス**
  - 場所: `src/server/store.rs:37-58` の分岐が現在テストなし
  - 対処: `manual_override=1` の row を手動で SQL insert、upsert 呼んで spotify_url が保たれることを確認

- [ ] **`list_recent` の 3件以上での順序テスト**
  - 現状の単件・2件テストだけだと偶然パスする可能性あり（CLAUDE.md のルール）
  - 対処: 異なる featured_at の3件を入れて、降順で返ることを確認

- [ ] **pipeline で Spotify resolver が Err を返す時の継続**
  - 現状: `Ok(None)` のテストはあるが `Err` のテストなし
  - 対処: wiremock で 500 を返す Mock を仕込んで、items_added がインクリメントされる（Spotify 抜きで保存される）ことを確認

## 機能拡張（要相談）

- [ ] **RSS から落ちた古い「推し」のバックフィル**
  - 現状: RSS は最新10件のみ。月次で 推し を5件以上出されると、cron 1日1回でも捌き切れず古いものが取りこぼされる可能性。
  - 案A: 過去 entrylist (`entrylist-2.html`, `-3.html`...) を初回バックフィル時のみ走査
  - 案B: 既知の `source_external_id` 全件を週次で再フェッチ
  - 案C: 月次でアーカイブページを走査
  - 検討: ブログの実際の更新ペース次第（今のとこ問題ない）

- [ ] **他メディアソース追加**
  - `MediaSource` trait は trait object 化されとるけぇ、新メディア追加は impl 1つ + `main.rs` の選択分岐 1行。
  - 候補例: 他の音楽批評ブログ、Bandcamp Daily、Pitchfork BNM の RSS、etc.

- [ ] **メディア／月で絞れる UI フィルタ**
  - 現状: 100件の単一グリッドのみ
  - 場所: `src/pages/home.rs`、route 追加が必要
  - 対処: `?source=rokinon&month=2026-05` クエリパラメータ + サーバ関数で WHERE 追加

- [ ] **manual_override 切替 UI**
  - DB スキーマには field あるが、操作 UI なし。要 admin auth。
  - もしくはこの field を削除して将来必要になったら入れ直す（review でも指摘済み）

- [ ] **RSS / JSON フィード提供**
  - 集約サイトとしては自前の RSS / JSON を出すと再利用しやすい
  - 場所: `src/main.rs` に `axum::routing::get("/feed.xml", ...)` 追加

## 確実な不要候補（必要なら削除して整理）

- `manual_override` カラム — 使う UI がないなら一旦消すのも選択肢（YAGNI）
