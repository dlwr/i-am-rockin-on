# funkstudy ソース Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** taizooo の `#yetanotherfunkstudy` 付きポストを twitterapi.io で拾い、ぶら下がる返信中の Spotify アルバム URL から推薦を取り込む `funkstudy` ソースを追加する。

**Architecture:** 既存の `MediaSource` trait に `FunkstudyAdapter`（Twitter 取得専念）を足す。アダプタは Spotify アルバム URL を `spotify_url` に乗せて返し、ピプラインは `spotify_url` が album URL のとき名前検索ではなく Spotify アルバム ID 解決でメタデータを確定する。返信に Spotify URL がまだ無い本体は transient エラーで次回リトライ（検索窓 30 日で自然消滅）。

**Tech Stack:** Rust / async-trait / reqwest / serde / regex / chrono / wiremock（テスト）。外部 API: twitterapi.io（`X-API-Key`）, Spotify Web API。

参照スペック: `docs/superpowers/specs/2026-06-01-funkstudy-source-design.md`

---

## File Structure

- `src/server/error.rs` — Modify: `AppError::Retryable(String)` を追加（transient skip の合図）
- `src/server/resolver/spotify.rs` — Modify: `spotify_album_id_from_url()`・`SpotifyAlbumMeta`・`resolve_by_album_id()`・`with_albums_url()` を追加
- `src/server/scrape.rs` — Modify: `process_candidate` に「spotify_url が album URL なら ID 解決」分岐を追加
- `src/server/adapter/funkstudy.rs` — Create: `FunkstudyAdapter`（MediaSource 実装）
- `src/server/adapter/mod.rs` — Modify: `pub mod funkstudy;`
- `src/server/config.rs` — Modify: `funkstudy_*` フィールド追加
- `src/main.rs` — Modify: funkstudy pipeline 生成・初回スクレイプ・cron 登録
- `src/bin/scrape.rs` — Modify: `--source funkstudy` の match arm
- `tests/fixtures/funkstudy/` — Create: wiremock 用 JSON fixture
- `README.md` — Modify: ソース一覧・env テーブル・scrape 例

---

## Task 1: AppError に Retryable variant を追加

**Files:**
- Modify: `src/server/error.rs`

`fetch_and_extract` が「本体は見つかったが Spotify 返信がまだ無い」を返すための transient エラー。ピプラインは `fetch_and_extract` の `Err` を warn+skip 扱いにし `mark_scraped` しない（`src/server/scrape.rs:104` の `?` 伝播 → `src/server/scrape.rs:67-74`）ので、これで次回リトライになる。

- [ ] **Step 1: Write the failing test**

`src/server/error.rs` の `mod tests` に追加:

```rust
    #[test]
    fn retryable_error_displays_message() {
        let err = AppError::Retryable("spotify reply not found yet".into());
        assert_eq!(err.to_string(), "retryable: spotify reply not found yet");
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --features ssr --lib server::error::tests::retryable_error_displays_message`
Expected: コンパイルエラー（`Retryable` variant が無い）

- [ ] **Step 3: Write minimal implementation**

`src/server/error.rs` の enum に variant を追加:

```rust
    #[error("retryable: {0}")]
    Retryable(String),
```

（`Config(String)` の下に追加する）

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --features ssr --lib server::error::tests::retryable_error_displays_message`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/server/error.rs
git commit -m "feat(error): transient リトライ用の AppError::Retryable を追加"
```

---

## Task 2: spotify_album_id_from_url ヘルパー

**Files:**
- Modify: `src/server/resolver/spotify.rs`

`open.spotify.com/album/<id>` から base62 id を取り出す純粋関数。ピプラインが `spotify_url` から id を抽出するのに使う。`?si=...` クエリと末尾スラッシュを除去する。

- [ ] **Step 1: Write the failing test**

`src/server/resolver/spotify.rs` の `mod tests` 末尾に追加:

```rust
    #[test]
    fn album_id_extracted_from_plain_url() {
        assert_eq!(
            spotify_album_id_from_url("https://open.spotify.com/album/4aawyAB9vmqN3uQ7FjRGTy"),
            Some("4aawyAB9vmqN3uQ7FjRGTy".to_string())
        );
    }

    #[test]
    fn album_id_extracted_ignoring_query_and_trailing() {
        assert_eq!(
            spotify_album_id_from_url("http://open.spotify.com/album/4aawyAB9vmqN3uQ7FjRGTy?si=abc123"),
            Some("4aawyAB9vmqN3uQ7FjRGTy".to_string())
        );
    }

    #[test]
    fn non_album_url_returns_none() {
        assert_eq!(
            spotify_album_id_from_url("https://open.spotify.com/track/123"),
            None
        );
        assert_eq!(spotify_album_id_from_url("not a url"), None);
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --features ssr --lib server::resolver::spotify::tests::album_id`
Expected: コンパイルエラー（`spotify_album_id_from_url` が無い）

- [ ] **Step 3: Write minimal implementation**

`src/server/resolver/spotify.rs` の先頭 `use` に追加:

```rust
use regex::Regex;
use std::sync::LazyLock;
```

