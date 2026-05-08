# 音楽メディア推薦集約サイト 実装プラン

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Leptos + fly.io + SQLite で、ロキノンには騙されないぞの「推し」記事を定期スクレイピングし、Spotify Search API でマッチングしてジャケット画像とリンクを並べた一覧サイトを構築する。

**Architecture:** 単一 Cargo crate、cargo-leptos でビルド。SSR（Axum）+ tokio cron + sqlx(SQLite) を1プロセス同居。adapter/resolver/store/scheduler の責務を `cfg(feature="ssr")` でサーバ専用モジュールに閉じ込め、Leptos コンポーネントは hydrate と共有。

**Tech Stack:**

- Rust 1.95 stable / cargo-leptos（mise でバージョン管理）
- Leptos 0.7（ssr + hydrate）
- Axum 0.7
- sqlx 0.8（sqlite, runtime-tokio, rustls）
- reqwest 0.12（rustls, json）
- scraper 0.20（HTML パース）
- serde_json（埋め込み JSON 抽出）
- regex（推しマーカー検出）
- tokio_cron_scheduler 0.13
- tracing + tracing-subscriber
- chrono 0.4
- thiserror / anyhow
- clap 4（CLI）
- wiremock（テスト）
- fly.io（fly volume + Dockerfile multi-stage）

**Spec:** `docs/superpowers/specs/2026-05-08-music-recommendations-aggregator-design.md`

---

## Phase 1: プロジェクト基盤

### Task 1: Rust ツールチェーン確認と cargo-leptos 導入

**Files:**
- Create: `mise.toml` (or verify if existing)
- Verify: `~/.cargo` (Rust toolchain)

- [ ] **Step 1: mise でツールバージョンをピン**

`mise.toml` に rust と cargo-leptos / sqlx-cli を記載：

```toml
[tools]
rust = "1.95"
"cargo:cargo-leptos" = "latest"
"cargo:sqlx-cli" = "latest"
```

```bash
mise install
mise current
```

Expected: rust 1.95 と cargo-leptos が PATH に通っとる。

- [ ] **Step 2: wasm32 target 追加**

```bash
rustup target add wasm32-unknown-unknown
```

- [ ] **Step 3: cargo-leptos 動作確認**

```bash
cargo leptos --version
```

Expected: `cargo-leptos 0.2.x` 以降。

### Task 2: Cargo crate 初期化と Leptos scaffold 構造作成

**Files:**
- Create: `Cargo.toml`
- Create: `src/lib.rs`
- Create: `src/main.rs`
- Create: `src/pages/mod.rs`
- Create: `src/pages/home.rs`
- Create: `style/main.css`
- Create: `assets/.gitkeep`
- Create: `.gitignore`

- [ ] **Step 1: `.gitignore` 作成**

```gitignore
/target
/dist
/.env
*.db
*.db-journal
.DS_Store
```

- [ ] **Step 2: `Cargo.toml` 作成（cargo-leptos 標準構成）**

```toml
[package]
name = "i-am-rockin-on"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib", "rlib"]

[[bin]]
name = "scrape"
path = "src/bin/scrape.rs"
required-features = ["ssr"]

[dependencies]
leptos = "0.7"
leptos_meta = "0.7"
leptos_router = "0.7"
leptos_axum = { version = "0.7", optional = true }
console_error_panic_hook = "0.1"
serde = { version = "1", features = ["derive"] }
chrono = { version = "0.4", features = ["serde"] }
thiserror = "2"

axum = { version = "0.7", optional = true }
tokio = { version = "1", features = ["full"], optional = true }
tower = { version = "0.5", optional = true }
tower-http = { version = "0.6", features = ["fs"], optional = true }
sqlx = { version = "0.8", features = ["runtime-tokio", "tls-rustls", "sqlite", "chrono", "macros"], optional = true }
reqwest = { version = "0.12", default-features = false, features = ["rustls-tls", "json", "gzip"], optional = true }
scraper = { version = "0.20", optional = true }
serde_json = { version = "1", optional = true }
regex = { version = "1", optional = true }
tokio-cron-scheduler = { version = "0.13", optional = true }
tracing = { version = "0.1", optional = true }
tracing-subscriber = { version = "0.3", features = ["env-filter"], optional = true }
anyhow = { version = "1", optional = true }
clap = { version = "4", features = ["derive"], optional = true }
async-trait = { version = "0.1", optional = true }

[dev-dependencies]
wiremock = "0.6"
tokio = { version = "1", features = ["full", "test-util"] }

[features]
hydrate = ["leptos/hydrate"]
ssr = [
  "dep:axum",
  "dep:tokio",
  "dep:tower",
  "dep:tower-http",
  "dep:leptos_axum",
  "dep:sqlx",
  "dep:reqwest",
  "dep:scraper",
  "dep:serde_json",
  "dep:regex",
  "dep:tokio-cron-scheduler",
  "dep:tracing",
  "dep:tracing-subscriber",
  "dep:anyhow",
  "dep:clap",
  "dep:async-trait",
  "leptos/ssr",
  "leptos_meta/ssr",
  "leptos_router/ssr",
]

[package.metadata.leptos]
output-name = "i-am-rockin-on"
site-root = "target/site"
site-pkg-dir = "pkg"
style-file = "style/main.css"
assets-dir = "assets"
site-addr = "0.0.0.0:3000"
reload-port = 3001
browserquery = "defaults"
watch = false
env = "DEV"
bin-features = ["ssr"]
bin-default-features = false
lib-features = ["hydrate"]
lib-default-features = false
```

- [ ] **Step 3: `src/lib.rs` に Leptos エントリ作成（最小骨格）**

```rust
use leptos::prelude::*;
use leptos_meta::*;
use leptos_router::{components::*, StaticSegment};

pub mod pages;

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();
    view! {
        <Stylesheet id="leptos" href="/pkg/i-am-rockin-on.css"/>
        <Title text="I am rockin on"/>
        <Router>
            <main>
                <Routes fallback=|| "Not found.">
                    <Route path=StaticSegment("") view=pages::home::Home/>
                </Routes>
            </main>
        </Router>
    }
}

#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    console_error_panic_hook::set_once();
    leptos::mount::hydrate_body(App);
}
```

- [ ] **Step 4: `src/pages/mod.rs` と `src/pages/home.rs`**

```rust
// src/pages/mod.rs
pub mod home;
```

```rust
// src/pages/home.rs
use leptos::prelude::*;

#[component]
pub fn Home() -> impl IntoView {
    view! {
        <h1>"I am rockin on"</h1>
        <p>"音楽メディアの『推し』を集めたページずら"</p>
    }
}
```

- [ ] **Step 5: `src/main.rs`（SSR サーバ entry の最小骨格）**

```rust
#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() {
    use axum::Router;
    use i_am_rockin_on::App;
    use leptos::prelude::*;
    use leptos_axum::{generate_route_list, LeptosRoutes};

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let conf = get_configuration(None).unwrap();
    let leptos_options = conf.leptos_options;
    let addr = leptos_options.site_addr;
    let routes = generate_route_list(App);

    let app = Router::new()
        .leptos_routes(&leptos_options, routes, {
            let opts = leptos_options.clone();
            move || leptos::prelude::shell(opts.clone())
        })
        .fallback(leptos_axum::file_and_error_handler(leptos::prelude::shell))
        .with_state(leptos_options);

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    tracing::info!("listening on http://{}", &addr);
    axum::serve(listener, app.into_make_service()).await.unwrap();
}

#[cfg(not(feature = "ssr"))]
pub fn main() {}
```

- [ ] **Step 6: `style/main.css`（最低限）**

```css
* { box-sizing: border-box; }
body { font-family: system-ui, -apple-system, sans-serif; margin: 0; padding: 1rem; }
main { max-width: 1200px; margin: 0 auto; }
```

- [ ] **Step 7: `assets/.gitkeep` 作成（空ファイル）と `src/bin/scrape.rs` の最小スタブ**

```rust
// src/bin/scrape.rs
#[cfg(feature = "ssr")]
fn main() {
    println!("scrape CLI placeholder");
}

#[cfg(not(feature = "ssr"))]
fn main() {}
```

- [ ] **Step 8: ビルドが通ることを確認**

```bash
cargo leptos build
```

Expected: 警告は出るかもしれんが `error` 無しで `target/debug/i-am-rockin-on` バイナリができとる。

- [ ] **Step 9: 開発サーバ起動して動作確認**

```bash
cargo leptos watch
```

Expected: `http://localhost:3000` にアクセスして「I am rockin on」と「音楽メディアの『推し』を集めたページずら」が表示される。Ctrl+Cで止める。

- [ ] **Step 10: コミット**

```bash
git add .
git commit -m "Leptos SSRアプリの最小骨格を追加"
```

### Task 3: tracing 設定とエラー型の足場

**Files:**
- Create: `src/server/mod.rs`
- Create: `src/server/error.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: 失敗するテストを書く**

`src/server/error.rs` を作る。まずテスト：

```rust
// src/server/error.rs
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("parse error: {0}")]
    Parse(String),
    #[error("config error: {0}")]
    Config(String),
}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_error_displays_message() {
        let err = AppError::Parse("missing entry_text".into());
        assert_eq!(err.to_string(), "parse error: missing entry_text");
    }
}
```

- [ ] **Step 2: `src/server/mod.rs`（エラーを公開）**

```rust
// src/server/mod.rs
pub mod error;
```

- [ ] **Step 3: `src/lib.rs` に server モジュールを ssr 限定で追加**

`src/lib.rs` の冒頭付近に：

```rust
#[cfg(feature = "ssr")]
pub mod server;
```

- [ ] **Step 4: テスト実行**

```bash
cargo test --features ssr server::error::tests::parse_error_displays_message
```

Expected: PASS（1 test）。

- [ ] **Step 5: コミット**

```bash
git add src/lib.rs src/server/
git commit -m "AppError 型と server モジュールの足場を追加"
```

---

## Phase 2: ドメイン型と DB

### Task 4: Recommendation ドメイン型を定義

**Files:**
- Create: `src/domain/mod.rs`
- Create: `src/domain/recommendation.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: 失敗するテストを書く**

