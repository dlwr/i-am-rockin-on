# 残タスク

このセッション（〜2026-05-09）でやり残したものの記録。最終 PR レビューで指摘された v1.5 4項目（per-article エラー許容 / fetch_and_extract 一貫化 / graceful shutdown / health endpoint）と Spotify URI / Bandcamp 正規化 / RSS 切替は実施済み。以下は未着手。

## Pitchfork ＋ カードマージセッション（2026-05-10）の引き継ぎ

Pitchfork ソース追加（スコア 8.0+ 直近 90 日）、 同一アルバムの Rokinon＋Pitchfork カードマージ、 Spotify 未配信アルバムの skip までデプロイ済み（PR #1, #2）。 以下は残り。

### 中優先度

- [ ] **Pitchfork レビューページ実機 HTML を fixture に保存**
  - 現状の `tests/fixtures/pitchfork/review_*.html` は手作りの最小構造。 検証で「fixture と実機構造のズレ」（artists 配下に genres ネスト無し、 musicRating の path 違い等）を踏んだ。 1 ページだけでも実機 HTML を `tests/fixtures/pitchfork/realistic_full.html` に保存し、 既存テストとは別の 1 ケースで読み込むと regression 化できる
  - 場所: `src/server/adapter/pitchfork.rs::tests`
  - 対処: `curl` でスナップ取得 → fixture コミット → `extract_score`/`extract_artist`/`extract_album`/`extract_publish_date` を全部食わせる test 1 件追加

- [x] **Spotify Search のクエリエスケープ** — `sanitize_query_value` で `"` と `\` を除去してから field filter に渡す。 wiremock で「サニタイズ後のクエリと厳密一致」を検証する test 追加（PR #8）

- [ ] **Spotify が同一アルバムを別 ID で返す時の merge 戦略**
  - dedup key は `spotify_url` 完全一致が第一候補。 Rokinon と Pitchfork で別エディション（deluxe / 通常）に当たって別 spotify_url が返ると、 同一作品が 2 カードに分かれる
  - 頻度未測定。 v1 では artist+album normalized で fallback dedup できとるが、 両方とも spotify_url 持っとる場合に fallback は効かん
  - 検討: spotify_url を normalize（`?si=` 落とす、 異 ID は許容するなど）／ または別軸の identity（MusicBrainz Release Group ID 等）

### 低優先度

- [ ] **`track_name` フィールドが unused**
  - PR #2 で resolver が track 検索 fallback を廃止したため、 `Recommendation.track_name` / `NewRecommendation.track_name` は常に None。 将来 track 単位ソースが入るまで dead field
  - 場所: `src/domain/recommendation.rs`、 migration
  - 対処（YAGNI）: 今は触らず。 トラック単位の媒体追加時に再評価

- [ ] **`pipeline で Spotify resolver が Err を返す時の skip テスト**（既存 TODO の置き換え）
  - PR #2 で挙動を「Err → 保存せず Skip」に変更。 既存の Ok(None) skip テストは追加済み。 Err パスの test は未追加
  - 場所: `src/server/scrape.rs::tests`
  - 対処: wiremock で 500 を返すモックを仕込んで、 items_added=0 / items_skipped=1 を確認

- [ ] **`pitchfork_max_pages` のページネーション拡張**
  - 既定 3 ページで直近 90 日カバーできとるか未測定。 もし足りんかったら increment、 多すぎなら scrape 時間短縮可
  - 場所: `src/server/adapter/pitchfork.rs::list_candidates`
  - 対処: 3 ページ目末尾の publish_date を見て決める

### 気づき・注意点（実装ノート）

- **Pitchfork JSON の正規パス** は `__PRELOADED_STATE__.transformed.review.headerProps.{musicRating, artists, dangerousHed}`。 regex で navigate すると `headerProps.artists[0].genres[0].node.name`（ジャンル）を artist 名と誤マッチする事故あり。 必ず `serde_json::Value::pointer` で取る
- **Spotify resolver は album 一致のみ**。 `track_name` 更新パスも残っとるが、 現状は届かない（resolver が常に None track_name 返す）
- **dedup の key 優先順は spotify_url > 正規化 artist+album**。 PR #2 以降は全保存行が spotify_url 持つので、 実質 spotify_url のみで dedup される。 fallback path は残しとるが unreachable に近い
- **prod DB 直接掃除**: 仕様変更で過去データが invalid になった場合は `flyctl ssh console -C "sqlite3 /data/app.db 'DELETE FROM recommendations WHERE …'"` で消す。 既存 PR の場合 9 件削除した

## ZINE リデザインセッション（〜2026-05-10）の引き継ぎ

ZINE リデザイン（spec / plan は `docs/superpowers/2026-05-09-zine-*.md`）と Tailwind v4 導入は本番デプロイ済み。以下が残り。

### 中優先度

- [x] **PR ベース開発フローへ移行** — `.github/workflows/ci.yml` で `cargo test --features ssr` / `cargo leptos build` / `cargo sqlx prepare --check` を main 向け push と PR で回す（PR #3）。 `main` の保護ブランチ化は GitHub UI 側の作業として follow-up（下記参照）

- [ ] **空状態 UI**
  - DB に推しが0件の時、現状は空 `<ul>` が描画されるだけで何も出ん。「まだ推しが集まっとらんずら」みてぇな案内が欲しい
  - 場所: `src/pages/home.rs` の `RecommendationGrid`
  - 対処: `items.is_empty()` 分岐を追加