ファイル中（`sanitize_query_value` の近く、関数スコープ外）に追加:

```rust
static ALBUM_ID_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"open\.spotify\.com/album/([A-Za-z0-9]+)").unwrap());

/// `open.spotify.com/album/<id>` から base62 の album id を取り出す。
/// `?si=...` クエリや末尾スラッシュは char class の範囲外なので自然に切れる。
pub fn spotify_album_id_from_url(url: &str) -> Option<String> {
    ALBUM_ID_RE
        .captures(url)
        .map(|c| c[1].to_string())
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --features ssr --lib server::resolver::spotify::tests::album_id`
Expected: 3 件 PASS

- [ ] **Step 5: Commit**

```bash
git add src/server/resolver/spotify.rs
git commit -m "feat(spotify): album URL から id を抽出する spotify_album_id_from_url を追加"
```

---

## Task 3: SpotifyResolver::resolve_by_album_id

**Files:**
- Modify: `src/server/resolver/spotify.rs`

アルバム id から `GET /v1/albums/{id}` でメタデータ（artist/album/jacket/正規 url）を取得する。404 は `Ok(None)`。テスト用に albums エンドポイントを差し替える `with_albums_url` を足す。

- [ ] **Step 1: Write the failing test**

`src/server/resolver/spotify.rs` の `mod tests` に追加:

```rust
    #[tokio::test]
    async fn resolve_by_album_id_returns_meta() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "tok", "token_type": "Bearer", "expires_in": 3600
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/albums/abc123"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "name": "Some Album",
                "artists": [{ "name": "Some Artist" }],
                "images": [{ "url": "https://i.scdn.co/image/x.jpg" }],
                "external_urls": { "spotify": "https://open.spotify.com/album/abc123" }
            })))
            .mount(&server)
            .await;

        let r = SpotifyResolver::new("id".into(), "sec".into())
            .with_endpoints(format!("{}/token", server.uri()), format!("{}/search", server.uri()))
            .with_albums_url(format!("{}/albums", server.uri()));
        let m = r.resolve_by_album_id("abc123").await.unwrap().unwrap();
        assert_eq!(m.artist_name, "Some Artist");
        assert_eq!(m.album_name, "Some Album");
        assert_eq!(m.url, "https://open.spotify.com/album/abc123");
        assert_eq!(m.image_url.unwrap(), "https://i.scdn.co/image/x.jpg");
    }

    #[tokio::test]
    async fn resolve_by_album_id_returns_none_on_404() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "tok", "token_type": "Bearer", "expires_in": 3600
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/albums/missing"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let r = SpotifyResolver::new("id".into(), "sec".into())
            .with_endpoints(format!("{}/token", server.uri()), format!("{}/search", server.uri()))
            .with_albums_url(format!("{}/albums", server.uri()));
        assert!(r.resolve_by_album_id("missing").await.unwrap().is_none());
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --features ssr --lib server::resolver::spotify::tests::resolve_by_album_id`
Expected: コンパイルエラー（`with_albums_url` / `resolve_by_album_id` / `SpotifyAlbumMeta` が無い）

- [ ] **Step 3: Write minimal implementation**

`src/server/resolver/spotify.rs` の定数に追加（`SEARCH_URL` の下）:

```rust
const ALBUMS_URL: &str = "https://api.spotify.com/v1/albums";
```

`SpotifyResolver` struct に `albums_url: String` フィールドを追加。`new()` で `albums_url: ALBUMS_URL.into(),` を初期化。`with_endpoints` の下に builder を追加:

```rust
    pub fn with_albums_url(mut self, albums_url: String) -> Self {
        self.albums_url = albums_url;
        self
    }
```

`resolve` メソッドの下に追加:

```rust
    /// album id から `GET /v1/albums/{id}` でメタデータを取得する。
    /// 404 は「配信停止/不正 id」とみなして None。
    pub async fn resolve_by_album_id(&self, album_id: &str) -> AppResult<Option<SpotifyAlbumMeta>> {
        let token = self.access_token().await?;
        let resp = self
            .http
            .get(format!("{}/{}", self.albums_url, album_id))
            .bearer_auth(&token)
            .send()
            .await?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let album: AlbumResp = resp.error_for_status()?.json().await?;
        let artist_name = album
            .artists
            .into_iter()
            .next()
            .map(|a| a.name)
            .unwrap_or_default();
        Ok(Some(SpotifyAlbumMeta {
            url: album.external_urls.spotify,
            image_url: album.images.into_iter().next().map(|i| i.url),
            artist_name,
            album_name: album.name,
        }))
    }
```

`SpotifyMatch` struct の下に型を追加:

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct SpotifyAlbumMeta {
    pub url: String,
    pub image_url: Option<String>,
    pub artist_name: String,
    pub album_name: String,
}

#[derive(Debug, Deserialize)]
struct AlbumResp {
    name: String,
    artists: Vec<ArtistRef>,
    images: Vec<Image>,
    external_urls: ExternalUrls,
}
#[derive(Debug, Deserialize)]
struct ArtistRef {
    name: String,
}
```

（`Image` と `ExternalUrls` は既存定義を再利用する）

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --features ssr --lib server::resolver::spotify`
Expected: 既存テスト + 新規 2 件すべて PASS