```rust
// src/domain/recommendation.rs
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recommendation {
    pub id: i64,
    pub source_id: String,
    pub source_url: String,
    pub source_external_id: String,
    pub featured_at: NaiveDate,
    pub artist_name: String,
    pub album_name: Option<String>,
    pub track_name: Option<String>,
    pub spotify_url: Option<String>,
    pub spotify_image_url: Option<String>,
    pub youtube_url: Option<String>,
    pub manual_override: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewRecommendation {
    pub source_id: String,
    pub source_url: String,
    pub source_external_id: String,
    pub featured_at: NaiveDate,
    pub artist_name: String,
    pub album_name: Option<String>,
    pub track_name: Option<String>,
    pub spotify_url: Option<String>,
    pub spotify_image_url: Option<String>,
    pub youtube_url: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_recommendation_can_be_constructed_with_minimal_fields() {
        let n = NewRecommendation {
            source_id: "rokinon".into(),
            source_url: "https://ameblo.jp/stamedba/entry-1.html".into(),
            source_external_id: "1".into(),
            featured_at: NaiveDate::from_ymd_opt(2026, 4, 1).unwrap(),
            artist_name: "Angelo De Augustine".into(),
            album_name: Some("Angel in Plainclothes".into()),
            track_name: None,
            spotify_url: None,
            spotify_image_url: None,
            youtube_url: None,
        };
        assert_eq!(n.artist_name, "Angelo De Augustine");
        assert!(n.spotify_url.is_none());
    }
}
```

- [ ] **Step 2: モジュールを公開**

```rust
// src/domain/mod.rs
pub mod recommendation;
```

`src/lib.rs` に追加：

```rust
pub mod domain;
```

- [ ] **Step 3: テスト実行**

```bash
cargo test --features ssr domain::
```

Expected: PASS。

- [ ] **Step 4: コミット**

```bash
git add src/domain/ src/lib.rs
git commit -m "Recommendation/NewRecommendation ドメイン型を追加"
```

### Task 5: SQLite マイグレーション作成

**Files:**
- Create: `migrations/20260508000001_init.sql`
- Modify: `Cargo.toml`（既に sqlx は依存済）

- [ ] **Step 1: マイグレーションファイル作成**

```sql
-- migrations/20260508000001_init.sql
CREATE TABLE recommendations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id TEXT NOT NULL,
    source_url TEXT NOT NULL,
    source_external_id TEXT NOT NULL,
    featured_at TEXT NOT NULL,
    artist_name TEXT NOT NULL,
    album_name TEXT,
    track_name TEXT,
    spotify_url TEXT,
    spotify_image_url TEXT,
    youtube_url TEXT,
    manual_override INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE (source_id, source_external_id)
);
CREATE INDEX idx_recommendations_featured_at ON recommendations (featured_at DESC);

CREATE TABLE scrape_runs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id TEXT NOT NULL,
    started_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    finished_at TEXT,
    status TEXT NOT NULL,
    items_added INTEGER NOT NULL DEFAULT 0,
    items_updated INTEGER NOT NULL DEFAULT 0,
    error_message TEXT
);
```

- [ ] **Step 2: ローカルで動作確認用 DB 作成**

```bash
mkdir -p data
export DATABASE_URL="sqlite:data/app.db"
cargo install sqlx-cli --no-default-features --features sqlite,rustls
sqlx database create
sqlx migrate run
```

Expected: `data/app.db` に2テーブルできとる。

```bash
sqlite3 data/app.db ".schema"
```

Expected: 上の SQL がそのまま反映されとる。

- [ ] **Step 3: コミット**

```bash
git add migrations/
git commit -m "recommendations と scrape_runs テーブルの初期マイグレーション"
```

### Task 6: Recommendation リポジトリ — UPSERT のテスト先行

**Files:**
- Create: `src/server/store.rs`
- Modify: `src/server/mod.rs`

- [ ] **Step 1: 失敗するテストを書く**

```rust
// src/server/store.rs
use crate::domain::recommendation::{NewRecommendation, Recommendation};
use crate::server::error::AppResult;
use sqlx::SqlitePool;

pub struct RecommendationRepo {
    pool: SqlitePool,
}

impl RecommendationRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// UPSERT。既存で manual_override=1 の場合は Spotify 系フィールドだけ保持し、
    /// それ以外（artist/album/youtube/featured_at）は更新する。
    /// 戻り値: (saved, was_inserted)
    pub async fn upsert(&self, item: NewRecommendation) -> AppResult<(Recommendation, bool)> {
        let mut tx = self.pool.begin().await?;
        let existing: Option<Recommendation> = sqlx::query_as!(
            Recommendation,
            r#"SELECT id as "id!", source_id, source_url, source_external_id,
                      featured_at as "featured_at: chrono::NaiveDate",
                      artist_name, album_name, track_name,
                      spotify_url, spotify_image_url, youtube_url,
                      manual_override as "manual_override!: bool",
                      created_at as "created_at: chrono::DateTime<chrono::Utc>",
                      updated_at as "updated_at: chrono::DateTime<chrono::Utc>"
               FROM recommendations
               WHERE source_id = ? AND source_external_id = ?"#,
            item.source_id,
            item.source_external_id,
        )
        .fetch_optional(&mut *tx)
        .await?;

        let (saved, was_inserted) = match existing {
            Some(prev) if prev.manual_override => {
                // Spotify 系は維持、それ以外を更新
                let row = sqlx::query_as!(
                    Recommendation,
                    r#"UPDATE recommendations SET
                        source_url = ?, featured_at = ?, artist_name = ?,
                        album_name = ?, track_name = ?, youtube_url = ?,
                        updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                       WHERE id = ?
                       RETURNING id as "id!", source_id, source_url, source_external_id,
                                 featured_at as "featured_at: chrono::NaiveDate",
                                 artist_name, album_name, track_name,
                                 spotify_url, spotify_image_url, youtube_url,
                                 manual_override as "manual_override!: bool",
                                 created_at as "created_at: chrono::DateTime<chrono::Utc>",
                                 updated_at as "updated_at: chrono::DateTime<chrono::Utc>""#,
                    item.source_url, item.featured_at, item.artist_name,
                    item.album_name, item.track_name, item.youtube_url,
                    prev.id,
                )
                .fetch_one(&mut *tx).await?;
                (row, false)
            }
            Some(prev) => {
                let row = sqlx::query_as!(
                    Recommendation,
                    r#"UPDATE recommendations SET
                        source_url = ?, featured_at = ?, artist_name = ?,
                        album_name = ?, track_name = ?,
                        spotify_url = ?, spotify_image_url = ?, youtube_url = ?,
                        updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                       WHERE id = ?
                       RETURNING id as "id!", source_id, source_url, source_external_id,
                                 featured_at as "featured_at: chrono::NaiveDate",
                                 artist_name, album_name, track_name,
                                 spotify_url, spotify_image_url, youtube_url,
                                 manual_override as "manual_override!: bool",
                                 created_at as "created_at: chrono::DateTime<chrono::Utc>",
                                 updated_at as "updated_at: chrono::DateTime<chrono::Utc>""#,
                    item.source_url, item.featured_at, item.artist_name,
                    item.album_name, item.track_name,
                    item.spotify_url, item.spotify_image_url, item.youtube_url,
                    prev.id,
                )
                .fetch_one(&mut *tx).await?;
                (row, false)
            }
            None => {
                let row = sqlx::query_as!(
                    Recommendation,
                    r#"INSERT INTO recommendations
                        (source_id, source_url, source_external_id, featured_at,
                         artist_name, album_name, track_name,
                         spotify_url, spotify_image_url, youtube_url)
                       VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                       RETURNING id as "id!", source_id, source_url, source_external_id,
                                 featured_at as "featured_at: chrono::NaiveDate",
                                 artist_name, album_name, track_name,
                                 spotify_url, spotify_image_url, youtube_url,
                                 manual_override as "manual_override!: bool",
                                 created_at as "created_at: chrono::DateTime<chrono::Utc>",
                                 updated_at as "updated_at: chrono::DateTime<chrono::Utc>""#,
                    item.source_id, item.source_url, item.source_external_id,
                    item.featured_at, item.artist_name, item.album_name, item.track_name,
                    item.spotify_url, item.spotify_image_url, item.youtube_url,
                )
                .fetch_one(&mut *tx).await?;
                (row, true)
            }
        };
        tx.commit().await?;
        Ok((saved, was_inserted))
    }

    pub async fn list_recent(&self, limit: i64) -> AppResult<Vec<Recommendation>> {
        let rows = sqlx::query_as!(
            Recommendation,
            r#"SELECT id as "id!", source_id, source_url, source_external_id,
                      featured_at as "featured_at: chrono::NaiveDate",
                      artist_name, album_name, track_name,
                      spotify_url, spotify_image_url, youtube_url,
                      manual_override as "manual_override!: bool",
                      created_at as "created_at: chrono::DateTime<chrono::Utc>",
                      updated_at as "updated_at: chrono::DateTime<chrono::Utc>"
               FROM recommendations
               ORDER BY featured_at DESC, id DESC
               LIMIT ?"#,
            limit,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn setup_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();
        pool
    }

    fn sample(id: &str) -> NewRecommendation {
        NewRecommendation {
            source_id: "rokinon".into(),
            source_url: format!("https://ameblo.jp/stamedba/entry-{id}.html"),
            source_external_id: id.into(),
            featured_at: NaiveDate::from_ymd_opt(2026, 4, 1).unwrap(),
            artist_name: "Angelo De Augustine".into(),
            album_name: Some("Angel in Plainclothes".into()),
            track_name: None,
            spotify_url: Some("https://open.spotify.com/album/abc".into()),
            spotify_image_url: Some("https://i.scdn.co/image/abc.jpg".into()),
            youtube_url: Some("https://www.youtube.com/watch?v=xyz".into()),
        }
    }

    #[tokio::test]
    async fn upsert_inserts_when_new() {
        let pool = setup_pool().await;
        let repo = RecommendationRepo::new(pool);
        let (saved, inserted) = repo.upsert(sample("12963931773")).await.unwrap();
        assert!(inserted);
        assert_eq!(saved.artist_name, "Angelo De Augustine");
    }

    #[tokio::test]
    async fn upsert_updates_when_existing() {
        let pool = setup_pool().await;
        let repo = RecommendationRepo::new(pool);
        repo.upsert(sample("12963931773")).await.unwrap();
        let mut updated = sample("12963931773");
        updated.album_name = Some("Different Album".into());
        let (saved, inserted) = repo.upsert(updated).await.unwrap();
        assert!(!inserted);
        assert_eq!(saved.album_name.unwrap(), "Different Album");
    }
}
```

