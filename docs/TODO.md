# 残タスク

このセッション（〜2026-05-09）でやり残したものの記録。最終 PR レビューで指摘された v1.5 4項目（per-article エラー許容 / fetch_and_extract 一貫化 / graceful shutdown / health endpoint）と Spotify URI / Bandcamp 正規化 / RSS 切替は実施済み。以下は未着手。

## Selector セッション（2026-05-15〜2026-05-17）の引き継ぎ

ホームに `Selector` ボタン (DJ メタファー = レゲエ・サウンドシステムの選盤者) を追加。 直近 30 日で dedup group 単位の `MIN(created_at)` が window 内のアルバムからランダム 1 枚を抜く `pick_recent_addition` を CTE + json_group_array で実装、 SelectorCard / SelectorCardView / `selector()` server fn / SelectorSlot+SelectorPick コンポーネントまで通して PR #30 で merge 済み。

CI 側で sqlx-cli cache が `sqlx` だけ保存して `cargo-sqlx` が抜けてた事故も並行で修正 (PR #30 内に同居)。 旧 cache 失効まで一回 cargo install が走る。

### 低優先度

- [ ] **ローカル macOS で `cargo leptos build` が失敗**
  - `mio 1.2.0` の wasm32 ターゲット不具合。 Cargo.lock 変更なしの状態で発生
  - SSR バイナリは普通に動く (`cargo build --features ssr` OK)。 CI (Linux) も通る
  - 場所: Cargo.lock の mio 行。 関連 issue: mio が wasm32 を将来サポートする方針かどうかチェック
  - 対処候補: mio を patch で固定するか、 別の I/O ライブラリへ替える話。 hobby 規模で実害は dev 体験のみなので低優先

- [ ] **Selector 「もう一度」 で同じアルバムが連続で出る可能性**
  - v1 では許容と spec で明記。 履歴 (LocalStorage か Cookie) 入れる時に対処
  - 体感で気になり始めたら検討

### 気づき・注意点（実装ノート）

- **`tests/visual/home.spec.ts` は pixel-diff スナップショットではない**。 grid-template-columns のトークン数と要素 count + aspect-ratio で layout を verify している。 UI 追加で header 部の DOM が増えても、 grid 兄弟要素が増えない限り既存テストはそのまま通る
- **CTE 共有不可**: `pick_recent_addition` と `list_recent_albums` の `keyed` CTE は同じ dedup_key 正規化を持つが、 `sqlx::query!` マクロのため Rust 側で共有化できない。 双方にコメントを置いてある (同期注意)
- **コンポーネント名と domain 型の衝突**: `SelectorCard` は domain 型 (`crate::domain::selector_card`) でも使うため、 Leptos コンポーネント側は `SelectorPick` に分離した。 似た構図が来たら最初から component 側を別名で切るのが正解
- **CI cache の罠**: 「`cargo cmd` で使う subcommand binary は `cargo-cmd` だけど、 cache path に `cmd` だけ書いてた」 という事故。 cargo の subcommand resolution を意識した cache 設計が必要

## Pitchfork ＋ カードマージセッション（2026-05-10）の引き継ぎ

Pitchfork ソース追加（スコア 8.0+ 直近 90 日）、 同一アルバムの Rokinon＋Pitchfork カードマージ、 Spotify 未配信アルバムの skip までデプロイ済み（PR #1, #2）。 以下は残り。

### 中優先度

- [x] **Pitchfork レビューページ実機 HTML を fixture に保存** — Aldous Harding "Train on the Island" のレビューを `tests/fixtures/pitchfork/realistic_full.html` に保存し、 4 つの extract 関数を一発で食わせる regression test を追加。 fixture 更新方法 (`curl -A '<UA>' …`) を test コメントに残した（PR #17）

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

- [x] **`pipeline で Spotify resolver が Err を返す時の skip テスト** — wiremock で `/search` に 500 を返すモックを仕込んで、 candidate 1 件に対し `items_added=0` / `items_skipped=1` となることを確認。 mutation で regression を捕捉できることも確認（PR #14）

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

- [x] **空状態 UI** — `AlbumGrid` 冒頭で `items.is_empty()` 分岐を入れて「まだ推しが集まっとらんずら」案内を出す（PR #16）。 本番 DB は常時データありなので browser 視覚確認は未実施

- [x] **ジャケ画像の alt テキスト** — 純粋関数 `image_alt(artist, album)` で "Artist - Album" / album 無しの時は "Artist" のみ。 末尾ダッシュ防止と空白 trim を含めユニットテスト 3 件で網羅（PR #15）

### 低優先度

- [ ] **ファビコン・OGP 画像の更新**
  - ZINE デザインに合うアイコン／OGP がまだ無い（あるいは旧デザイン依拠）。SNS シェア時の見栄えに影響
  - 場所: `assets/`、`<head>` への `<link rel="icon">` / `<meta property="og:*">` 追加

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

- [x] **`/readyz` を `/healthz` と分離** — `db_ready(&pool)` で `SELECT 1` を打つ `/readyz` を新設。 alive / closed の両ケースを in-memory pool でテスト。 fly check は引き続き `/healthz`（PR #11）

- [x] **main マージ時の auto-deploy (CD) 設定** — `.github/workflows/deploy.yml` で `workflow_run` トリガ（CI が main で success の時のみ発火） ＋ `flyctl deploy --remote-only` ＋ `FLY_API_TOKEN` secret。 `concurrency=deploy-main` ＋ `cancel-in-progress=false` で連続 merge も順番に処理。 初回 deploy（v7、 PR #24 マージ自身がトリガ）で疎通確認済（PR #24）
  - 残課題: docs-only の merge でも deploy が走る（コード変更が無ければ実害は再起動コストのみ）。 `paths-ignore` を後付けする余地あり
  - 残課題: 視覚回帰テスト未整備のまま CD 化したため、 UI 変更 PR の崩れが本番直撃しうる（下記 視覚回帰テスト 項目で対応予定）
  - 残課題: migrations 含む PR の manual approval は未設定。 SQLite ALTER TABLE 系の事故が起きたら GitHub Environments で挟む

- [x] **視覚回帰テスト** — Playwright で home の grid layout を 3 viewport (375/768/1280px) ＋ aspect-ratio で検証。 `tests/visual/` 配下に独立した npm パッケージ、 CI でも自動実行（`.github/workflows/ci.yml`）。 pixel-perfect な screenshot diff は採らず、 computed CSS (`grid-template-columns`、 `aspect-ratio`) と DOM 構造の assertion でカバー（PR #26）
  - スコープ外: empty state UI、 font / 色味の差分。 pre-seeded 構成（DB を config 評価時に migrate + seed）に統一し、 test 中に DB を mutate せん。 Playwright の `globalSetup` が `webServer` と並列で起動する仕様に依存する race を避けるため

- [ ] **空状態の視覚カバレッジ**
  - 現状 視覚回帰は seeded grid のみで、 「まだ推しが集まっとらんずら」案内のレイアウト崩れを検出できん
  - 候補: 別 Playwright project を切って `webServer` を別 port ・ 別 DB で立ち上げ、 そっち向けの empty.spec.ts を回す。 もしくは Rust 統合テストで `axum::TestServer` 経由でレスポンス HTML を assert
  - 優先度: 低。 ZINE redesign 後の本番は常時データありなので実害確率小

## 低優先度（コード品質・整備）

- [x] **800ms スリープを Config field 化** — `Config::scrape_throttle_ms` (env: `SCRAPE_THROTTLE_MS`、 既定 800) に切り出し、 `throttle_ms=0` で sleep を skip。 副次でテスト時間 2.4s → 0.06s に短縮（PR #20）

- [x] **`Regex::new(...)` の毎回コンパイル削減** — `rokinon.rs` の `OSHI_PATTERN` / `ENTRY_ID_PATTERN` を `LazyLock<Regex>` でモジュールトップに上げ、 `pitchfork.rs` と同じ形に揃えた（PR #19）

- [ ] **`ScrapePipeline::new()` コンストラクタ追加**
  - 場所: `src/server/scrape.rs`
  - 現状: `pub` field を直接指定する struct literal で構築。リファクタ耐性が弱い。
  - 対処: `pub fn new(source, resolver, repo, log) -> Self` を切る

- [x] **cron への `CancellationToken` 伝播** — `ScrapePipeline.cancel: CancellationToken` を持たせ、 候補ループ前で `is_cancelled()` 観測。 `main.rs` の shutdown future で ctrl_c 受信時に `cancel()` 呼ぶ。 事前 cancel した token + 候補 3 件で 1 件も処理されんことを確認（PR #13）

- [x] **SQLx offline metadata の CI チェック** — `.github/workflows/ci.yml` の `Check SQLx offline metadata` step で `cargo sqlx prepare --check` を走らせる（PR #3）

## テスト追加

- [x] **`RecommendationRepo::upsert` の `manual_override` 保護パス** — `manual_override=1` の row に upsert したとき spotify_url / spotify_image_url が保たれ、 artist/album/youtube/featured_at は更新されることを確認。 `if false && prev.manual_override` mutation で regression を捕捉できることも確認（PR #12）

- [x] **`list_recent` の 3件以上での順序テスト** — 当初 stale な TODO だった (`list_recent_albums_orders_by_latest_featured_at_desc` は既に 3 件)。 死蔵の `list_recent` 自体を削除して問題ごと閉じた（PR #18）

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