- [ ] **Step 5: Commit**

```bash
git add src/server/resolver/spotify.rs
git commit -m "feat(spotify): album id からメタデータを引く resolve_by_album_id を追加"
```

---

## Task 4: ピプラインに album ID 解決の分岐を追加

**Files:**
- Modify: `src/server/scrape.rs:114-145`
- Test: `src/server/scrape.rs`（`mod tests`）

`spotify_url` が事前セットされた album URL のときは名前検索ではなく `resolve_by_album_id` でメタデータを確定する。`spotify_url=None` を返す rokinon/pitchfork は従来どおり名前検索（挙動不変）。

- [ ] **Step 1: Write the failing test**

`src/server/scrape.rs` の `mod tests` に追加（既存 `FakeSource` を利用）。`use` は `mod tests` 冒頭に揃っている前提。MockServer 用 import は `wiremock::{matchers::{method, path}, Mock, MockServer, ResponseTemplate}` を関数内で使う:

```rust
    #[tokio::test]
    async fn pipeline_resolves_by_album_id_when_spotify_url_preset() {
        use wiremock::{matchers::{method, path}, Mock, MockServer, ResponseTemplate};

        let pool = SqlitePoolOptions::new().connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "tok", "token_type": "Bearer", "expires_in": 3600
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/albums/abc123"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "name": "Resolved Album",
                "artists": [{ "name": "Resolved Artist" }],
                "images": [{ "url": "https://i.scdn.co/image/y.jpg" }],
                "external_urls": { "spotify": "https://open.spotify.com/album/abc123" }
            })))
            .mount(&server)
            .await;

        let resolver = Arc::new(
            SpotifyResolver::new("id".into(), "sec".into())
                .with_endpoints(format!("{}/token", server.uri()), format!("{}/search", server.uri()))
                .with_albums_url(format!("{}/albums", server.uri())),
        );
        let source = Arc::new(FakeSource {
            items: vec![NewRecommendation {
                source_id: "funkstudy".into(),
                source_url: "https://x.com/taizooo/status/1".into(),
                source_external_id: "1".into(),
                featured_at: NaiveDate::from_ymd_opt(2026, 5, 30).unwrap(),
                artist_name: String::new(),
                album_name: None,
                track_name: None,
                spotify_url: Some("https://open.spotify.com/album/abc123".into()),
                spotify_image_url: None,
                youtube_url: None,
            }],
        });
        let repo = Arc::new(RecommendationRepo::new(pool.clone()));
        let pipeline = ScrapePipeline {
            source,
            resolver,
            repo: repo.clone(),
            log: Arc::new(ScrapeLog::new(pool.clone())),
            cancel: CancellationToken::new(),
            throttle_ms: 0,
        };
        let _ = repo; // upsert 済みの行を直接 DB から検証する
        let outcome = pipeline.run().await.unwrap();
        assert_eq!(outcome.items_added, 1);

        let row: (String, Option<String>, Option<String>, Option<String>) = sqlx::query_as(
            "SELECT artist_name, album_name, spotify_url, spotify_image_url \
             FROM recommendations WHERE source_external_id = '1'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.0, "Resolved Artist");
        assert_eq!(row.1.as_deref(), Some("Resolved Album"));
        assert_eq!(row.2.as_deref(), Some("https://open.spotify.com/album/abc123"));
        assert_eq!(row.3.as_deref(), Some("https://i.scdn.co/image/y.jpg"));
    }
```

注: `SpotifyResolver` / `ScrapeLog` / `RecommendationRepo` / `Arc` / `CancellationToken` は scrape.rs 冒頭の `use` が `use super::*` 経由で `mod tests` に入っている。`CancellationToken` が見つからない場合のみ関数内で `use tokio_util::sync::CancellationToken;` を足す。

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --features ssr --lib server::scrape::tests::pipeline_resolves_by_album_id_when_spotify_url_preset`
Expected: FAIL（現状は名前検索パスに入り album_name=None で resolve→None→skip、items_added=0）

- [ ] **Step 3: Write minimal implementation**

`src/server/scrape.rs` の `let mut new_rec = extracted;`（114 行目付近）の直後から、`resolve` を呼んでいる `match` ブロック全体（〜145 行目の閉じ）を次で置き換える:

```rust
        let mut new_rec = extracted;
        // funkstudy のようにアダプタが Spotify アルバム URL を確定済みなら、
        // 名前検索ではなく album id で直接メタデータを解決する。
        if let Some(album_id) = new_rec
            .spotify_url
            .as_deref()
            .and_then(crate::server::resolver::spotify::spotify_album_id_from_url)
        {
            match self.resolver.resolve_by_album_id(&album_id).await {
                Ok(Some(meta)) => {
                    new_rec.artist_name = meta.artist_name;
                    new_rec.album_name = Some(meta.album_name);
                    new_rec.spotify_url = Some(meta.url);
                    new_rec.spotify_image_url = meta.image_url;
                }
                Ok(None) => {
                    tracing::info!(album_id = %album_id, "spotify album id not found; skipping");
                    return Ok(ProcessResult::Skipped);
                }
                Err(e) => {
                    tracing::warn!(error = %e, album_id = %album_id, "spotify album resolve failed; will retry next scrape");
                    return Ok(ProcessResult::Skipped);
                }
            }
        } else {
            match self
                .resolver
                .resolve(&new_rec.artist_name, new_rec.album_name.as_deref())
                .await
            {
                Ok(Some(m)) => {
                    new_rec.spotify_url = Some(m.url);
                    new_rec.spotify_image_url = m.image_url;
                    if new_rec.track_name.is_none() {
                        new_rec.track_name = m.track_name;
                    }
                }
                Ok(None) => {
                    tracing::info!(
                        artist = %new_rec.artist_name,
                        album = ?new_rec.album_name,
                        "spotify album match not found; skipping recommendation"
                    );
                    return Ok(ProcessResult::Skipped);
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        artist = %new_rec.artist_name,
                        "spotify resolve failed; skipping recommendation (will retry next scrape)"
                    );
                    return Ok(ProcessResult::Skipped);
                }
            }
        }