- [ ] **Step 2: `src/server/mod.rs` を更新**

```rust
// src/server/mod.rs
pub mod error;
pub mod store;
```

- [ ] **Step 3: `.env` で sqlx-cli/macros 用 DATABASE_URL を設定**

```bash
echo 'DATABASE_URL=sqlite:data/app.db' > .env
```

- [ ] **Step 4: テスト実行**

```bash
DATABASE_URL=sqlite:data/app.db cargo test --features ssr server::store::tests
```

Expected: 2 tests PASS。

- [ ] **Step 5: コミット**

```bash
git add src/server/ .env
git commit -m "RecommendationRepo の upsert/list_recent と統合テストを追加"
```

注意: `.env` は `.gitignore` で除外しとるけぇ、実際にはコミットされん。Step 3 で開発者ローカルにのみ作る。

(訂正) 上の `.gitignore` に `.env` 入れたけぇ追加されん。代わりに `.env.example` をコミットする：

```bash
echo 'DATABASE_URL=sqlite:data/app.db' > .env.example
git add .env.example src/server/
git commit -m "RecommendationRepo の upsert/list_recent と統合テストを追加"
```

### Task 7: scrape_runs ロギング用リポジトリ

**Files:**
- Create: `src/server/scrape_log.rs`
- Modify: `src/server/mod.rs`

- [ ] **Step 1: 失敗するテストを書く**

```rust
// src/server/scrape_log.rs
use crate::server::error::AppResult;
use sqlx::SqlitePool;

pub struct ScrapeLog {
    pool: SqlitePool,
}

#[derive(Debug, Clone)]
pub struct ScrapeRunHandle {
    pub run_id: i64,
}

impl ScrapeLog {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn start(&self, source_id: &str) -> AppResult<ScrapeRunHandle> {
        let row = sqlx::query!(
            r#"INSERT INTO scrape_runs (source_id, status) VALUES (?, 'running') RETURNING id"#,
            source_id,
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(ScrapeRunHandle { run_id: row.id })
    }

    pub async fn finish_success(
        &self,
        handle: &ScrapeRunHandle,
        items_added: i64,
        items_updated: i64,
    ) -> AppResult<()> {
        sqlx::query!(
            r#"UPDATE scrape_runs SET status='success',
                finished_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                items_added = ?, items_updated = ?
               WHERE id = ?"#,
            items_added, items_updated, handle.run_id,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn finish_error(&self, handle: &ScrapeRunHandle, message: &str) -> AppResult<()> {
        sqlx::query!(
            r#"UPDATE scrape_runs SET status='error',
                finished_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                error_message = ?
               WHERE id = ?"#,
            message, handle.run_id,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn count(&self, source_id: &str) -> AppResult<i64> {
        let row = sqlx::query!(
            r#"SELECT COUNT(*) as "n!: i64" FROM scrape_runs WHERE source_id = ?"#,
            source_id,
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(row.n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn setup() -> SqlitePool {
        let pool = SqlitePoolOptions::new().connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn start_then_success_records_counts() {
        let log = ScrapeLog::new(setup().await);
        let h = log.start("rokinon").await.unwrap();
        log.finish_success(&h, 3, 1).await.unwrap();
        assert_eq!(log.count("rokinon").await.unwrap(), 1);
    }

    #[tokio::test]
    async fn count_returns_zero_for_unknown_source() {
        let log = ScrapeLog::new(setup().await);
        assert_eq!(log.count("does-not-exist").await.unwrap(), 0);
    }
}
```

- [ ] **Step 2: モジュール公開**

`src/server/mod.rs` を更新：

```rust
pub mod error;
pub mod scrape_log;
pub mod store;
```

- [ ] **Step 3: テスト実行**

```bash
DATABASE_URL=sqlite:data/app.db cargo test --features ssr server::scrape_log::tests
```

Expected: 2 tests PASS。

- [ ] **Step 4: コミット**

```bash
git add src/server/scrape_log.rs src/server/mod.rs
git commit -m "ScrapeLog でスクレイプ実行履歴を記録"
```

---

## Phase 3: Rokinon アダプタ

ロキノンには騙されないぞは Ameblo SPA で、HTML 内の `window.INITIAL_STATE` JSON に記事本文（`entry_text`）が Unicode エスケープされた状態で埋め込まれとる。抽出は次の手順：

1. ページHTML取得
2. `window.INITIAL_STATE = {...};` を正規表現で抜き出して `serde_json::Value` にパース
3. 該当エントリの `entry_text` を取得（Unicode エスケープは serde_json が自動デコード）
4. `entry_text` を `scraper` で HTML パース
5. `\d{6}推し` パターンを本文中で検索
6. 記事タイトルから `{Artist Name} の新作` 形式でアーティスト名抽出
7. 本文 `<h2>` からアルバム名抽出（無ければ本文先頭テキストから）
8. `<a href>` から YouTube URL を拾う

### Task 8: テスト用フィクスチャを保存

**Files:**
- Create: `tests/fixtures/rokinon/oshi_article.html`
- Create: `tests/fixtures/rokinon/non_oshi_article.html`
- Create: `tests/fixtures/rokinon/entrylist_page1.html`

- [ ] **Step 1: 「推し」ありの記事フィクスチャ取得**

```bash
mkdir -p tests/fixtures/rokinon
curl -s -A "Mozilla/5.0 (compatible; i-am-rockin-on-fixture/1.0)" \
  "https://ameblo.jp/stamedba/entry-12963931773.html" \
  -o tests/fixtures/rokinon/oshi_article.html
```

Expected: 数百KB のファイル。`grep -c "推し" tests/fixtures/rokinon/oshi_article.html` が 2 以上。

- [ ] **Step 2: 「推し」無しの記事フィクスチャ取得**

```bash
curl -s -A "Mozilla/5.0 (compatible; i-am-rockin-on-fixture/1.0)" \
  "https://ameblo.jp/stamedba/entry-12963909942.html" \
  -o tests/fixtures/rokinon/non_oshi_article.html
```

- [ ] **Step 3: 記事一覧ページのフィクスチャ取得**

```bash
curl -s -A "Mozilla/5.0 (compatible; i-am-rockin-on-fixture/1.0)" \
  "https://ameblo.jp/stamedba/entrylist.html" \
  -o tests/fixtures/rokinon/entrylist_page1.html
```

- [ ] **Step 4: コミット**

```bash
git add tests/fixtures/rokinon/
git commit -m "ロキノン記事のテスト用 HTML フィクスチャを追加"
```

### Task 9: `window.INITIAL_STATE` JSON 抽出

**Files:**
- Create: `src/server/adapter/mod.rs`
- Create: `src/server/adapter/rokinon.rs`
- Modify: `src/server/mod.rs`

- [ ] **Step 1: 失敗するテストを書く**

```rust
// src/server/adapter/rokinon.rs
use crate::server::error::{AppError, AppResult};
use regex::Regex;
use serde_json::Value;

/// HTML ページから window.INITIAL_STATE の JSON を抜き出して serde_json::Value にする。
pub fn extract_initial_state(html: &str) -> AppResult<Value> {
    let re = Regex::new(r"window\.INITIAL_STATE\s*=\s*(\{.*?\});").unwrap();
    let caps = re
        .captures(html)
        .ok_or_else(|| AppError::Parse("window.INITIAL_STATE not found".into()))?;
    let json_str = &caps[1];
    serde_json::from_str(json_str).map_err(|e| AppError::Parse(format!("invalid JSON: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(format!("tests/fixtures/rokinon/{name}")).unwrap()
    }

    #[test]
    fn extract_initial_state_finds_entry_map() {
        let html = fixture("oshi_article.html");
        let state = extract_initial_state(&html).unwrap();
        let entry_map = state
            .pointer("/entryState/entryMap")
            .expect("entryMap path should exist");
        assert!(entry_map.is_object());
    }

    #[test]
    fn extract_initial_state_errors_when_missing() {
        let err = extract_initial_state("<html>no state</html>").unwrap_err();
        assert!(err.to_string().contains("INITIAL_STATE"));
    }
}
```

- [ ] **Step 2: モジュール構造**

```rust
// src/server/adapter/mod.rs
pub mod rokinon;
```

```rust
// src/server/mod.rs
pub mod adapter;
pub mod error;
pub mod scrape_log;
pub mod store;
```

- [ ] **Step 3: テスト実行（失敗確認）**

```bash
cargo test --features ssr server::adapter::rokinon::tests::extract_initial_state
```

Expected: 上のコードでビルドして PASS する想定。実フィクスチャの JSON path が違う場合に FAIL する場合は次の調整。

- [ ] **Step 4: 実 HTML で path 確認（FAIL したら）**

正しい path を見つける：

```bash
grep -oE 'window\.INITIAL_STATE = \{.*?\};' tests/fixtures/rokinon/oshi_article.html | head -c 500
```

`/entryState/entryMap` 以外が正しければテストの assertion を実 path に合わせて修正してから再 run。Phase 1 のリサーチで `entry_text` は `entryState.entryMap.<entry_id>.entry_text` にあると確認済み。

- [ ] **Step 5: テスト PASS 確認**

```bash
cargo test --features ssr server::adapter::rokinon
```

Expected: 2 tests PASS。

- [ ] **Step 6: コミット**

