# 設計書: 音楽メディア推薦集約サイト

- 作成日: 2026-05-08
- ステータス: ドラフト

## 1. 目的とスコープ

複数の音楽メディア（ブログ・批評サイト等）が「推し」として取り上げた音楽を集約・一覧表示するWebサイトを構築する。各楽曲・アルバムは Spotify のリンクとジャケット画像とともに表示する。

### 第1イテレーションのスコープ

- 1ソースのみ実装：「ロキノンには騙されないぞ」(<https://ameblo.jp/stamedba/>)
- 「推し」マーカー（記事末尾の `YYYYMM推し` 形式）が付いた記事のみ抽出
- 抽出したアーティスト名・アルバム名で Spotify Search API を叩き、Track/Album URL とジャケット画像を取得
- Spotify でマッチしなかった場合は YouTube リンク（記事中のURL）にフォールバック
- 一覧ページに「ジャケット画像 + アーティスト + アルバム + Spotify/YouTubeリンク + ソースメディア + 取り上げ月」を表示

### スコープ外（将来拡張）

- 他メディア追加（アダプタを追加するだけで対応可能な設計にする）
- ユーザー認証・投稿機能
- 手動レビューキュー（スクレイプ結果の確認UI）
- メディア・タグ・期間でのフィルタリング
- RSS/JSONフィード提供
- モバイルアプリ

## 2. アーキテクチャ概要

```
┌─────────────────────────────────────────────────────────┐
│ fly.io (Rust binary, 1 process)                         │
│                                                          │
│  ┌───────────────┐    ┌──────────────────────────────┐ │
│  │ Leptos SSR    │    │ Scraper Job (tokio cron)     │ │
│  │ (Axum)        │    │  - MediaSource trait         │ │
│  │  - 一覧ページ  │    │  - RokinonAdapter            │ │
│  │  - 詳細ページ  │    │  - SpotifyResolver           │ │
│  └───────┬───────┘    └────────┬─────────────────────┘ │
│          │                     │                        │
│          └─────────┬───────────┘                        │
│                    ▼                                    │
│             ┌──────────────┐                            │
│             │ SQLite       │                            │
│             │ (fly volume) │                            │
│             └──────┬───────┘                            │
│                    │                                    │
│             ┌──────▼─────────┐                          │
│             │ litestream→S3  │ (バックアップ)            │
│             └────────────────┘                          │
└─────────────────────────────────────────────────────────┘
       │                                  ▲
       ▼                                  │
   ┌────────┐                       ┌────────────────────┐
   │ ユーザー │                       │ Spotify Web API    │
   └────────┘                       │ ameblo.jp          │
                                    └────────────────────┘
```

- 単一プロセスで Web サーバとスクレイパが同居（個人規模、シンプル優先）
- スクレイパは tokio スケジューラで定期起動（例：1日1回）
- DB は SQLite、fly volume にマウント、litestream で S3 にレプリケーション

## 3. コンポーネント

### 3.1 `core/`（ドメインモデル）

- `Recommendation` 構造体：1件の推薦アイテム
  - `id: i64`（DB主キー）
  - `source_id: String`（メディア識別子、例 `"rokinon"`）
  - `source_url: String`（記事URL）
  - `source_external_id: String`（記事ID等、重複防止キー）
  - `featured_at: NaiveDate`（「推し」として取り上げられた月。`YYYYMM推し` から復元）
  - `artist_name: String`
  - `album_name: Option<String>`
  - `track_name: Option<String>`
  - `spotify_url: Option<String>`
  - `spotify_image_url: Option<String>`
  - `youtube_url: Option<String>`
  - `manual_override: bool`（true なら自動再上書きしない）
  - `created_at`, `updated_at`

### 3.2 `adapter/`（ソースアダプタ）

```rust
#[async_trait]
trait MediaSource {
    fn id(&self) -> &'static str;
    async fn fetch_candidates(&self) -> Result<Vec<RawArticle>>;
    fn extract_recommendation(&self, article: &RawArticle) -> Option<ExtractedItem>;
}
```

- `RawArticle`：記事URL・HTML・タイトル等の中間表現
- `ExtractedItem`：「推し」判定済みの抽出結果（アーティスト名、アルバム名、YouTube URL等）
- `RokinonAdapter`：
  - 記事一覧ページ（`/stamedba/entrylist-N.html`）から記事URLを列挙
  - 各記事HTMLを取得し、埋め込み JSON (`window.INITIAL_STATE`) から `entry_text` を取り出して解析
  - `\d{6}推し` パターンを検出した記事のみ抽出
  - タイトルから `{Artist Name} の新作` 形式でアーティスト名を抽出
  - `<h2>` または本文先頭からアルバム名を抽出
  - YouTube リンクを `<a href>` から拾う

### 3.3 `resolver/`（外部API解決）

- `SpotifyResolver`：
  - Spotify Web API（Client Credentials フロー）
  - `search?q=artist:"X" album:"Y"&type=album` でアルバム検索
  - 取れたら `external_urls.spotify` と `images[0].url` を保存
  - 取れなければトラック検索にフォールバック
  - それでも取れなければ `spotify_url = None`

### 3.4 `scheduler/`（cron）

- tokio + `tokio_cron_scheduler` で `0 0 4 * * *`（毎日4時、JST）に全アダプタを実行
- 起動時実行は `scrape_runs` が空のときのみ（初回投入のため。再起動ループでスクレイプ連打されるのを防ぐ）
- 手動キック用の CLI バイナリも提供（`cargo run --bin scrape -- --source rokinon`）

### 3.5 `web/`（Leptos SSR）

- `/`：一覧ページ（取り上げ月で降順、ジャケット画像グリッド）
- `/r/:id`：詳細ページ（記事本文URL、Spotify埋め込み、YouTube埋め込み）
- Leptos Server Functions は不要、SSR + 静的リンクで完結

## 4. データフロー

### 4.1 スクレイプ→保存

```
cron tick
  → RokinonAdapter.fetch_candidates()
      → /entrylist-1.html, /entrylist-2.html ...
      → 各記事 fetch
  → for each article:
      → extract_recommendation()
          → 「推し」マーカーが無ければスキップ
      → DBに source_external_id でUPSERT
          → 既存で manual_override=true ならSpotify再解決スキップ
          → そうでなければ SpotifyResolver.resolve() を呼んで上書き
```

### 4.2 ページ表示

```
GET /
  → SQL: SELECT * FROM recommendations ORDER BY featured_at DESC, id DESC LIMIT 100
  → Leptos view! でグリッドレンダー
      → spotify_image_url があれば <img>、無ければ灰色プレースホルダ
      → spotify_url があれば「Spotifyで開く」、無ければ youtube_url、両方無ければソース記事リンクのみ
```

## 5. データベース

### `recommendations` テーブル

```sql
CREATE TABLE recommendations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id TEXT NOT NULL,
    source_url TEXT NOT NULL,
    source_external_id TEXT NOT NULL,
    featured_at TEXT NOT NULL,  -- YYYY-MM-01 形式
    artist_name TEXT NOT NULL,
    album_name TEXT,
    track_name TEXT,
    spotify_url TEXT,
    spotify_image_url TEXT,
    youtube_url TEXT,
    manual_override INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE (source_id, source_external_id)
);
CREATE INDEX idx_featured_at ON recommendations (featured_at DESC);
```

### `scrape_runs` テーブル（運用ログ）

```sql
CREATE TABLE scrape_runs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id TEXT NOT NULL,
    started_at TEXT NOT NULL,
    finished_at TEXT,
    status TEXT NOT NULL,  -- 'running' | 'success' | 'error'
    items_added INTEGER DEFAULT 0,
    items_updated INTEGER DEFAULT 0,
    error_message TEXT
);
```

## 6. エラーハンドリング

- スクレイピング失敗：1記事失敗しても他は続行。`scrape_runs.error_message` に残す
- Spotify API 失敗（レート制限・障害）：その記事の Spotify フィールドだけ空のまま保存し、次回 cron で再試行
- DB 接続失敗：プロセス起動失敗とみなす（fly.io が再起動）
- パニックは `tracing` でログ、プロセスは継続

## 7. テスト戦略

- `core/` モデル：純粋データ構造、ユニットテストなし（型で十分）
- `adapter/` ：実HTMLフィクスチャ（`tests/fixtures/rokinon/*.html`）を読んで `extract_recommendation()` の出力を検証
- `resolver/` ：Spotify API は wiremock でモック
- `scheduler/` ：時刻トリガはテスト対象外（手動キックCLIで動作確認）
- `web/` ：Leptos の SSR 出力を `assert_html` で確認（ジャケット画像 `<img>` が出る、リンクが正しい等）

t_wada 流TDDに沿って、`adapter/` は実HTMLからの抽出ロジックを最初にテスト→実装。

## 8. デプロイと運用

- `Dockerfile`：multi-stage build、`cargo-leptos` で SSR バイナリ化
- `fly.toml`：1 machine、shared-cpu-1x、512MB、`/data` に volume mount
- 環境変数：
  - `SPOTIFY_CLIENT_ID`, `SPOTIFY_CLIENT_SECRET`（fly secrets）
  - `DATABASE_URL=sqlite:/data/app.db`
  - `LITESTREAM_BUCKET`（後付け）
- マイグレーション：sqlx-cli を起動時に実行
- ログ：`tracing` → fly logs

## 9. セキュリティ

- 公開ページのみ、認証無し
- Spotify Client Credentials は fly secrets に格納（リポジトリにコミットしない）
- スクレイピング先には `User-Agent: i-am-rockin-on bot/1.0 (+contact)` を付け、`robots.txt` を尊重、レート制限（1記事1秒スリープ）

## 10. オープンクエスチョン（実装中に詰める）

- Ameblo の `entrylist-N.html` 全ページ走査するか、最近2-3ページに留めるか（初回フル、以降は差分）→ 初回フル、以降は最新ページのみで十分とする
- 「推し」マーカーが過去記事に遡及して付くケース → 既存記事にも更新があれば再評価する
- アルバム名抽出のフェイルセーフ → アルバム名が無いときはトラック検索もフォールバック

## 11. 将来拡張への備え

- `MediaSource` trait を切ることで、新メディアはアダプタ追加だけで対応
- `recommendations.source_id` でメディア識別、UI でフィルタ追加可能
- 手動レビューキューは `recommendations.status` カラム追加で対応可能（pending/published）
- SQLite から Postgres 移行は sqlx の方言切替で対応可能