```

（その後の `let (_, inserted) = self.repo.upsert(new_rec).await?;` 以降は変更しない）

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --features ssr --lib server::scrape`
Expected: 既存テスト + 新規 1 件すべて PASS

- [ ] **Step 5: Commit**

```bash
git add src/server/scrape.rs
git commit -m "feat(scrape): spotify_url 事前セット時は album id で直接解決する分岐を追加"
```

---

## Task 5: FunkstudyAdapter

**Files:**
- Create: `src/server/adapter/funkstudy.rs`
- Modify: `src/server/adapter/mod.rs`
- Create: `tests/fixtures/funkstudy/search.json`, `tests/fixtures/funkstudy/replies_with_spotify.json`, `tests/fixtures/funkstudy/replies_without_spotify.json`

twitterapi.io の advanced_search で本体ポストを列挙し、replies で taizooo 自身の返信から Spotify アルバム URL を抽出する。返信に無ければ `AppError::Retryable`。

- [ ] **Step 1: mod.rs に登録（コンパイル単位を作る）**

`src/server/adapter/mod.rs` を次にする:

```rust
pub mod funkstudy;
pub mod pitchfork;
pub mod rokinon;
pub mod source;
```

- [ ] **Step 2: fixture を作成**

`tests/fixtures/funkstudy/search.json`:

```json
{
  "tweets": [
    {
      "id": "1001",
      "url": "https://x.com/taizooo/status/1001",
      "text": "#yetanotherfunkstudy",
      "createdAt": "Sat May 30 12:00:00 +0000 2026",
      "author": { "userName": "taizooo" }
    }
  ],
  "has_next_page": false,
  "next_cursor": ""
}
```

`tests/fixtures/funkstudy/replies_with_spotify.json`:

```json
{
  "replies": [
    {
      "id": "1002",
      "url": "https://x.com/taizooo/status/1002",
      "text": "https://t.co/short",
      "createdAt": "Sat May 30 12:01:00 +0000 2026",
      "author": { "userName": "taizooo" },
      "entities": {
        "urls": [
          { "expanded_url": "https://open.spotify.com/album/4aawyAB9vmqN3uQ7FjRGTy?si=abc" }
        ]
      }
    },
    {
      "id": "1003",
      "text": "https://open.spotify.com/album/SHOULDNOTWIN",
      "createdAt": "Sat May 30 12:02:00 +0000 2026",
      "author": { "userName": "someone_else" }
    }
  ],
  "has_next_page": false,
  "next_cursor": ""
}
```

`tests/fixtures/funkstudy/replies_without_spotify.json`:

```json
{
  "replies": [
    {
      "id": "1004",
      "text": "いいね",
      "createdAt": "Sat May 30 12:01:00 +0000 2026",
      "author": { "userName": "taizooo" }
    }
  ],
  "has_next_page": false,
  "next_cursor": ""
}
```

- [ ] **Step 3: Write the failing tests**

`src/server/adapter/funkstudy.rs` を作成し、まずテストを書く（実装は空 struct のみ）:

```rust
use crate::domain::recommendation::NewRecommendation;
use crate::server::adapter::source::{CandidateRef, MediaSource};
use crate::server::error::{AppError, AppResult};
use async_trait::async_trait;
use chrono::{NaiveDate, Utc};
use regex::Regex;
use reqwest::Client;
use serde::Deserialize;
use std::sync::LazyLock;

const DEFAULT_BASE_URL: &str = "https://api.twitterapi.io";

static SPOTIFY_ALBUM_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"https?://open\.spotify\.com/album/[A-Za-z0-9]+").unwrap());

pub struct FunkstudyAdapter {
    client: Client,
    base_url: String,
    api_key: String,
    screen_name: String,
    backfill_days: i64,
    max_search_pages: u32,
}

impl FunkstudyAdapter {
    pub fn new(api_key: String, screen_name: String, backfill_days: i64) -> Self {
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap(),
            base_url: DEFAULT_BASE_URL.into(),
            api_key,
            screen_name,
            backfill_days,
            max_search_pages: 5,
        }
    }

    pub fn with_base_url(mut self, base_url: String) -> Self {
        self.base_url = base_url;
        self
    }
}

/// twitter の `createdAt`（例: "Sat May 30 12:00:00 +0000 2026"）を JST 日付に変換する。
fn created_at_to_jst_date(s: &str) -> Option<NaiveDate> {
    chrono::DateTime::parse_from_str(s, "%a %b %d %H:%M:%S %z %Y")
        .ok()
        .map(|dt| (dt.with_timezone(&Utc) + chrono::Duration::hours(9)).date_naive())
}

/// テキスト/URL 群から最初の Spotify album URL を取り出す。
fn first_spotify_album_url(candidates: &[String]) -> Option<String> {
    candidates
        .iter()
        .find_map(|s| SPOTIFY_ALBUM_RE.find(s).map(|m| m.as_str().to_string()))
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    #[serde(default)]
    tweets: Vec<Tweet>,
    #[serde(default)]
    has_next_page: bool,
    #[serde(default)]
    next_cursor: String,
}

#[derive(Debug, Deserialize)]
struct RepliesResponse {
    #[serde(default)]
    replies: Vec<Tweet>,
}

#[derive(Debug, Deserialize)]
struct Tweet {
    id: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    text: String,
    #[serde(default, rename = "createdAt")]
    created_at: String,
    author: Option<Author>,
    entities: Option<Entities>,
}

#[derive(Debug, Deserialize)]
struct Author {
    #[serde(default, rename = "userName")]
    user_name: String,
}

#[derive(Debug, Deserialize)]
struct Entities {
    #[serde(default)]
    urls: Vec<UrlEntity>,
}

#[derive(Debug, Deserialize)]
struct UrlEntity {
    #[serde(default, rename = "expanded_url")]
    expanded_url: String,
}

#[async_trait]
impl MediaSource for FunkstudyAdapter {
    fn id(&self) -> &'static str {
        "funkstudy"
    }

    async fn list_candidates(&self) -> AppResult<Vec<CandidateRef>> {
        let since = (Utc::now() - chrono::Duration::days(self.backfill_days))
            .format("%Y-%m-%d")
            .to_string();
        let query = format!(
            "from:{} #yetanotherfunkstudy since:{}",
            self.screen_name, since
        );
        let mut out = Vec::new();
        let mut cursor = String::new();
        for page in 0..self.max_search_pages {
            let resp: SearchResponse = self
                .client
                .get(format!("{}/twitter/tweet/advanced_search", self.base_url))
                .header("X-API-Key", &self.api_key)
                .query(&[
                    ("query", query.as_str()),
                    ("queryType", "Latest"),
                    ("cursor", cursor.as_str()),
                ])
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            for t in resp.tweets {
                let source_url = if t.url.is_empty() {
                    format!("https://x.com/{}/status/{}", self.screen_name, t.id)
                } else {
                    t.url
                };
                out.push(CandidateRef {
                    source_external_id: t.id,
                    source_url,
                });
            }
            if !resp.has_next_page || resp.next_cursor.is_empty() {
                break;
            }
            cursor = resp.next_cursor;
            if page + 1 == self.max_search_pages {
                tracing::warn!(
                    pages = self.max_search_pages,
                    "funkstudy advanced_search hit page cap; older posts may be truncated"
                );
            }
        }
        Ok(out)
    }

    async fn fetch_and_extract(
        &self,
        candidate: &CandidateRef,
    ) -> AppResult<Option<NewRecommendation>> {
        let resp: RepliesResponse = self
            .client
            .get(format!("{}/twitter/tweet/replies", self.base_url))
            .header("X-API-Key", &self.api_key)
            .query(&[("tweetId", candidate.source_external_id.as_str()), ("cursor", "")])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        for reply in resp.replies {
            let is_self = reply
                .author
                .as_ref()
                .map(|a| a.user_name.eq_ignore_ascii_case(&self.screen_name))
                .unwrap_or(false);
            if !is_self {
                continue;
            }
            let mut urls: Vec<String> = reply
                .entities
                .as_ref()
                .map(|e| e.urls.iter().map(|u| u.expanded_url.clone()).collect())
                .unwrap_or_default();
            urls.push(reply.text.clone());
            if let Some(album_url) = first_spotify_album_url(&urls) {
                let featured_at = created_at_to_jst_date(&reply.created_at)
                    .unwrap_or_else(|| (Utc::now() + chrono::Duration::hours(9)).date_naive());
                return Ok(Some(NewRecommendation {
                    source_id: "funkstudy".into(),
                    source_url: candidate.source_url.clone(),
                    source_external_id: candidate.source_external_id.clone(),
                    featured_at,
                    artist_name: String::new(),
                    album_name: None,
                    track_name: None,
                    spotify_url: Some(album_url),
                    spotify_image_url: None,
                    youtube_url: None,
                }));
            }
        }

        // 本体は見つかったが Spotify 返信がまだ無い。 transient として次回リトライさせる
        // （None だと mark_scraped されて後追い返信を永久に取り逃す）。
        Err(AppError::Retryable(format!(
            "spotify album reply not found for tweet {}",
            candidate.source_external_id
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(format!("tests/fixtures/funkstudy/{name}")).unwrap()
    }

    #[test]
    fn created_at_parses_to_jst_date() {
        // 2026-05-30 23:30 UTC は JST で 2026-05-31
        let d = created_at_to_jst_date("Sat May 30 23:30:00 +0000 2026").unwrap();
        assert_eq!(d, NaiveDate::from_ymd_opt(2026, 5, 31).unwrap());
    }

    #[test]
    fn first_spotify_album_url_picks_album_links_only() {
        let urls = vec![
            "https://example.com".to_string(),
            "https://open.spotify.com/album/ABC?si=1".to_string(),
        ];
        assert_eq!(
            first_spotify_album_url(&urls),
            Some("https://open.spotify.com/album/ABC".to_string())
        );
        assert_eq!(
            first_spotify_album_url(&["https://open.spotify.com/track/Z".to_string()]),
            None
        );
    }

    #[tokio::test]
    async fn list_candidates_returns_funkstudy_posts() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/twitter/tweet/advanced_search"))
            .respond_with(ResponseTemplate::new(200).set_body_string(fixture("search.json")))
            .mount(&server)
            .await;

        let adapter = FunkstudyAdapter::new("key".into(), "taizooo".into(), 30)
            .with_base_url(server.uri());
        let cands = adapter.list_candidates().await.unwrap();
        assert_eq!(cands.len(), 1);
        assert_eq!(cands[0].source_external_id, "1001");
        assert_eq!(cands[0].source_url, "https://x.com/taizooo/status/1001");
    }

    #[tokio::test]
    async fn fetch_and_extract_pulls_spotify_from_self_reply() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/twitter/tweet/replies"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(fixture("replies_with_spotify.json")),
            )
            .mount(&server)
            .await;

        let adapter = FunkstudyAdapter::new("key".into(), "taizooo".into(), 30)
            .with_base_url(server.uri());
        let cand = CandidateRef {
            source_external_id: "1001".into(),
            source_url: "https://x.com/taizooo/status/1001".into(),
        };
        let rec = adapter.fetch_and_extract(&cand).await.unwrap().unwrap();
        // taizooo の返信の entities.urls から拾い、他人の返信(SHOULDNOTWIN)は無視する
        assert_eq!(
            rec.spotify_url.as_deref(),
            Some("https://open.spotify.com/album/4aawyAB9vmqN3uQ7FjRGTy")
        );
        assert_eq!(rec.source_external_id, "1001");
        assert_eq!(rec.featured_at, NaiveDate::from_ymd_opt(2026, 5, 30).unwrap());
        assert!(rec.artist_name.is_empty()); // ピプラインが解決して埋める
    }

    #[tokio::test]
    async fn fetch_and_extract_errors_retryable_when_no_spotify_reply() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/twitter/tweet/replies"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(fixture("replies_without_spotify.json")),
            )
            .mount(&server)
            .await;

        let adapter = FunkstudyAdapter::new("key".into(), "taizooo".into(), 30)
            .with_base_url(server.uri());
        let cand = CandidateRef {
            source_external_id: "1001".into(),
            source_url: "https://x.com/taizooo/status/1001".into(),
        };
        let err = adapter.fetch_and_extract(&cand).await.unwrap_err();
        assert!(matches!(err, AppError::Retryable(_)));
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --features ssr --lib server::adapter::funkstudy`
Expected: 5 件 PASS（コンパイル含む）。`cargo test` の作業ディレクトリはクレートルートなので `tests/fixtures/...` の相対パスで読める。