```bash
git add src/server/adapter/ src/server/mod.rs
git commit -m "Ameblo INITIAL_STATE JSON 抽出ロジックを追加"
```

### Task 10: 記事 ID から entry_text を取り出す

**Files:**
- Modify: `src/server/adapter/rokinon.rs`

- [ ] **Step 1: 失敗するテストを書く**

`src/server/adapter/rokinon.rs` の末尾に：

```rust
/// JSON state と entry_id から、entry_text の HTML 文字列を取り出す。
pub fn entry_text_for(state: &Value, entry_id: &str) -> AppResult<String> {
    let path = format!("/entryState/entryMap/{}/entry_text", entry_id);
    state
        .pointer(&path)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| AppError::Parse(format!("entry_text not found at {path}")))
}
```

テストモジュールに追加：

```rust
#[test]
fn entry_text_for_returns_html_with_p_tags() {
    let html = fixture("oshi_article.html");
    let state = extract_initial_state(&html).unwrap();
    let text = entry_text_for(&state, "12963931773").unwrap();
    assert!(text.contains("<p>") || text.contains("<a "), "expected HTML, got: {}", &text[..200.min(text.len())]);
    assert!(text.contains("推し"), "should mention 推し");
}
```

- [ ] **Step 2: テスト実行**

```bash
cargo test --features ssr server::adapter::rokinon::tests::entry_text_for_returns_html_with_p_tags
```

Expected: PASS。

- [ ] **Step 3: コミット**

```bash
git add src/server/adapter/rokinon.rs
git commit -m "entry_id から entry_text HTML を取り出す関数を追加"
```

### Task 11: 「推し」マーカーから featured_at を取り出す

**Files:**
- Modify: `src/server/adapter/rokinon.rs`

- [ ] **Step 1: 失敗するテストを書く**

```rust
use chrono::NaiveDate;

/// entry_text から `YYYYMM推し` パターンを探し、その月の1日を NaiveDate で返す。
/// 見つからなければ None。
pub fn detect_oshi(entry_text: &str) -> Option<NaiveDate> {
    let re = Regex::new(r"(\d{4})(\d{2})推し").unwrap();
    let caps = re.captures(entry_text)?;
    let year: i32 = caps[1].parse().ok()?;
    let month: u32 = caps[2].parse().ok()?;
    NaiveDate::from_ymd_opt(year, month, 1)
}
```

テスト追加：

```rust
#[test]
fn detect_oshi_returns_first_of_month() {
    let html = fixture("oshi_article.html");
    let state = extract_initial_state(&html).unwrap();
    let text = entry_text_for(&state, "12963931773").unwrap();
    let date = detect_oshi(&text).unwrap();
    assert_eq!(date, NaiveDate::from_ymd_opt(2026, 4, 1).unwrap());
}

#[test]
fn detect_oshi_returns_none_when_marker_absent() {
    let html = fixture("non_oshi_article.html");
    let state = extract_initial_state(&html).unwrap();
    // entry_id を非推し記事のものに変える
    // grep で確認: tests/fixtures/rokinon/non_oshi_article.html の entry ID
    let entry_id = std::env::var("NON_OSHI_ENTRY_ID").unwrap_or_else(|_| "12963909942".into());
    let text = entry_text_for(&state, &entry_id).unwrap();
    assert!(detect_oshi(&text).is_none());
}
```

- [ ] **Step 2: テスト実行**

```bash
cargo test --features ssr server::adapter::rokinon::tests::detect_oshi
```

Expected: 2 tests PASS。FAIL する場合：non_oshi_article のフィクスチャに「推し」が偶然含まれとるか、entry_id が違う可能性。

```bash
grep -oE '"entry_id":[0-9]+' tests/fixtures/rokinon/non_oshi_article.html | head -1
```

で確認して assertion 修正。

- [ ] **Step 3: コミット**

```bash
git add src/server/adapter/rokinon.rs
git commit -m "推しマーカーから featured_at を抽出するロジック"
```

### Task 12: 記事タイトル / アルバム名 / YouTube URL 抽出

**Files:**
- Modify: `src/server/adapter/rokinon.rs`

- [ ] **Step 1: 失敗するテストを書く**

```rust
use scraper::{Html, Selector};

#[derive(Debug, Clone, PartialEq)]
pub struct ExtractedItem {
    pub entry_id: String,
    pub source_url: String,
    pub featured_at: NaiveDate,
    pub artist_name: String,
    pub album_name: Option<String>,
    pub youtube_url: Option<String>,
}

/// JSON state からエントリのタイトルを取得（例: "Angelo De Augustine の新作"）
pub fn entry_title(state: &Value, entry_id: &str) -> AppResult<String> {
    let path = format!("/entryState/entryMap/{}/entry_title", entry_id);
    state
        .pointer(&path)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| AppError::Parse(format!("entry_title not found at {path}")))
}

/// "{Artist Name} の新作" 形式からアーティスト名を抽出。
/// 末尾サフィックスが他のパターン（"のニューシングル" 等）でも一応取れるよう、
/// 「の新作」「のニュー」「の新譜」「の新EP」等で分割。
pub fn extract_artist_name(title: &str) -> String {
    for suffix in ["の新作", "のニューアルバム", "の新譜", "のニューシングル", "の新EP"] {
        if let Some(idx) = title.rfind(suffix) {
            return title[..idx].trim().to_string();
        }
    }
    title.trim().to_string()
}

/// entry_text の HTML から最初の <h2> テキストをアルバム名として取り出す。
pub fn extract_album_from_html(entry_html: &str) -> Option<String> {
    let frag = Html::parse_fragment(entry_html);
    let sel = Selector::parse("h2").ok()?;
    frag.select(&sel)
        .next()
        .map(|el| el.text().collect::<String>().trim().to_string())
        .filter(|s| !s.is_empty())
}

/// entry_text の HTML から最初の YouTube リンクを取り出す。
pub fn extract_youtube_url(entry_html: &str) -> Option<String> {
    let frag = Html::parse_fragment(entry_html);
    let sel = Selector::parse("a[href]").ok()?;
    frag.select(&sel)
        .filter_map(|el| el.value().attr("href"))
        .find(|href| href.contains("youtube.com") || href.contains("youtu.be"))
        .map(|s| s.to_string())
}
```

テスト追加：

```rust
#[test]
fn extract_artist_name_strips_no_shinsaku_suffix() {
    assert_eq!(extract_artist_name("Angelo De Augustine の新作"), "Angelo De Augustine");
    assert_eq!(extract_artist_name("Blu & Exile の新作"), "Blu & Exile");
}

#[test]
fn extract_artist_name_returns_full_title_when_no_suffix() {
    assert_eq!(extract_artist_name("Some Title"), "Some Title");
}

#[test]
fn extract_album_from_html_returns_first_h2() {
    let html = r#"<p>intro</p><h2>Angel in Plainclothes</h2><p>...</p>"#;
    assert_eq!(extract_album_from_html(html).unwrap(), "Angel in Plainclothes");
}

#[test]
fn extract_youtube_url_finds_link() {
    let html = r#"<a href="https://www.youtube.com/watch?v=abc">listen</a>"#;
    assert_eq!(
        extract_youtube_url(html).unwrap(),
        "https://www.youtube.com/watch?v=abc"
    );
}

#[test]
fn end_to_end_extraction_from_fixture() {
    let html = fixture("oshi_article.html");
    let state = extract_initial_state(&html).unwrap();
    let entry_id = "12963931773";
    let entry_text = entry_text_for(&state, entry_id).unwrap();
    let title = entry_title(&state, entry_id).unwrap();
    let artist = extract_artist_name(&title);
    let album = extract_album_from_html(&entry_text);
    let date = detect_oshi(&entry_text);
    assert_eq!(artist, "Angelo De Augustine");
    assert!(album.is_some());
    assert_eq!(date, Some(NaiveDate::from_ymd_opt(2026, 4, 1).unwrap()));
}
```

- [ ] **Step 2: テスト実行**

```bash
cargo test --features ssr server::adapter::rokinon
```

Expected: 全テスト PASS。end_to_end の album が「Angel in Plainclothes」として取れることが確認できる（実フィクスチャ依存）。

- [ ] **Step 3: コミット**

```bash
git add src/server/adapter/rokinon.rs
git commit -m "アーティスト名・アルバム名・YouTube URL の抽出"
```

### Task 13: `MediaSource` trait と Rokinon 実装統合

**Files:**
- Create: `src/server/adapter/source.rs`
- Modify: `src/server/adapter/mod.rs`
- Modify: `src/server/adapter/rokinon.rs`

- [ ] **Step 1: trait 定義**

```rust
// src/server/adapter/source.rs
use crate::domain::recommendation::NewRecommendation;
use crate::server::error::AppResult;
use async_trait::async_trait;

#[derive(Debug, Clone)]
pub struct CandidateRef {
    pub source_external_id: String,
    pub source_url: String,
}

#[async_trait]
pub trait MediaSource: Send + Sync {
    fn id(&self) -> &'static str;

    /// 一覧ページから候補記事 URL を列挙。
    async fn list_candidates(&self) -> AppResult<Vec<CandidateRef>>;

    /// 単記事を取得して、推しならば NewRecommendation の素材を返す。
    /// Spotify 解決前の段階（spotify_url 等は None）。
    async fn fetch_and_extract(&self, candidate: &CandidateRef) -> AppResult<Option<NewRecommendation>>;
}
```

- [ ] **Step 2: モジュール公開**

```rust
// src/server/adapter/mod.rs
pub mod rokinon;
pub mod source;
```

- [ ] **Step 3: Rokinon 実装に MediaSource 適合**

`src/server/adapter/rokinon.rs` の末尾に：