- [ ] **ジャケ画像の alt テキスト**
  - 現在 `<img alt="">`（装飾扱い）。スクリーンリーダ向けにアーティスト＋アルバム名を入れた方がアクセシブル
  - 場所: `src/pages/home.rs` の `<img>` 行
  - 対処: `alt=format!("{} - {}", item.artist_name, item.album_name.as_deref().unwrap_or(""))`

### 低優先度

- [ ] **ファビコン・OGP 画像の更新**
  - ZINE デザインに合うアイコン／OGP がまだ無い（あるいは旧デザイン依拠）。SNS シェア時の見栄えに影響
  - 場所: `assets/`、`<head>` への `<link rel="icon">` / `<meta property="og:*">` 追加

- [ ] **視覚回帰テスト**
  - Tailwind v4 やブレイクポイント変更で意図せぬ崩れが起きとらんか自動検出する手段が無い
  - 候補: Playwright screenshot diff を CI に組み込む

- [ ] **本番でジャケなしレコードの実機確認**
  - dev DB を直接編集して動作は確認済みじゃが、本番は全件 Spotify マッチ済みのため未踏。次に Spotify miss 出た時に画面崩れんか確認

### 気づき・注意点（実装ノート）

- **cargo-leptos 0.3.6 のデフォルト Tailwind は v4**（v3 ではない）。v4 は CSS-first 設定 (`@theme` ブロック) が標準で、JS config (`tailwind.config.js`) は実質無視される。バージョン上げる時は v3→v4 とは別の互換性確認が要る
- **v4 のブレイクポイントは `@media (width >= 600px)` range syntax**。Chrome 104+/Firefox 102+/Safari 16.4+ 必須。古い Safari ユーザを切る判断はしとらんが、現状は問題なし
- **`--breakpoint-sm: initial;` で v4 デフォルト（sm/md/lg/xl/2xl）を封じとる**。今後 utility 増やす時にハマる可能性。封じとる事実は `style/tailwind.css` 冒頭で明示
- **`@source "./src/**/*.rs"` で .rs を検出対象に明示追加**しとる。動的にクラス名組み立てる場面が出たら safelist が要る（今は静的のみ）
- **SQLite WAL モード**: `cp app.db` でバックアップ・復元する時、WAL 内の変更が反映され続けて期待通りに戻らんことがある。`sqlite3 .backup` か WAL チェックポイント後にコピーするのが正解
- **CSS バンドルサイズ**: 旧 `main.css` 1.2KB → 新 `tailwind.css` ビルド後 15.5KB。本番では gzip が効く想定で実害は無いが、定期的に出力サイズ眺める習慣をつけたほうがええ
- **Docker image サイズ**: Tailwind 入れても 39MB 据え置き（cargo-leptos が tailwindcss バイナリをビルド時にだけ使うため、ランタイム image には含まれん）

### スコープ外として保留

- **ダークモード対応** — クラフト紙質感が `prefers-color-scheme: dark` と相性悪いけぇ、対応するなら別デザインの ZINE「夜版」を切るレベルの作業。今回はやらん

## 中優先度（運用上、いずれ噛む）

- [x] **`main` を保護ブランチ化** — `gh api PUT /repos/.../branches/main/protection` で classic branch protection を設定。 `Require PR` (reviews=0) ＋ `Require status checks` (`ci`) ＋ `strict`（PR は最新 main と up-to-date 必須）。 `enforce_admins=false` なので緊急時は dlwr が UI 側で bypass 可

- [x] **CI 上の cargo-leptos / sqlx-cli インストールキャッシュ** — `actions/cache@v4` で `~/.cargo/bin/sqlx`（key: `bin-sqlx-<os>-v0.8`）と `~/.cargo/bin/cargo-leptos`（key: `bin-leptos-<os>-v0.3.6`）を個別キャッシュ。 hit 時は install step ごと skip。 7m6s → 54s（PR #5）

- [x] **スクレイプで 0件抽出時の警告** — `should_warn_zero_items(candidates_count, added, updated)` を pure fn で切り、 候補 5 超 ＆ 追加更新 0 件で `tracing::error!` を発火。 閾値境界をユニットテストで網羅（PR #9）

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

- [x] **SQLx offline metadata の CI チェック** — `.github/workflows/ci.yml` の `Check SQLx offline metadata` step で `cargo sqlx prepare --check` を走らせる（PR #3）

## テスト追加

- [ ] **`RecommendationRepo::upsert` の `manual_override` 保護パス**
  - 場所: `src/server/store.rs:37-58` の分岐が現在テストなし
  - 対処: `manual_override=1` の row を手動で SQL insert、upsert 呼んで spotify_url が保たれることを確認

- [ ] **`list_recent` の 3件以上での順序テスト**
  - 現状の単件・2件テストだけだと偶然パスする可能性あり（CLAUDE.md のルール）
  - 対処: 異なる featured_at の3件を入れて、降順で返ることを確認

## 機能拡張（要相談）

- [ ] **RSS から落ちた古い「推し」のバックフィル**
  - 現状: RSS は最新10件のみ。月次で 推し を5件以上出されると、cron 1日1回でも捌き切れず古いものが取りこぼされる可能性。
  - 案A: 過去 entrylist (`entrylist-2.html`, `-3.html`...) を初回バックフィル時のみ走査
  - 案B: 既知の `source_external_id` 全件を週次で再フェッチ
  - 案C: 月次でアーカイブページを走査
  - 検討: ブログの実際の更新ペース次第（今のとこ問題ない）

- [x] **他メディアソース追加（Pitchfork）** — `src/server/adapter/pitchfork.rs` で 8.0+ かつ直近 90 日を取得。同一アルバムは Rokinon と統合表示

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