- [ ] **Step 5: Commit**

```bash
git add src/server/adapter/funkstudy.rs src/server/adapter/mod.rs tests/fixtures/funkstudy/
git commit -m "feat(funkstudy): twitterapi.io から #yetanotherfunkstudy を拾う MediaSource を追加"
```

---

## Task 6: Config に funkstudy_* を追加

**Files:**
- Modify: `src/server/config.rs`

- [ ] **Step 1: Write the failing test**

`src/server/config.rs` の `mod tests` に追加:

```rust
    #[test]
    fn funkstudy_defaults_when_env_absent() {
        let saved_db = std::env::var("DATABASE_URL").ok();
        let saved_key = std::env::var("FUNKSTUDY_API_KEY").ok();
        let saved_name = std::env::var("FUNKSTUDY_SCREEN_NAME").ok();
        let saved_days = std::env::var("FUNKSTUDY_BACKFILL_DAYS").ok();
        let saved_enabled = std::env::var("FUNKSTUDY_ENABLED").ok();
        std::env::remove_var("FUNKSTUDY_API_KEY");
        std::env::remove_var("FUNKSTUDY_SCREEN_NAME");
        std::env::remove_var("FUNKSTUDY_BACKFILL_DAYS");
        std::env::remove_var("FUNKSTUDY_ENABLED");
        std::env::set_var("DATABASE_URL", "sqlite::memory:");
        std::env::set_var("SPOTIFY_CLIENT_ID", "x");
        std::env::set_var("SPOTIFY_CLIENT_SECRET", "y");

        let cfg = Config::from_env().unwrap();
        assert!(cfg.funkstudy_api_key.is_none());
        assert_eq!(cfg.funkstudy_screen_name, "taizooo");
        assert_eq!(cfg.funkstudy_backfill_days, 30);
        assert!(cfg.funkstudy_enabled);

        if let Some(v) = saved_key { std::env::set_var("FUNKSTUDY_API_KEY", v); }
        if let Some(v) = saved_name { std::env::set_var("FUNKSTUDY_SCREEN_NAME", v); }
        if let Some(v) = saved_days { std::env::set_var("FUNKSTUDY_BACKFILL_DAYS", v); }
        if let Some(v) = saved_enabled { std::env::set_var("FUNKSTUDY_ENABLED", v); }
        if let Some(v) = saved_db { std::env::set_var("DATABASE_URL", v); } else { std::env::remove_var("DATABASE_URL"); }
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --features ssr --lib server::config::tests::funkstudy_defaults_when_env_absent`
Expected: コンパイルエラー（フィールドが無い）