```rust
use crate::domain::recommendation::NewRecommendation;
use crate::server::adapter::source::{CandidateRef, MediaSource};
use async_trait::async_trait;
use reqwest::Client;

const ROKINON_BASE: &str = "https://ameblo.jp/stamedba";
const USER_AGENT: &str = "i-am-rockin-on bot/1.0 (+https://github.com/dlwr/i-am-rockin-on)";

pub struct RokinonAdapter {
    client: Client,
    base_url: String,
}

impl RokinonAdapter {
    pub fn new() -> Self {
        let client = Client::builder()
            .user_agent(USER_AGENT)
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("reqwest client builds");
        Self { client, base_url: ROKINON_BASE.to_string() }
    }

    pub fn with_base_url(base_url: impl Into<String>) -> Self {
        let mut me = Self::new();
        me.base_url = base_url.into();
        me
    }

    fn entry_id_from_url(url: &str) -> Option<String> {
        Regex::new(r"entry-(\d+)\.html").ok()?
            .captures(url)?
            .get(1)
            .map(|m| m.as_str().to_string())
    }
}

#[async_trait]
impl MediaSource for RokinonAdapter {
    fn id(&self) -> &'static str { "rokinon" }

    async fn list_candidates(&self) -> AppResult<Vec<CandidateRef>> {
        let url = format!("{}/entrylist.html", self.base_url);
        let html = self.client.get(&url).send().await?.text().await?;
        let re = Regex::new(r"/stamedba/entry-(\d+)\.html").unwrap();
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for cap in re.captures_iter(&html) {
            let id = cap[1].to_string();
            if seen.insert(id.clone()) {
                out.push(CandidateRef {
                    source_external_id: id.clone(),
                    source_url: format!("{}/entry-{}.html", self.base_url, id),
                });
            }
        }
        Ok(out)
    }

    async fn fetch_and_extract(&self, candidate: &CandidateRef) -> AppResult<Option<NewRecommendation>> {
        let html = self.client.get(&candidate.source_url).send().await?.text().await?;
        let state = extract_initial_state(&html)?;
        let entry_text = match entry_text_for(&state, &candidate.source_external_id) {
            Ok(t) => t,
            Err(_) => return Ok(None),
        };
        let featured_at = match detect_oshi(&entry_text) {
            Some(d) => d,
            None => return Ok(None),
        };
        let title = entry_title(&state, &candidate.source_external_id)?;
        let artist = extract_artist_name(&title);
        let album = extract_album_from_html(&entry_text);
        let youtube = extract_youtube_url(&entry_text);

        Ok(Some(NewRecommendation {
            source_id: "rokinon".into(),
            source_url: candidate.source_url.clone(),
            source_external_id: candidate.source_external_id.clone(),
            featured_at,
            artist_name: artist,
            album_name: album,
            track_name: None,
            spotify_url: None,
            spotify_image_url: None,
            youtube_url: youtube,
        }))
    }
}
```

- [ ] **Step 4: list_candidates のオフラインテスト追加**

`src/server/adapter/rokinon.rs` のテストモジュールに：

```rust
#[tokio::test]
async fn list_candidates_parses_entrylist_fixture() {
    use wiremock::{matchers::path, Mock, MockServer, ResponseTemplate};
    let server = MockServer::start().await;
    let body = std::fs::read_to_string("tests/fixtures/rokinon/entrylist_page1.html").unwrap();
    Mock::given(path("/entrylist.html"))
        .respond_with(ResponseTemplate::new(200).set_body_string(body))
        .mount(&server)
        .await;

    let adapter = RokinonAdapter::with_base_url(server.uri());
    let cands = adapter.list_candidates().await.unwrap();
    assert!(cands.len() > 5, "should find multiple candidates, got {}", cands.len());
    assert!(cands.iter().all(|c| c.source_url.contains("/entry-")));
}
```

- [ ] **Step 5: テスト実行**

```bash
cargo test --features ssr server::adapter::rokinon
```

Expected: 全 PASS。

- [ ] **Step 6: コミット**

```bash
git add src/server/adapter/
git commit -m "MediaSource trait と RokinonAdapter の統合実装"
```

---

## Phase 4: Spotify Resolver

### Task 14: Spotify アクセストークン取得（Client Credentials）

**Files:**
- Create: `src/server/resolver/mod.rs`
- Create: `src/server/resolver/spotify.rs`
- Modify: `src/server/mod.rs`

- [ ] **Step 1: 失敗するテストを書く**

```rust
// src/server/resolver/spotify.rs
use crate::server::error::{AppError, AppResult};
use reqwest::Client;
use serde::Deserialize;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

const TOKEN_URL: &str = "https://accounts.spotify.com/api/token";
const SEARCH_URL: &str = "https://api.spotify.com/v1/search";

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
}

pub struct SpotifyResolver {
    client_id: String,
    client_secret: String,
    http: Client,
    token_url: String,
    search_url: String,
    token: Mutex<Option<(String, Instant)>>,
}

impl SpotifyResolver {
    pub fn new(client_id: String, client_secret: String) -> Self {
        Self {
            client_id, client_secret,
            http: Client::builder()
                .timeout(Duration::from_secs(20))
                .build().unwrap(),
            token_url: TOKEN_URL.into(),
            search_url: SEARCH_URL.into(),
            token: Mutex::new(None),
        }
    }

    pub fn with_endpoints(mut self, token_url: String, search_url: String) -> Self {
        self.token_url = token_url;
        self.search_url = search_url;
        self
    }

    async fn access_token(&self) -> AppResult<String> {
        let mut guard = self.token.lock().await;
        if let Some((tok, exp)) = guard.as_ref() {
            if Instant::now() < *exp - Duration::from_secs(30) {
                return Ok(tok.clone());
            }
        }
        let resp = self.http
            .post(&self.token_url)
            .basic_auth(&self.client_id, Some(&self.client_secret))
            .form(&[("grant_type", "client_credentials")])
            .send().await?
            .error_for_status()?
            .json::<TokenResponse>().await?;
        let exp = Instant::now() + Duration::from_secs(resp.expires_in);
        *guard = Some((resp.access_token.clone(), exp));
        Ok(resp.access_token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{matchers::{method, path}, Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn access_token_caches_value() {
        let server = MockServer::start().await;
        Mock::given(method("POST")).and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "tok-1", "token_type": "Bearer", "expires_in": 3600
            })))
            .expect(1)  // 1回しか叩かれんことを期待
            .mount(&server).await;

        let r = SpotifyResolver::new("id".into(), "secret".into())
            .with_endpoints(format!("{}/token", server.uri()), format!("{}/search", server.uri()));
        let t1 = r.access_token().await.unwrap();
        let t2 = r.access_token().await.unwrap();
        assert_eq!(t1, "tok-1");
        assert_eq!(t2, "tok-1");
    }
}
```

- [ ] **Step 2: モジュール公開**

```rust
// src/server/resolver/mod.rs
pub mod spotify;
```

```rust
// src/server/mod.rs に追加
pub mod resolver;
```

- [ ] **Step 3: テスト実行**

```bash
cargo test --features ssr server::resolver::spotify::tests::access_token_caches_value
```

Expected: PASS。

- [ ] **Step 4: コミット**

```bash
git add src/server/resolver/ src/server/mod.rs
git commit -m "Spotify Client Credentials トークン取得とキャッシュ"
```

### Task 15: Spotify アルバム検索とフォールバック

**Files:**
- Modify: `src/server/resolver/spotify.rs`

- [ ] **Step 1: 失敗するテストを書く**

`src/server/resolver/spotify.rs` の末尾に：

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct SpotifyMatch {
    pub url: String,
    pub image_url: Option<String>,
    pub track_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AlbumsResp { albums: AlbumPage }
#[derive(Debug, Deserialize)]
struct AlbumPage { items: Vec<AlbumItem> }
#[derive(Debug, Deserialize)]
struct AlbumItem {
    external_urls: ExternalUrls,
    images: Vec<Image>,
}
#[derive(Debug, Deserialize)]
struct ExternalUrls { spotify: String }
#[derive(Debug, Deserialize)]
struct Image { url: String }

#[derive(Debug, Deserialize)]
struct TracksResp { tracks: TrackPage }
#[derive(Debug, Deserialize)]
struct TrackPage { items: Vec<TrackItem> }
#[derive(Debug, Deserialize)]
struct TrackItem {
    name: String,
    external_urls: ExternalUrls,
    album: TrackAlbum,
}
#[derive(Debug, Deserialize)]
struct TrackAlbum { images: Vec<Image> }

impl SpotifyResolver {
    pub async fn resolve(&self, artist: &str, album: Option<&str>) -> AppResult<Option<SpotifyMatch>> {
        let token = self.access_token().await?;
        if let Some(album) = album {
            let q = format!("artist:\"{}\" album:\"{}\"", artist, album);
            let resp: AlbumsResp = self.http
                .get(&self.search_url)
                .bearer_auth(&token)
                .query(&[("q", q.as_str()), ("type", "album"), ("limit", "1")])
                .send().await?
                .error_for_status()?
                .json().await?;
            if let Some(first) = resp.albums.items.into_iter().next() {
                return Ok(Some(SpotifyMatch {
                    url: first.external_urls.spotify,
                    image_url: first.images.into_iter().next().map(|i| i.url),
                    track_name: None,
                }));
            }
        }
        // フォールバック: track 検索
        let q = format!("artist:\"{}\"{}", artist,
            album.map(|a| format!(" {}", a)).unwrap_or_default());
        let resp: TracksResp = self.http
            .get(&self.search_url)
            .bearer_auth(&token)
            .query(&[("q", q.as_str()), ("type", "track"), ("limit", "1")])
            .send().await?
            .error_for_status()?
            .json().await?;
        if let Some(t) = resp.tracks.items.into_iter().next() {
            return Ok(Some(SpotifyMatch {
                url: t.external_urls.spotify,
                image_url: t.album.images.into_iter().next().map(|i| i.url),
                track_name: Some(t.name),
            }));
        }
        Ok(None)
    }
}
```

テスト追加：

```rust
#[tokio::test]
async fn resolve_returns_album_when_search_hits() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "tok", "token_type": "Bearer", "expires_in": 3600
        }))).mount(&server).await;
    Mock::given(method("GET")).and(path("/search"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "albums": {
                "items": [{
                    "external_urls": { "spotify": "https://open.spotify.com/album/abc" },
                    "images": [{ "url": "https://i.scdn.co/image/abc.jpg" }]
                }]
            }
        }))).mount(&server).await;

    let r = SpotifyResolver::new("id".into(), "sec".into())
        .with_endpoints(format!("{}/token", server.uri()), format!("{}/search", server.uri()));
    let m = r.resolve("Angelo De Augustine", Some("Angel in Plainclothes")).await.unwrap().unwrap();
    assert_eq!(m.url, "https://open.spotify.com/album/abc");
    assert_eq!(m.image_url.unwrap(), "https://i.scdn.co/image/abc.jpg");
}