- [ ] **Step 3: Write minimal implementation**

`Config` struct に追加（`scrape_throttle_ms` の下）:

```rust
    /// twitterapi.io の API キー。 未設定なら funkstudy ソースは登録されない
    pub funkstudy_api_key: Option<String>,
    pub funkstudy_enabled: bool,
    pub funkstudy_screen_name: String,
    pub funkstudy_backfill_days: i64,
```

`from_env` の `scrape_throttle_ms: ...` の下に追加:

```rust
            funkstudy_api_key: std::env::var("FUNKSTUDY_API_KEY")
                .ok()
                .filter(|s| !s.is_empty()),
            funkstudy_enabled: std::env::var("FUNKSTUDY_ENABLED").ok().as_deref() != Some("0"),
            funkstudy_screen_name: std::env::var("FUNKSTUDY_SCREEN_NAME")
                .ok()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "taizooo".into()),
            funkstudy_backfill_days: std::env::var("FUNKSTUDY_BACKFILL_DAYS")
                .ok()
                .and_then(|v| v.parse::<i64>().ok())
                .unwrap_or(30),
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --features ssr --lib server::config`
Expected: 既存 + 新規すべて PASS

- [ ] **Step 5: Commit**

```bash
git add src/server/config.rs
git commit -m "feat(config): funkstudy_* 環境変数を追加"
```

---

## Task 7: 配線（main.rs / bin/scrape.rs）

**Files:**
- Modify: `src/main.rs`
- Modify: `src/bin/scrape.rs`

ユニットテスト対象外（バイナリ配線）なので `cargo build --features ssr` の成功と `cargo clippy` で検証する。

- [ ] **Step 1: main.rs に use を追加**

`src/main.rs:5` の `use ...pitchfork::PitchforkAdapter;` の下に:

```rust
    use i_am_rockin_on::server::adapter::funkstudy::FunkstudyAdapter;
```

- [ ] **Step 2: pitchfork pipeline の下に funkstudy pipeline を追加**

`src/main.rs:68`（pitchfork_pipeline 定義の閉じ）の直後に:

```rust
    let funkstudy_on = cfg.funkstudy_enabled && cfg.funkstudy_api_key.is_some();
    let funkstudy_pipeline = funkstudy_on.then(|| {
        let source: Arc<dyn MediaSource> = Arc::new(FunkstudyAdapter::new(
            cfg.funkstudy_api_key.clone().expect("checked by funkstudy_on"),
            cfg.funkstudy_screen_name.clone(),
            cfg.funkstudy_backfill_days,
        ));
        Arc::new(ScrapePipeline {
            source,
            resolver: resolver.clone(),
            repo: repo.clone(),
            log: log.clone(),
            cancel: cancel.clone(),
            throttle_ms: cfg.scrape_throttle_ms,
        })
    });
    if !funkstudy_on {
        tracing::info!("funkstudy source disabled (FUNKSTUDY_API_KEY 未設定 or FUNKSTUDY_ENABLED=0)");
    }
```

- [ ] **Step 3: 初回スクレイプに funkstudy を追加**

`src/main.rs:91`（pitchfork の初回スクレイプ block の閉じ `}`）の直後、`}` (if !scrape_disabled の閉じ) の前に:

```rust
        if let Some(p) = funkstudy_pipeline.clone() {
            let l = log.clone();
            tokio::spawn(async move {
                if let Err(e) = run_initial_scrape_if_empty(p, l, "funkstudy").await {
                    tracing::error!(error = %e, "initial funkstudy scrape failed");
                }
            });
        }
```

- [ ] **Step 4: cron 登録に funkstudy を追加**

`src/main.rs:97`（pitchfork の `add_scrape_job`）の下に:

```rust
        if let Some(p) = funkstudy_pipeline.clone() {
            // UTC 22:00 = JST 07:00（rokinon 04:00 / pitchfork 16:00 とずらす）
            add_scrape_job(&scheduler, p, "0 0 22 * * *").await?;
        }
```

- [ ] **Step 5: bin/scrape.rs に match arm を追加**

`src/bin/scrape.rs:5` の下に use を追加:

```rust
    use i_am_rockin_on::server::adapter::funkstudy::FunkstudyAdapter;
```

`src/bin/scrape.rs:46`（pitchfork arm の閉じ `)),`）の下、`other =>` の前に:

```rust
        "funkstudy" => {
            let key = cfg
                .funkstudy_api_key
                .clone()
                .ok_or_else(|| anyhow::anyhow!("FUNKSTUDY_API_KEY required for --source funkstudy"))?;
            Arc::new(FunkstudyAdapter::new(
                key,
                cfg.funkstudy_screen_name.clone(),
                cfg.funkstudy_backfill_days,
            ))
        }
```

- [ ] **Step 6: ビルドと lint で検証**

Run: `cargo build --features ssr --bins`
Expected: 成功（warning 0 が理想）

Run: `cargo clippy --features ssr --bins -- -D warnings`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add src/main.rs src/bin/scrape.rs
git commit -m "feat: funkstudy pipeline を main の初回/定期スクレイプと scrape CLI に配線"
```

---

## Task 8: README を更新

**Files:**
- Modify: `README.md`

- [ ] **Step 1: ソース一覧に追記**

`README.md:42`（pitchfork の行）の下に追加:

```markdown
- **funkstudy** — X の taizooo (`FUNKSTUDY_SCREEN_NAME`) の `#yetanotherfunkstudy` 付きポストを twitterapi.io 経由で拾い、 ぶら下がる返信中の Spotify アルバム URL から取り込む
```

- [ ] **Step 2: scrape 例に追記**

`README.md:28`（`--source pitchfork` の行）の下に追加:

```markdown
# 別ソース指定: cargo run --features ssr --bin scrape -- --source funkstudy
```

- [ ] **Step 3: 環境変数テーブルに追記**

`README.md:56`（`PITCHFORK_MAX_PAGES` の行）の下に追加:

```markdown
| `FUNKSTUDY_API_KEY` | no | — | twitterapi.io の API キー。 未設定なら funkstudy ソースは無効 |
| `FUNKSTUDY_ENABLED` | no | `1` | `0` で funkstudy ソースを無効化 |
| `FUNKSTUDY_SCREEN_NAME` | no | `taizooo` | 監視する X アカウント |
| `FUNKSTUDY_BACKFILL_DAYS` | no | `30` | advanced_search で遡る日数 |
```

- [ ] **Step 4: Commit**

```bash
git add README.md
git commit -m "docs(readme): funkstudy ソースと環境変数を追記"
```

---

## 最終検証

- [ ] **全テスト**: `cargo test --features ssr` → 全 PASS
- [ ] **clippy**: `cargo clippy --features ssr --all-targets -- -D warnings` → PASS
- [ ] **fmt**: `cargo fmt --check` → PASS（崩れていれば `cargo fmt` して追加コミット）

---

## 注意 / 既知の前提

- **twitterapi.io 課金**: 実行には有効な `FUNKSTUDY_API_KEY` と従量課金アカウントが必要。未設定なら main は funkstudy を登録せずスキップする（他ソースは通常動作）。
- **本体 id == conversation root の前提**: `#yetanotherfunkstudy` 本体はオリジナル投稿なので、その tweet id を replies の `tweetId` に使う。万一本体が誰かへの返信だった場合はスレッド root がずれるが、運用上発生しない想定。
- **featured_at**: Spotify 返信の `createdAt`（JST 日付）を採用。本体投稿と同日のことがほとんど。
- **返信ページング**: replies は 1 ページ目（最大 20 件）のみ走査する。taizooo の Spotify 返信は本体直後の自己返信なので 1 ページ目に入る。将来取りこぼしが判明したらページングを足す。
- **スキーマ確認済み（twitterapi.io docs ベース）**: `createdAt` は classic Twitter 形式 `"Tue Dec 10 07:00:30 +0000 2024"`（パーサ `%a %b %d %H:%M:%S %z %Y` と一致）、`id` は string、`entities.urls[].expanded_url` 存在。サイレント日付バグ（ISO-8601 なら featured_at が全て「今日」になる）のリスクは解消済み。
- **能動的スモークテスト（キー入手後・必須）**: `cargo run --features ssr --bin scrape -- --source funkstudy` を実行。「クラッシュしない」だけでは不十分 — `#[serde(default)]` だらけなので統合不整合はサイレントに空を返す。最低限:
  1. `list_candidates` が既知のアクティブ期間で **非空** を返すこと（空なら `from:` / `since:` / ハッシュタグ検索のどれかが効いていない）。
  2. 既知の 1 投稿が end-to-end で解決し、`recommendations` に正しい artist / album / featured_at で入ること。
  3. **ハッシュタグ位置の前提検証**: 本実装は `#yetanotherfunkstudy` が画像本体側に付き Spotify URL は返信側にある前提（検索 root → 返信走査）。もしハッシュタグが返信側に付くなら検索と走査が逆転して 0 件になる。実データで要確認。
  差異があれば fixture と deserialize struct を実レスポンスに合わせて修正する。