#[tokio::test]
async fn resolve_falls_back_to_track_when_album_empty() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "tok", "token_type": "Bearer", "expires_in": 3600
        }))).mount(&server).await;

    use wiremock::matchers::query_param;
    Mock::given(method("GET")).and(path("/search")).and(query_param("type", "album"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "albums": { "items": [] }
        }))).mount(&server).await;
    Mock::given(method("GET")).and(path("/search")).and(query_param("type", "track"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "tracks": { "items": [{
                "name": "Some Track",
                "external_urls": { "spotify": "https://open.spotify.com/track/xyz" },
                "album": { "images": [{ "url": "https://i.scdn.co/image/xyz.jpg" }] }
            }]}
        }))).mount(&server).await;

    let r = SpotifyResolver::new("id".into(), "sec".into())
        .with_endpoints(format!("{}/token", server.uri()), format!("{}/search", server.uri()));
    let m = r.resolve("Foo", Some("Bar")).await.unwrap().unwrap();
    assert_eq!(m.url, "https://open.spotify.com/track/xyz");
    assert_eq!(m.track_name.unwrap(), "Some Track");
}

#[tokio::test]
async fn resolve_returns_none_when_nothing_found() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "tok", "token_type": "Bearer", "expires_in": 3600
        }))).mount(&server).await;
    Mock::given(method("GET")).and(path("/search"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "albums": { "items": [] },
            "tracks": { "items": [] }
        }))).mount(&server).await;

    let r = SpotifyResolver::new("id".into(), "sec".into())
        .with_endpoints(format!("{}/token", server.uri()), format!("{}/search", server.uri()));
    let m = r.resolve("Nope", Some("Nope")).await.unwrap();
    assert!(m.is_none());
}
```

- [ ] **Step 2: テスト実行**

```bash
cargo test --features ssr server::resolver::spotify
```

Expected: 4 tests PASS。

- [ ] **Step 3: コミット**

```bash
git add src/server/resolver/spotify.rs
git commit -m "Spotify アルバム検索＋トラック検索フォールバック"
```

---

## Phase 5: オーケストレーション

### Task 16: スクレイプ実行統合（Adapter + Resolver + Repo）

**Files:**
- Create: `src/server/scrape.rs`
- Modify: `src/server/mod.rs`

- [ ] **Step 1: 失敗するテストを書く**

```rust
// src/server/scrape.rs
use crate::server::adapter::source::MediaSource;
use crate::server::error::AppResult;
use crate::server::resolver::spotify::SpotifyResolver;
use crate::server::scrape_log::ScrapeLog;
use crate::server::store::RecommendationRepo;
use std::sync::Arc;

pub struct ScrapePipeline {
    pub source: Arc<dyn MediaSource>,
    pub resolver: Arc<SpotifyResolver>,
    pub repo: Arc<RecommendationRepo>,
    pub log: Arc<ScrapeLog>,
}

#[derive(Debug, Default)]
pub struct ScrapeOutcome {
    pub items_added: i64,
    pub items_updated: i64,
    pub items_skipped: i64,
}

impl ScrapePipeline {
    pub async fn run(&self) -> AppResult<ScrapeOutcome> {
        let handle = self.log.start(self.source.id()).await?;
        let result = self.run_inner().await;
        match &result {
            Ok(o) => self.log.finish_success(&handle, o.items_added, o.items_updated).await?,
            Err(e) => self.log.finish_error(&handle, &e.to_string()).await?,
        }
        result
    }

    async fn run_inner(&self) -> AppResult<ScrapeOutcome> {
        let candidates = self.source.list_candidates().await?;
        let mut outcome = ScrapeOutcome::default();
        for c in candidates {
            let extracted = match self.source.fetch_and_extract(&c).await? {
                Some(item) => item,
                None => { outcome.items_skipped += 1; continue; }
            };
            let mut new_rec = extracted;
            // Spotify 解決
            match self.resolver.resolve(&new_rec.artist_name, new_rec.album_name.as_deref()).await {
                Ok(Some(m)) => {
                    new_rec.spotify_url = Some(m.url);
                    new_rec.spotify_image_url = m.image_url;
                    if new_rec.track_name.is_none() {
                        new_rec.track_name = m.track_name;
                    }
                }
                Ok(None) => tracing::info!(artist = %new_rec.artist_name, "spotify match not found"),
                Err(e) => tracing::warn!(error = %e, "spotify resolve failed; saving without"),
            }
            let (_, inserted) = self.repo.upsert(new_rec).await?;
            if inserted { outcome.items_added += 1; } else { outcome.items_updated += 1; }
            // 礼儀正しいレート制限
            tokio::time::sleep(std::time::Duration::from_millis(800)).await;
        }
        Ok(outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::recommendation::NewRecommendation;
    use crate::server::adapter::source::CandidateRef;
    use async_trait::async_trait;
    use chrono::NaiveDate;
    use sqlx::sqlite::SqlitePoolOptions;

    struct FakeSource { items: Vec<NewRecommendation> }

    #[async_trait]
    impl MediaSource for FakeSource {
        fn id(&self) -> &'static str { "fake" }
        async fn list_candidates(&self) -> AppResult<Vec<CandidateRef>> {
            Ok(self.items.iter().map(|i| CandidateRef {
                source_external_id: i.source_external_id.clone(),
                source_url: i.source_url.clone(),
            }).collect())
        }
        async fn fetch_and_extract(&self, c: &CandidateRef) -> AppResult<Option<NewRecommendation>> {
            Ok(self.items.iter().find(|i| i.source_external_id == c.source_external_id).cloned())
        }
    }

    #[tokio::test]
    async fn pipeline_records_added_count() {
        let pool = SqlitePoolOptions::new().connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();

        // wiremock で空の Spotify レスポンスを返す
        use wiremock::{matchers::{method, path}, Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        Mock::given(method("POST")).and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "tok", "token_type": "Bearer", "expires_in": 3600
            }))).mount(&server).await;
        Mock::given(method("GET")).and(path("/search"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "albums": { "items": [] },
                "tracks": { "items": [] }
            }))).mount(&server).await;

        let resolver = SpotifyResolver::new("id".into(), "sec".into())
            .with_endpoints(format!("{}/token", server.uri()), format!("{}/search", server.uri()));

        let item = NewRecommendation {
            source_id: "fake".into(),
            source_url: "https://example.com/1".into(),
            source_external_id: "1".into(),
            featured_at: NaiveDate::from_ymd_opt(2026, 4, 1).unwrap(),
            artist_name: "Foo".into(), album_name: None, track_name: None,
            spotify_url: None, spotify_image_url: None, youtube_url: None,
        };
        let pipeline = ScrapePipeline {
            source: Arc::new(FakeSource { items: vec![item] }),
            resolver: Arc::new(resolver),
            repo: Arc::new(RecommendationRepo::new(pool.clone())),
            log: Arc::new(ScrapeLog::new(pool)),
        };
        // sleep を短縮するため tokio::time::pause/resume は使わず、テストは ~800ms 待つ
        let outcome = pipeline.run().await.unwrap();
        assert_eq!(outcome.items_added, 1);
        assert_eq!(outcome.items_updated, 0);
    }
}
```

- [ ] **Step 2: モジュール公開**

```rust
// src/server/mod.rs に追加
pub mod scrape;
```

- [ ] **Step 3: テスト実行**

```bash
cargo test --features ssr server::scrape
```

Expected: 1 test PASS（〜1秒かかる）。

- [ ] **Step 4: コミット**

```bash
git add src/server/scrape.rs src/server/mod.rs
git commit -m "ScrapePipeline でアダプタ・リゾルバ・リポジトリを統合"
```

### Task 17: 設定読み込み（環境変数）

**Files:**
- Create: `src/server/config.rs`
- Modify: `src/server/mod.rs`

- [ ] **Step 1: 失敗するテストを書く**

```rust
// src/server/config.rs
use crate::server::error::{AppError, AppResult};

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub spotify_client_id: String,
    pub spotify_client_secret: String,
}

impl Config {
    pub fn from_env() -> AppResult<Self> {
        Ok(Self {
            database_url: std::env::var("DATABASE_URL")
                .map_err(|_| AppError::Config("DATABASE_URL required".into()))?,
            spotify_client_id: std::env::var("SPOTIFY_CLIENT_ID")
                .map_err(|_| AppError::Config("SPOTIFY_CLIENT_ID required".into()))?,
            spotify_client_secret: std::env::var("SPOTIFY_CLIENT_SECRET")
                .map_err(|_| AppError::Config("SPOTIFY_CLIENT_SECRET required".into()))?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_database_url_errors() {
        // 注意: 環境変数を弄るテストはシリアル実行が必要だが、ここは並列でも安全な書き方にする
        let saved = std::env::var("DATABASE_URL").ok();
        std::env::remove_var("DATABASE_URL");
        let result = Config::from_env();
        if let Some(v) = saved { std::env::set_var("DATABASE_URL", v); }
        let err = result.unwrap_err();
        assert!(err.to_string().contains("DATABASE_URL"));
    }
}
```

注意: `std::env::set_var` / `remove_var` は Rust 2024 では unsafe 化される可能性あり。テストが unstable 警告出す場合は `#[serial_test::serial]` か、別途の env 抽象化を検討。最初は wrapping せず動かしてOK。

- [ ] **Step 2: モジュール公開**

```rust
// src/server/mod.rs
pub mod adapter;
pub mod config;
pub mod error;
pub mod resolver;
pub mod scrape;
pub mod scrape_log;
pub mod store;
```

- [ ] **Step 3: テスト実行**

```bash
cargo test --features ssr server::config
```

Expected: PASS。

- [ ] **Step 4: コミット**

```bash
git add src/server/config.rs src/server/mod.rs
git commit -m "環境変数からの Config 読み込み"
```

### Task 18: スクレイプ CLI バイナリ

**Files:**
- Modify: `src/bin/scrape.rs`

- [ ] **Step 1: CLI 実装**

```rust
// src/bin/scrape.rs
#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use clap::Parser;
    use i_am_rockin_on::server::adapter::rokinon::RokinonAdapter;
    use i_am_rockin_on::server::adapter::source::MediaSource;
    use i_am_rockin_on::server::config::Config;
    use i_am_rockin_on::server::resolver::spotify::SpotifyResolver;
    use i_am_rockin_on::server::scrape::ScrapePipeline;
    use i_am_rockin_on::server::scrape_log::ScrapeLog;
    use i_am_rockin_on::server::store::RecommendationRepo;
    use std::sync::Arc;

    #[derive(Parser)]
    #[command(name = "scrape")]
    struct Cli {
        #[arg(long, default_value = "rokinon")]
        source: String,
    }

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let _ = dotenvy::dotenv();
    let cli = Cli::parse();
    let cfg = Config::from_env()?;

    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(4)
        .connect(&cfg.database_url).await?;
    sqlx::migrate!().run(&pool).await?;

    let source: Arc<dyn MediaSource> = match cli.source.as_str() {
        "rokinon" => Arc::new(RokinonAdapter::new()),
        other => anyhow::bail!("unknown source: {other}"),
    };
    let resolver = Arc::new(SpotifyResolver::new(cfg.spotify_client_id, cfg.spotify_client_secret));
    let repo = Arc::new(RecommendationRepo::new(pool.clone()));
    let log = Arc::new(ScrapeLog::new(pool));

    let pipeline = ScrapePipeline { source, resolver, repo, log };
    let outcome = pipeline.run().await?;
    println!("added: {}, updated: {}, skipped: {}",
        outcome.items_added, outcome.items_updated, outcome.items_skipped);
    Ok(())
}

#[cfg(not(feature = "ssr"))]
fn main() {}
```

- [ ] **Step 2: dotenvy 依存追加**

`Cargo.toml` の `[dependencies]` に：

```toml
dotenvy = { version = "0.15", optional = true }
```

そして `[features] ssr` の配列に `"dep:dotenvy",` を追加。

- [ ] **Step 3: ビルド確認**

```bash
cargo build --features ssr --bin scrape
```

Expected: ビルド成功。

- [ ] **Step 4: コミット**

```bash
git add Cargo.toml src/bin/scrape.rs
git commit -m "scrape CLI バイナリで手動スクレイプを可能に"
```

### Task 19: tokio cron スケジューラ統合

**Files:**
- Create: `src/server/scheduler.rs`
- Modify: `src/server/mod.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: スケジューラ実装**

```rust
// src/server/scheduler.rs
use crate::server::error::AppResult;
use crate::server::scrape::ScrapePipeline;
use std::sync::Arc;
use tokio_cron_scheduler::{Job, JobScheduler};

pub async fn install_daily_scrape(pipeline: Arc<ScrapePipeline>) -> AppResult<JobScheduler> {
    let sched = JobScheduler::new().await
        .map_err(|e| crate::server::error::AppError::Config(e.to_string()))?;
    let p = pipeline.clone();
    let job = Job::new_async("0 0 19 * * *", move |_uuid, _l| {
        // UTC 19:00 = JST 04:00
        let p = p.clone();
        Box::pin(async move {
            match p.run().await {
                Ok(o) => tracing::info!(added = o.items_added, updated = o.items_updated, "scrape ok"),
                Err(e) => tracing::error!(error = %e, "scrape failed"),
            }
        })
    }).map_err(|e| crate::server::error::AppError::Config(e.to_string()))?;
    sched.add(job).await
        .map_err(|e| crate::server::error::AppError::Config(e.to_string()))?;
    sched.start().await
        .map_err(|e| crate::server::error::AppError::Config(e.to_string()))?;
    Ok(sched)
}

pub async fn run_initial_scrape_if_empty(
    pipeline: Arc<ScrapePipeline>,
    log: Arc<crate::server::scrape_log::ScrapeLog>,
    source_id: &str,
) -> AppResult<()> {
    if log.count(source_id).await? == 0 {
        tracing::info!(%source_id, "no prior runs; performing initial scrape");
        let _ = pipeline.run().await;
    }
    Ok(())
}
```

- [ ] **Step 2: モジュール公開**

```rust
// src/server/mod.rs に追加
pub mod scheduler;
```

- [ ] **Step 3: `src/main.rs` を更新して、起動時に DB プール作成・マイグレーション・スケジューラ起動**

```rust
#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use axum::Router;
    use i_am_rockin_on::App;
    use i_am_rockin_on::server::adapter::rokinon::RokinonAdapter;
    use i_am_rockin_on::server::adapter::source::MediaSource;
    use i_am_rockin_on::server::config::Config;
    use i_am_rockin_on::server::resolver::spotify::SpotifyResolver;
    use i_am_rockin_on::server::scheduler::{install_daily_scrape, run_initial_scrape_if_empty};
    use i_am_rockin_on::server::scrape::ScrapePipeline;
    use i_am_rockin_on::server::scrape_log::ScrapeLog;
    use i_am_rockin_on::server::store::RecommendationRepo;
    use leptos::prelude::*;
    use leptos_axum::{generate_route_list, LeptosRoutes};
    use std::sync::Arc;

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let _ = dotenvy::dotenv();

    let cfg = Config::from_env()?;
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(8)
        .connect(&cfg.database_url).await?;
    sqlx::migrate!().run(&pool).await?;

    let source: Arc<dyn MediaSource> = Arc::new(RokinonAdapter::new());
    let resolver = Arc::new(SpotifyResolver::new(cfg.spotify_client_id, cfg.spotify_client_secret));
    let repo = Arc::new(RecommendationRepo::new(pool.clone()));
    let log = Arc::new(ScrapeLog::new(pool.clone()));
    let pipeline = Arc::new(ScrapePipeline {
        source: source.clone(), resolver, repo: repo.clone(), log: log.clone(),
    });

    // 初回起動時のみフルスクレイプ
    let init_pipe = pipeline.clone();
    let init_log = log.clone();
    tokio::spawn(async move {
        if let Err(e) = run_initial_scrape_if_empty(init_pipe, init_log, "rokinon").await {
            tracing::error!(error = %e, "initial scrape failed");
        }
    });

    // 日次 cron
    let _sched = install_daily_scrape(pipeline.clone()).await?;

    let conf = get_configuration(None).unwrap();
    let leptos_options = conf.leptos_options;
    let addr = leptos_options.site_addr;
    let routes = generate_route_list(App);

    let app = Router::new()
        .leptos_routes_with_context(
            &leptos_options,
            routes,
            {
                let repo = repo.clone();
                move || provide_context(repo.clone())
            },
            {
                let opts = leptos_options.clone();
                move || leptos::prelude::shell(opts.clone())
            },
        )
        .fallback(leptos_axum::file_and_error_handler(leptos::prelude::shell))
        .with_state(leptos_options);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("listening on http://{}", &addr);
    axum::serve(listener, app.into_make_service()).await?;
    Ok(())
}

#[cfg(not(feature = "ssr"))]
pub fn main() {}
```

- [ ] **Step 4: ビルド確認**

```bash
cargo leptos build
```

Expected: ビルド成功（warnings あっても error 無し）。

- [ ] **Step 5: コミット**

```bash
git add src/main.rs src/server/scheduler.rs src/server/mod.rs
git commit -m "tokio cron スケジューラと初回スクレイプ起動を統合"
```

---

## Phase 6: Web UI

### Task 20: 一覧ページの Server Function

**Files:**
- Modify: `src/pages/home.rs`

- [ ] **Step 1: 一覧表示用のローダーを書く**

```rust
// src/pages/home.rs
use leptos::prelude::*;

#[server(ListRecommendations, "/api")]
pub async fn list_recommendations() -> Result<Vec<RecommendationView>, ServerFnError> {
    use crate::server::store::RecommendationRepo;
    use std::sync::Arc;
    let repo = use_context::<Arc<RecommendationRepo>>()
        .ok_or_else(|| ServerFnError::ServerError("repo missing".into()))?;
    let rows = repo.list_recent(100).await
        .map_err(|e| ServerFnError::ServerError(e.to_string()))?;
    Ok(rows.into_iter().map(RecommendationView::from).collect())
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct RecommendationView {
    pub id: i64,
    pub source_id: String,
    pub source_url: String,
    pub featured_at: String,
    pub artist_name: String,
    pub album_name: Option<String>,
    pub spotify_url: Option<String>,
    pub spotify_image_url: Option<String>,
    pub youtube_url: Option<String>,
}

#[cfg(feature = "ssr")]
impl From<crate::domain::recommendation::Recommendation> for RecommendationView {
    fn from(r: crate::domain::recommendation::Recommendation) -> Self {
        Self {
            id: r.id,
            source_id: r.source_id,
            source_url: r.source_url,
            featured_at: r.featured_at.format("%Y-%m").to_string(),
            artist_name: r.artist_name,
            album_name: r.album_name,
            spotify_url: r.spotify_url,
            spotify_image_url: r.spotify_image_url,
            youtube_url: r.youtube_url,
        }
    }
}

#[component]
pub fn Home() -> impl IntoView {
    let recs = Resource::new(|| (), |_| async { list_recommendations().await });
    view! {
        <h1>"I am rockin on"</h1>
        <p class="lede">"音楽メディアの『推し』を集めたページずら"</p>
        <Suspense fallback=|| view! { <p>"loading..."</p> }>
            {move || recs.get().map(|r| match r {
                Ok(items) => view! { <RecommendationGrid items=items/> }.into_any(),
                Err(e) => view! { <p class="error">{format!("error: {e}")}</p> }.into_any(),
            })}
        </Suspense>
    }
}

#[component]
fn RecommendationGrid(items: Vec<RecommendationView>) -> impl IntoView {
    view! {
        <ul class="grid">
            {items.into_iter().map(|item| view! {
                <li class="card">
                    {item.spotify_image_url.as_ref().map(|src| view! {
                        <img src=src.clone() alt="" loading="lazy"/>
                    })}
                    <div class="meta">
                        <div class="artist">{item.artist_name.clone()}</div>
                        {item.album_name.clone().map(|a| view! { <div class="album">{a}</div> })}
                        <div class="featured">{item.featured_at.clone()}</div>
                    </div>
                    <div class="links">
                        {item.spotify_url.clone().map(|u| view! {
                            <a class="btn spotify" href=u target="_blank" rel="noopener">"Spotify"</a>
                        })}
                        {item.youtube_url.clone().map(|u| view! {
                            <a class="btn youtube" href=u target="_blank" rel="noopener">"YouTube"</a>
                        })}
                        <a class="btn source" href=item.source_url target="_blank" rel="noopener">"記事"</a>
                    </div>
                </li>
            }).collect_view()}
        </ul>
    }
}
```

- [ ] **Step 2: ビルド確認**

```bash
cargo leptos build
```

Expected: 成功。

- [ ] **Step 3: コミット**

```bash
git add src/pages/home.rs
git commit -m "一覧ページの ListRecommendations Server Function とグリッド表示"
```

### Task 21: スタイル整備

**Files:**
- Modify: `style/main.css`

- [ ] **Step 1: 最低限のグリッドスタイル**

```css
* { box-sizing: border-box; }
body {
    font-family: system-ui, -apple-system, "Hiragino Kaku Gothic ProN", sans-serif;
    margin: 0; padding: 1rem;
    background: #fafafa; color: #1a1a1a;
}
main { max-width: 1200px; margin: 0 auto; }
h1 { font-size: 2rem; margin-bottom: 0.25rem; }
.lede { color: #666; margin-top: 0; margin-bottom: 1.5rem; }
.error { color: #b00020; }

.grid {
    list-style: none; padding: 0; margin: 0;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
    gap: 1.25rem;
}
.card {
    background: #fff; border-radius: 8px; padding: 0.75rem;
    box-shadow: 0 1px 3px rgba(0,0,0,0.06);
    display: flex; flex-direction: column; gap: 0.5rem;
}
.card img {
    width: 100%; aspect-ratio: 1 / 1; object-fit: cover;
    border-radius: 4px; background: #eee;
}
.meta .artist { font-weight: 600; }
.meta .album { font-size: 0.9rem; color: #444; }
.meta .featured { font-size: 0.8rem; color: #888; margin-top: 0.25rem; }
.links { display: flex; gap: 0.5rem; flex-wrap: wrap; margin-top: auto; }
.btn {
    font-size: 0.8rem; padding: 0.25rem 0.6rem; border-radius: 999px;
    text-decoration: none; color: #fff;
}
.btn.spotify { background: #1db954; }
.btn.youtube { background: #ff0000; }
.btn.source { background: #555; }
```

- [ ] **Step 2: 開発サーバで目視確認**

DB に少なくとも1件入っとる必要あり。空ならまず CLI で投入：

```bash
DATABASE_URL=sqlite:data/app.db SPOTIFY_CLIENT_ID=xxx SPOTIFY_CLIENT_SECRET=yyy \
  cargo run --features ssr --bin scrape -- --source rokinon
```

Spotify creds が無い場合は手動で1行 insert：

```bash
sqlite3 data/app.db <<'SQL'
INSERT INTO recommendations
  (source_id, source_url, source_external_id, featured_at, artist_name, album_name, spotify_url, spotify_image_url, youtube_url)
VALUES
  ('rokinon', 'https://ameblo.jp/stamedba/entry-12963931773.html', '12963931773', '2026-04-01',
   'Angelo De Augustine', 'Angel in Plainclothes',
   'https://open.spotify.com/album/example',
   'https://i.scdn.co/image/example.jpg',
   'https://www.youtube.com/watch?v=example');
SQL
```

そして起動：

```bash
DATABASE_URL=sqlite:data/app.db SPOTIFY_CLIENT_ID=dummy SPOTIFY_CLIENT_SECRET=dummy \
  cargo leptos watch
```

`http://localhost:3000` でジャケット画像が出ることを目視確認。

- [ ] **Step 3: コミット**

```bash
git add style/main.css
git commit -m "一覧ページのグリッド／カードスタイルを追加"
```

---

## Phase 7: デプロイ

### Task 22: Dockerfile（multi-stage）

**Files:**
- Create: `Dockerfile`
- Create: `.dockerignore`

- [ ] **Step 1: `.dockerignore`**

```
target
.git
data
*.db
.env
```

- [ ] **Step 2: `Dockerfile`**

```dockerfile
# Build stage
FROM rust:1.83-slim AS builder
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev curl \
    && rm -rf /var/lib/apt/lists/*

RUN rustup default nightly && \
    rustup target add wasm32-unknown-unknown && \
    cargo install cargo-leptos --locked

WORKDIR /app
COPY . .
ENV SQLX_OFFLINE=true
RUN cargo leptos build --release

# Runtime stage
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 sqlite3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/i-am-rockin-on /app/
COPY --from=builder /app/target/site /app/site
COPY migrations /app/migrations

ENV LEPTOS_OUTPUT_NAME=i-am-rockin-on
ENV LEPTOS_SITE_ROOT=site
ENV LEPTOS_SITE_PKG_DIR=pkg
ENV LEPTOS_SITE_ADDR=0.0.0.0:3000
ENV LEPTOS_ENV=PROD
EXPOSE 3000
CMD ["/app/i-am-rockin-on"]
```

- [ ] **Step 3: SQLX_OFFLINE 用の sqlx-data を生成（初回のみ）**

```bash
DATABASE_URL=sqlite:data/app.db cargo sqlx prepare --workspace -- --features ssr
```

これで `.sqlx/` ディレクトリが生成される。

```bash
git add .sqlx/
git commit -m "sqlx offline metadata を追加"
```

- [ ] **Step 4: ローカルでビルド確認**

```bash
docker build -t rockin-on:dev .
```

Expected: ビルド成功（〜10分かかる、初回はコンパイルキャッシュ無いから）。

- [ ] **Step 5: コミット**

```bash
git add Dockerfile .dockerignore
git commit -m "multi-stage Dockerfile を追加"
```

### Task 23: fly.io 設定

**Files:**
- Create: `fly.toml`
- Create: `docs/deploy.md`

- [ ] **Step 1: `fly.toml`**

```toml
app = "i-am-rockin-on"
primary_region = "nrt"

[build]

[env]
DATABASE_URL = "sqlite:/data/app.db"
RUST_LOG = "info,i_am_rockin_on=debug"
TZ = "Asia/Tokyo"

[mounts]
source = "rockin_data"
destination = "/data"

[http_service]
internal_port = 3000
force_https = true
auto_stop_machines = "off"
auto_start_machines = true
min_machines_running = 1

[[vm]]
cpu_kind = "shared"
cpus = 1
memory_mb = 512
```

- [ ] **Step 2: `docs/deploy.md`**

```markdown
# デプロイ手順

## 初回セットアップ

1. `flyctl auth login`
2. `flyctl launch --no-deploy --copy-config --name i-am-rockin-on --region nrt`
3. ボリューム作成: `flyctl volumes create rockin_data --region nrt --size 1`
4. シークレット設定:
   ```
   flyctl secrets set SPOTIFY_CLIENT_ID=xxx SPOTIFY_CLIENT_SECRET=yyy
   ```
5. デプロイ: `flyctl deploy`

## 確認

- `flyctl logs` でログ確認
- 起動初回は `scrape_runs` が空ぃけぇ自動で1回スクレイプが走る
- 以降は JST 04:00 に日次

## DB 直接確認

```
flyctl ssh console
sqlite3 /data/app.db
.tables
SELECT count(*) FROM recommendations;
```

## 手動スクレイプ実行

```
flyctl ssh console -C "/app/scrape --source rokinon"
```
```

- [ ] **Step 3: コミット**

```bash
git add fly.toml docs/deploy.md
git commit -m "fly.io 設定とデプロイ手順を追加"
```

### Task 24: README とライセンス

**Files:**
- Create: `README.md`

- [ ] **Step 1: README**

```markdown
# i-am-rockin-on

音楽メディア（まずは「ロキノンには騙されないぞ」）の「推し」記事を集約し、Spotify ジャケットとリンクを並べる Web サイト。

## 開発

前提: Rust 1.83+, cargo-leptos, sqlx-cli

```bash
cargo install cargo-leptos --locked
cargo install sqlx-cli --no-default-features --features sqlite,rustls

# DB 準備
mkdir -p data
echo 'DATABASE_URL=sqlite:data/app.db' > .env
sqlx database create
sqlx migrate run

# Spotify creds（https://developer.spotify.com/ で取得）
echo 'SPOTIFY_CLIENT_ID=...' >> .env
echo 'SPOTIFY_CLIENT_SECRET=...' >> .env

# 開発サーバ
cargo leptos watch

# 手動スクレイプ
cargo run --features ssr --bin scrape -- --source rokinon
```

## デプロイ

`docs/deploy.md` を参照。

## 設計

`docs/superpowers/specs/2026-05-08-music-recommendations-aggregator-design.md`
```

- [ ] **Step 2: コミット**

```bash
git add README.md
git commit -m "README を追加"
```

---

## 実装後チェック

- [ ] **すべてのテスト実行**

```bash
DATABASE_URL=sqlite:data/app.db cargo test --features ssr
```

Expected: 全 PASS。

- [ ] **lint と format**

```bash
cargo fmt --check
cargo clippy --features ssr -- -D warnings
```

問題あれば修正してコミット。

- [ ] **デプロイ後の動作確認**

1. `https://i-am-rockin-on.fly.dev/` を開いて一覧表示される
2. `flyctl logs` でスクレイプログを確認
3. ジャケット画像が表示され、Spotify リンクで実際に Spotify が開く
