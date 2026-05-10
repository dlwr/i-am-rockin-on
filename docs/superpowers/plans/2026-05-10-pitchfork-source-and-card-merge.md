# Pitchfork ソース追加 ＋ カードマージ実装プラン

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Pitchfork のアルバムレビュー（スコア 8.0+ かつ直近 90 日）を新しい `MediaSource` として収集し、Rokinon と同一アルバムが重なった場合はホームグリッドのカードを 1 枚にマージして「記事」ボタンの hover で各メディアを選べるようにする。

**Architecture:** 既存の `MediaSource` trait に `PitchforkAdapter` を 1 impl 追加し、`ScrapePipeline` と `RecommendationRepo` をそのまま流用する。重複処理は表示時のみで完結（DB スキーマは不変）。`RecommendationRepo` に `list_recent_albums` を追加し、SQLite の `json_group_array` で 1 アルバム = 1 行に集約してから Rust 側で代表値を選ぶ。Home ビューは `AlbumCard` 駆動に書き換え、複数ソース時は `<details>` ベースのドロップダウンを描画する。

**Tech Stack:** Rust 2021 / Leptos 0.7 (SSR) / sqlx 0.8 (SQLite) / reqwest 0.12 / scraper 0.20 / regex 1 / serde_json 1 / wiremock 0.6 / tokio-cron-scheduler 0.13

**Spec:** `docs/superpowers/specs/2026-05-10-pitchfork-source-and-card-merge-design.md`

---

## ファイル構成

| Path | 役割 | 操作 |
|---|---|---|
| `src/domain/album_card.rs` | `AlbumCard` / `SourceLink` ドメイン型 | 新規 |
| `src/domain/mod.rs` | `pub mod album_card` 追加 | 修正 |
| `src/server/store.rs` | `RecommendationRepo::list_recent_albums` | 追加 |
| `src/server/adapter/pitchfork.rs` | `PitchforkAdapter` impl | 新規 |
| `src/server/adapter/mod.rs` | `pub mod pitchfork` 追加 | 修正 |
| `src/server/config.rs` | `pitchfork_*` フィールド追加 | 修正 |
| `src/server/scheduler.rs` | `install_scrape_job(pipeline, cron)` に汎用化 | 修正 |
| `src/pages/home.rs` | `list_recommendations` を `AlbumCard` 駆動に変更／hover ドロップダウン | 修正 |
| `src/main.rs` | Pitchfork パイプライン登録 | 修正 |
| `tests/fixtures/pitchfork/index.html` | レビュー一覧ページ（最小） | 新規 |
| `tests/fixtures/pitchfork/review_high.html` | スコア 9.0 のレビューページ（最小、最近の日付） | 新規 |
| `tests/fixtures/pitchfork/review_low.html` | スコア 7.5 のレビューページ（最小） | 新規 |
| `tests/fixtures/pitchfork/review_high_old.html` | スコア 9.0／古い日付のレビューページ（recency テスト用） | 新規 |

---

## 共通の前提

- すべてのコマンドはリポジトリルート `~/ghq/github.com/dlwr/i-am-rockin-on` で実行する。
- テスト実行は `mise run test`（= `cargo test --features ssr`）。`cargo leptos build` はビルド検証用。
- `cargo sqlx prepare --workspace` を新規 SQL クエリ追加後に走らせる必要がある。`.sqlx/` 配下のファイル更新を commit に含める。
- コミットメッセージは既存リポジトリのスタイルに合わせる（簡潔な日本語、prefix は `feat`/`fix`/`refactor`/`test`/`docs`/`chore` など）。
- 各タスク末尾の commit は **ひとつの意味単位ずつ**。複数を 1 commit にまとめない（CLAUDE.md ルール）。

---

## Task 1: `AlbumCard` / `SourceLink` ドメイン型を追加

**Files:**
- Create: `src/domain/album_card.rs`
- Modify: `src/domain/mod.rs`

これは pure data type のため fail → pass の TDD 反復は省略し、型定義 ＋ 構築スモークテストの 1 ステップで進める。

- [ ] **Step 1: モジュール宣言を追加**

`src/domain/mod.rs` を以下に書き換え：

```rust
pub mod album_card;
pub mod recommendation;
```

- [ ] **Step 2: 型定義と構築テストを書く**

`src/domain/album_card.rs` を新規作成：

```rust
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AlbumCard {
    pub artist_name: String,
    pub album_name: Option<String>,
    pub spotify_url: Option<String>,
    pub spotify_image_url: Option<String>,
    pub youtube_url: Option<String>,
    /// 最新の `featured_at`（同一アルバム内のソースで最も新しいもの）。
    pub featured_at: NaiveDate,
    /// `featured_at DESC, source_id ASC` で並んだソース一覧。
    pub sources: Vec<SourceLink>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceLink {
    pub source_id: String,
    pub source_url: String,
    pub featured_at: NaiveDate,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn album_card_can_hold_multiple_sources() {
        let card = AlbumCard {
            artist_name: "Aldous Harding".into(),
            album_name: Some("Train on the Island".into()),
            spotify_url: None,
            spotify_image_url: None,
            youtube_url: None,
            featured_at: NaiveDate::from_ymd_opt(2026, 5, 8).unwrap(),
            sources: vec![
                SourceLink {
                    source_id: "pitchfork".into(),
                    source_url: "https://pitchfork.com/reviews/albums/aldous-harding-train-on-the-island/".into(),
                    featured_at: NaiveDate::from_ymd_opt(2026, 5, 8).unwrap(),
                },
                SourceLink {
                    source_id: "rokinon".into(),
                    source_url: "https://ameblo.jp/stamedba/entry-1.html".into(),
                    featured_at: NaiveDate::from_ymd_opt(2026, 5, 1).unwrap(),
                },
            ],
        };
        assert_eq!(card.sources.len(), 2);
        assert_eq!(card.sources[0].source_id, "pitchfork");
    }
}
```

- [ ] **Step 3: テスト pass を確認**

```bash
mise run test -- album_card 2>&1 | tail -10
```

期待: `test domain::album_card::tests::album_card_can_hold_multiple_sources ... ok`

- [ ] **Step 4: Commit**

```bash
git add src/domain/album_card.rs src/domain/mod.rs
git commit -m "feat(domain): AlbumCard / SourceLink 型を追加"
```

---

## Task 2: `RecommendationRepo::list_recent_albums` を追加

**Files:**
- Modify: `src/server/store.rs`

dedup の SQL 戦略：
1. `recommendations` 全行に対し `dedup_key = COALESCE(spotify_url, lower(trim(artist_name))||'|'||lower(trim(coalesce(album_name,''))))` を計算
2. `dedup_key` で `GROUP BY`、各グループの行を `json_group_array(json_object(...))` で集約
3. `MAX(featured_at)` で並び替え、上位 `limit` 件を取得
4. Rust 側で集約 JSON をパースし、`featured_at DESC, source_id ASC` で並べ、最新行の値を代表値として `AlbumCard` を組み立てる

- [ ] **Step 1: 失敗するテストを書く**

`src/server/store.rs` の `mod tests` 末尾に以下を追加（`use super::*;` と `setup_pool` / `sample` は既存）：

```rust
    fn sample_with(
        source_id: &str,
        external_id: &str,
        artist: &str,
        album: Option<&str>,
        spotify_url: Option<&str>,
        featured_at: NaiveDate,
    ) -> NewRecommendation {
        NewRecommendation {
            source_id: source_id.into(),
            source_url: format!("https://example.com/{source_id}/{external_id}"),
            source_external_id: external_id.into(),
            featured_at,
            artist_name: artist.into(),
            album_name: album.map(|s| s.into()),
            track_name: None,
            spotify_url: spotify_url.map(|s| s.into()),
            spotify_image_url: None,
            youtube_url: None,
        }
    }

    #[tokio::test]
    async fn list_recent_albums_returns_one_card_per_album() {
        let pool = setup_pool().await;
        let repo = RecommendationRepo::new(pool);
        repo.upsert(sample_with(
            "rokinon", "1", "Aldous Harding", Some("Train on the Island"),
            None, NaiveDate::from_ymd_opt(2026, 4, 1).unwrap(),
        )).await.unwrap();

        let cards = repo.list_recent_albums(10).await.unwrap();
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].sources.len(), 1);
        assert_eq!(cards[0].sources[0].source_id, "rokinon");
    }

    #[tokio::test]
    async fn list_recent_albums_merges_same_spotify_url() {
        let pool = setup_pool().await;
        let repo = RecommendationRepo::new(pool);
        let url = "https://open.spotify.com/album/abc";
        repo.upsert(sample_with(
            "rokinon", "r1", "Aldous Harding", Some("Train on the Island"),
            Some(url), NaiveDate::from_ymd_opt(2026, 4, 1).unwrap(),
        )).await.unwrap();
        repo.upsert(sample_with(
            "pitchfork", "p1", "Aldous Harding", Some("Train on the Island"),
            Some(url), NaiveDate::from_ymd_opt(2026, 5, 8).unwrap(),
        )).await.unwrap();

        let cards = repo.list_recent_albums(10).await.unwrap();
        assert_eq!(cards.len(), 1, "same spotify_url must merge");
        assert_eq!(cards[0].sources.len(), 2);
        // 最新ソースが先頭
        assert_eq!(cards[0].sources[0].source_id, "pitchfork");
        assert_eq!(cards[0].featured_at, NaiveDate::from_ymd_opt(2026, 5, 8).unwrap());
    }

    #[tokio::test]
    async fn list_recent_albums_merges_by_normalized_artist_album_when_no_spotify_url() {
        let pool = setup_pool().await;
        let repo = RecommendationRepo::new(pool);
        // 大小・空白の差を吸収できることを確認
        repo.upsert(sample_with(
            "rokinon", "r1", "Aldous Harding", Some("Train on the Island"),
            None, NaiveDate::from_ymd_opt(2026, 4, 1).unwrap(),
        )).await.unwrap();
        repo.upsert(sample_with(
            "pitchfork", "p1", "  aldous harding  ", Some(" train on the island "),
            None, NaiveDate::from_ymd_opt(2026, 5, 8).unwrap(),
        )).await.unwrap();

        let cards = repo.list_recent_albums(10).await.unwrap();
        assert_eq!(cards.len(), 1, "normalized artist+album must merge");
        assert_eq!(cards[0].sources.len(), 2);
    }

    #[tokio::test]
    async fn list_recent_albums_orders_by_latest_featured_at_desc() {
        let pool = setup_pool().await;
        let repo = RecommendationRepo::new(pool);
        // 異なるアルバム 3 件、featured_at バラバラで 3 件以上の順序を検証（CLAUDE.md ルール）
        repo.upsert(sample_with(
            "rokinon", "a", "ArtistA", Some("AlbumA"),
            Some("https://open.spotify.com/album/A"),
            NaiveDate::from_ymd_opt(2026, 3, 1).unwrap(),
        )).await.unwrap();
        repo.upsert(sample_with(
            "rokinon", "b", "ArtistB", Some("AlbumB"),
            Some("https://open.spotify.com/album/B"),
            NaiveDate::from_ymd_opt(2026, 5, 1).unwrap(),
        )).await.unwrap();
        repo.upsert(sample_with(
            "rokinon", "c", "ArtistC", Some("AlbumC"),
            Some("https://open.spotify.com/album/C"),
            NaiveDate::from_ymd_opt(2026, 4, 1).unwrap(),
        )).await.unwrap();

        let cards = repo.list_recent_albums(10).await.unwrap();
        let dates: Vec<_> = cards.iter().map(|c| c.featured_at).collect();
        assert_eq!(dates, vec![
            NaiveDate::from_ymd_opt(2026, 5, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 4, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 3, 1).unwrap(),
        ]);
    }
```

- [ ] **Step 2: テスト失敗を確認**

```bash
mise run test -- list_recent_albums 2>&1 | tail -15
```

期待: 4 つの新規テストが「`list_recent_albums` not found」または `RecommendationRepo` に該当メソッド無しでコンパイルエラー。

- [ ] **Step 3: 実装を書く**

`src/server/store.rs` の `RecommendationRepo` impl ブロックの末尾（`list_recent` の下）に追加：

```rust
    /// 同一アルバム（spotify_url 一致 もしくは 正規化された artist+album 一致）を 1 件にまとめて
    /// 最新 `featured_at` 順で `limit` 件返す。
    pub async fn list_recent_albums(&self, limit: i64) -> AppResult<Vec<crate::domain::album_card::AlbumCard>> {
        use crate::domain::album_card::{AlbumCard, SourceLink};
        use crate::server::error::AppError;

        #[derive(serde::Deserialize)]
        struct RawRow {
            source_id: String,
            source_url: String,
            artist_name: String,
            album_name: Option<String>,
            spotify_url: Option<String>,
            spotify_image_url: Option<String>,
            youtube_url: Option<String>,
            featured_at: chrono::NaiveDate,
        }

        let rows = sqlx::query!(
            r#"WITH keyed AS (
                SELECT
                    COALESCE(
                        spotify_url,
                        lower(trim(artist_name)) || '|' || lower(trim(coalesce(album_name, '')))
                    ) AS dedup_key,
                    source_id,
                    source_url,
                    artist_name,
                    album_name,
                    spotify_url,
                    spotify_image_url,
                    youtube_url,
                    featured_at
                FROM recommendations
            )
            SELECT
                dedup_key AS "dedup_key!: String",
                MAX(featured_at) AS "latest_featured_at!: chrono::NaiveDate",
                json_group_array(json_object(
                    'source_id', source_id,
                    'source_url', source_url,
                    'artist_name', artist_name,
                    'album_name', album_name,
                    'spotify_url', spotify_url,
                    'spotify_image_url', spotify_image_url,
                    'youtube_url', youtube_url,
                    'featured_at', featured_at
                )) AS "rows_json!: String"
            FROM keyed
            GROUP BY dedup_key
            ORDER BY latest_featured_at DESC, dedup_key ASC
            LIMIT ?"#,
            limit,
        )
        .fetch_all(&self.pool)
        .await?;

        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let mut raw: Vec<RawRow> = serde_json::from_str(&row.rows_json)
                .map_err(|e| AppError::Parse(format!("rows_json: {e}")))?;
            // featured_at DESC, source_id ASC
            raw.sort_by(|a, b| b.featured_at.cmp(&a.featured_at).then_with(|| a.source_id.cmp(&b.source_id)));
            let head = raw.first().ok_or_else(|| AppError::Parse("empty group".into()))?;
            out.push(AlbumCard {
                artist_name: head.artist_name.clone(),
                album_name: head.album_name.clone(),
                spotify_url: head.spotify_url.clone(),
                spotify_image_url: head.spotify_image_url.clone(),
                youtube_url: head.youtube_url.clone(),
                featured_at: head.featured_at,
                sources: raw.iter().map(|r| SourceLink {
                    source_id: r.source_id.clone(),
                    source_url: r.source_url.clone(),
                    featured_at: r.featured_at,
                }).collect(),
            });
        }
        Ok(out)
    }
```

- [ ] **Step 4: sqlx offline metadata を更新**

```bash
DATABASE_URL=sqlite:data/app.db cargo sqlx prepare --workspace -- --features ssr --tests
```

期待: `.sqlx/` 配下に新しい query metadata が出る。差分を `git status` で確認。

備考: `data/app.db` が無い場合は事前に `mise run db:create && mise run db:migrate`。

- [ ] **Step 5: テスト pass を確認**

```bash
mise run test -- list_recent_albums 2>&1 | tail -20
```

期待: 4 件すべて pass。

- [ ] **Step 6: Commit**

```bash
git add src/server/store.rs .sqlx/
git commit -m "feat(store): list_recent_albums で同一アルバムをマージして取得"
```

---

## Task 3: Pitchfork 純粋関数（パース）を追加

**Files:**
- Create: `src/server/adapter/pitchfork.rs`
- Modify: `src/server/adapter/mod.rs`

純粋関数を 5 個（DOM/JSON 触らずに `&str` から値を取り出す）：
- `extract_review_urls(index_html: &str) -> Vec<String>`
- `extract_score(review_html: &str) -> Option<f32>`
- `extract_artist(review_html: &str) -> Option<String>`
- `extract_album(review_html: &str) -> Option<String>`
- `extract_publish_date(review_html: &str) -> Option<NaiveDate>`

- [ ] **Step 1: モジュール宣言を追加**

`src/server/adapter/mod.rs` を以下に書き換え：

```rust
pub mod pitchfork;
pub mod rokinon;
pub mod source;
```

- [ ] **Step 2: 最小フィクスチャを作成**

`tests/fixtures/pitchfork/` ディレクトリを作成：

```bash
mkdir -p tests/fixtures/pitchfork
```

`tests/fixtures/pitchfork/index.html` を新規作成：

```html
<!DOCTYPE html>
<html><body>
<a href="/reviews/albums/aldous-harding-train-on-the-island/">Aldous Harding: Train on the Island</a>
<a href="/reviews/albums/the-lemon-twigs-look-for-your-mind/">The Lemon Twigs: Look For Your Mind!</a>
<a href="/reviews/albums/aldous-harding-train-on-the-island/">duplicate link should dedupe</a>
<a href="/reviews/tracks/some-track/">tracks should be ignored</a>
<a href="/news/something/">non-review link</a>
</body></html>
```

`tests/fixtures/pitchfork/review_high.html` を新規作成（スコア 9.0、artist "Aldous Harding"、album "Train on the Island"、公開日 2026-05-08）：

```html
<!DOCTYPE html>
<html><head>
<script type="application/ld+json">{"@context":"http://schema.org","@type":"Review","datePublished":"2026-05-08T00:03:00.000-04:00","modifiedDate":"2026-05-08T00:03:00-04:00","itemReviewed":{"@type":"MusicRecording","name":"Aldous Harding: Train on the Island"}}</script>
</head><body>
<script>window.__PRELOADED_STATE__ = {"headerProps":{"artists":[{"name":"Aldous Harding","uri":"artists/34354-aldous-harding/"}],"dangerousHed":"<em>Train on the Island</em>"},"musicRating":{"isBestNewMusic":true,"isBestNewReissue":false,"score":9},"publishDate":"May 8, 2026"};</script>
</body></html>
```

`tests/fixtures/pitchfork/review_low.html` を新規作成（スコア 7.5、公開日 2026-04-01）：

```html
<!DOCTYPE html>
<html><head>
<script type="application/ld+json">{"@context":"http://schema.org","@type":"Review","datePublished":"2026-04-01T08:00:00.000-04:00","itemReviewed":{"@type":"MusicRecording","name":"Some Artist: Some Album"}}</script>
</head><body>
<script>window.__PRELOADED_STATE__ = {"headerProps":{"artists":[{"name":"Some Artist"}],"dangerousHed":"Some Album"},"musicRating":{"isBestNewMusic":false,"isBestNewReissue":false,"score":7.5},"publishDate":"April 1, 2026"};</script>
</body></html>
```

`tests/fixtures/pitchfork/review_high_old.html` を新規作成（スコア 9.0、公開日 **2020-01-01**＝確実に古い）：

```html
<!DOCTYPE html>
<html><head>
<script type="application/ld+json">{"@context":"http://schema.org","@type":"Review","datePublished":"2020-01-01T00:00:00.000-04:00","itemReviewed":{"@type":"MusicRecording","name":"Old Artist: Old Album"}}</script>
</head><body>
<script>window.__PRELOADED_STATE__ = {"headerProps":{"artists":[{"name":"Old Artist"}],"dangerousHed":"<em>Old Album</em>"},"musicRating":{"isBestNewMusic":true,"isBestNewReissue":false,"score":9},"publishDate":"January 1, 2020"};</script>
</body></html>
```

- [ ] **Step 3: 失敗するテストを書く**

`src/server/adapter/pitchfork.rs` を新規作成：

```rust
use crate::domain::recommendation::NewRecommendation;
use crate::server::adapter::source::{CandidateRef, MediaSource};
use crate::server::error::{AppError, AppResult};
use async_trait::async_trait;
use chrono::{NaiveDate, Utc};
use regex::Regex;
use reqwest::Client;
use std::collections::HashSet;
use std::sync::LazyLock;

const PITCHFORK_BASE: &str = "https://pitchfork.com";
const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36";

static REVIEW_URL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"/reviews/albums/([a-z0-9][a-z0-9-]*)/"#).unwrap());
static SCORE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#""musicRating"\s*:\s*\{[^}]*"score"\s*:\s*([0-9]+(?:\.[0-9]+)?)"#).unwrap());
static ARTIST_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#""headerProps"\s*:\s*\{[^}]*?"artists"\s*:\s*\[\s*\{[^}]*?"name"\s*:\s*"([^"]+)""#).unwrap());
static DANGEROUS_HED_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#""dangerousHed"\s*:\s*"((?:[^"\\]|\\.)*)""#).unwrap());
static EM_TAG_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"\\u003C(?:em|i|strong|b)\\u003E|\\u003C/(?:em|i|strong|b)\\u003E|<(?:em|i|strong|b)>|</(?:em|i|strong|b)>"#).unwrap());

/// レビュー一覧 HTML から `/reviews/albums/<slug>/` 形式のリンクを重複排除して返す。
pub fn extract_review_urls(index_html: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for cap in REVIEW_URL_RE.captures_iter(index_html) {
        let slug = cap.get(1).unwrap().as_str();
        if seen.insert(slug.to_string()) {
            out.push(format!("/reviews/albums/{slug}/"));
        }
    }
    out
}

/// レビューページ HTML から `musicRating.score` を抽出。
pub fn extract_score(review_html: &str) -> Option<f32> {
    SCORE_RE
        .captures(review_html)?
        .get(1)?
        .as_str()
        .parse::<f32>()
        .ok()
}

/// レビューページ HTML から `headerProps.artists[0].name` を抽出。
pub fn extract_artist(review_html: &str) -> Option<String> {
    ARTIST_RE
        .captures(review_html)?
        .get(1)
        .map(|m| m.as_str().to_string())
}

/// レビューページ HTML から `headerProps.dangerousHed` を抽出し、`<em>` 等の HTML タグを除去。
pub fn extract_album(review_html: &str) -> Option<String> {
    let raw = DANGEROUS_HED_RE.captures(review_html)?.get(1)?.as_str();
    let stripped = EM_TAG_RE.replace_all(raw, "");
    let trimmed = stripped.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// レビューページ HTML 内の JSON-LD `Review` ブロックから `datePublished` を抽出し、日付に変換。
pub fn extract_publish_date(review_html: &str) -> Option<NaiveDate> {
    let frag = scraper::Html::parse_document(review_html);
    let sel = scraper::Selector::parse(r#"script[type="application/ld+json"]"#).ok()?;
    for el in frag.select(&sel) {
        let text = el.text().collect::<String>();
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        if v.get("@type").and_then(|x| x.as_str()) != Some("Review") {
            continue;
        }
        let Some(date_str) = v.get("datePublished").and_then(|x| x.as_str()) else {
            continue;
        };
        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(date_str) {
            return Some(dt.naive_utc().date());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(format!("tests/fixtures/pitchfork/{name}")).unwrap()
    }

    #[test]
    fn extract_review_urls_dedupes_and_filters_non_albums() {
        let urls = extract_review_urls(&fixture("index.html"));
        assert_eq!(urls.len(), 2);
        assert!(urls.contains(&"/reviews/albums/aldous-harding-train-on-the-island/".to_string()));
        assert!(urls.contains(&"/reviews/albums/the-lemon-twigs-look-for-your-mind/".to_string()));
    }

    #[test]
    fn extract_score_parses_integer_as_float() {
        assert_eq!(extract_score(&fixture("review_high.html")), Some(9.0));
    }

    #[test]
    fn extract_score_parses_decimal() {
        assert_eq!(extract_score(&fixture("review_low.html")), Some(7.5));
    }

    #[test]
    fn extract_artist_returns_first_artist_name() {
        assert_eq!(extract_artist(&fixture("review_high.html")), Some("Aldous Harding".to_string()));
    }

    #[test]
    fn extract_album_strips_em_tags() {
        assert_eq!(extract_album(&fixture("review_high.html")), Some("Train on the Island".to_string()));
    }

    #[test]
    fn extract_publish_date_parses_iso() {
        assert_eq!(
            extract_publish_date(&fixture("review_high.html")),
            Some(NaiveDate::from_ymd_opt(2026, 5, 8).unwrap())
        );
    }
}
```

- [ ] **Step 4: テスト pass を確認**

```bash
mise run test -- pitchfork 2>&1 | tail -15
```

期待: 6 件全部 pass。

- [ ] **Step 5: Commit**

```bash
git add src/server/adapter/pitchfork.rs src/server/adapter/mod.rs tests/fixtures/pitchfork/
git commit -m "feat(adapter): Pitchfork パーサ純粋関数とフィクスチャを追加"
```

---

## Task 4: `PitchforkAdapter::list_candidates` を追加

**Files:**
- Modify: `src/server/adapter/pitchfork.rs`

- [ ] **Step 1: 失敗するテストを追加**

`src/server/adapter/pitchfork.rs` の `mod tests` 末尾に追加：

```rust
    #[tokio::test]
    async fn list_candidates_fetches_index_and_extracts_urls() {
        use wiremock::{matchers::path, Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        let body = fixture("index.html");
        Mock::given(path("/reviews/albums/"))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .mount(&server)
            .await;

        let adapter = PitchforkAdapter::with_base_url(server.uri(), 8.0, 90, 1);
        let cands = adapter.list_candidates().await.unwrap();
        assert_eq!(cands.len(), 2);
        assert!(cands.iter().any(|c| c.source_external_id == "aldous-harding-train-on-the-island"));
        assert!(cands.iter().all(|c| c.source_url.starts_with(&server.uri())));
    }
```

- [ ] **Step 2: テスト失敗を確認**

```bash
mise run test -- list_candidates_fetches 2>&1 | tail -10
```

期待: `PitchforkAdapter` not found / `with_base_url` not found のコンパイルエラー。

- [ ] **Step 3: アダプタ struct と `list_candidates` を実装**

`src/server/adapter/pitchfork.rs` の `extract_publish_date` の下、`#[cfg(test)]` の上に追加：

```rust
/// Pitchfork のアルバムレビューを `MediaSource` として収集するアダプタ。
pub struct PitchforkAdapter {
    client: Client,
    base_url: String,
    score_threshold: f32,
    recency_days: i64,
    max_pages: u32,
}

impl PitchforkAdapter {
    pub fn new(score_threshold: f32, recency_days: i64, max_pages: u32) -> Self {
        let client = Client::builder()
            .user_agent(USER_AGENT)
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("reqwest client builds");
        Self {
            client,
            base_url: PITCHFORK_BASE.to_string(),
            score_threshold,
            recency_days,
            max_pages,
        }
    }

    pub fn with_base_url(
        base_url: impl Into<String>,
        score_threshold: f32,
        recency_days: i64,
        max_pages: u32,
    ) -> Self {
        let mut me = Self::new(score_threshold, recency_days, max_pages);
        me.base_url = base_url.into();
        me
    }

    fn make_absolute(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }
}

#[async_trait]
impl MediaSource for PitchforkAdapter {
    fn id(&self) -> &'static str {
        "pitchfork"
    }

    async fn list_candidates(&self) -> AppResult<Vec<CandidateRef>> {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for page in 1..=self.max_pages {
            let url = if page == 1 {
                format!("{}/reviews/albums/", self.base_url)
            } else {
                format!("{}/reviews/albums/?page={}", self.base_url, page)
            };
            let resp = self.client.get(&url).send().await?;
            if !resp.status().is_success() {
                tracing::warn!(status = %resp.status(), %url, "pitchfork index fetch failed; stopping pagination");
                break;
            }
            let body = resp.text().await?;
            let page_urls = extract_review_urls(&body);
            if page_urls.is_empty() {
                break;
            }
            for path in page_urls {
                let slug = path.trim_start_matches("/reviews/albums/").trim_end_matches('/');
                if seen.insert(slug.to_string()) {
                    out.push(CandidateRef {
                        source_external_id: slug.to_string(),
                        source_url: self.make_absolute(&path),
                    });
                }
            }
        }
        Ok(out)
    }

    async fn fetch_and_extract(
        &self,
        _candidate: &CandidateRef,
    ) -> AppResult<Option<NewRecommendation>> {
        // Task 5 で実装
        Ok(None)
    }
}
```

- [ ] **Step 4: テスト pass を確認**

```bash
mise run test -- pitchfork 2>&1 | tail -10
```

期待: 7 件全部 pass。

- [ ] **Step 5: Commit**

```bash
git add src/server/adapter/pitchfork.rs
git commit -m "feat(adapter): PitchforkAdapter の list_candidates を実装"
```

---

## Task 5: `PitchforkAdapter::fetch_and_extract` をスコア／recency フィルタ込みで実装

**Files:**
- Modify: `src/server/adapter/pitchfork.rs`

- [ ] **Step 1: 失敗するテストを追加**

`src/server/adapter/pitchfork.rs` の `mod tests` 末尾に追加：

```rust
    #[tokio::test]
    async fn fetch_and_extract_returns_recommendation_for_high_score_recent_review() {
        use wiremock::{matchers::path, Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        Mock::given(path("/reviews/albums/aldous-harding-train-on-the-island/"))
            .respond_with(ResponseTemplate::new(200).set_body_string(fixture("review_high.html")))
            .mount(&server)
            .await;

        // 公開日 2026-05-08 → recency_days=10000 で必ず通る
        let adapter = PitchforkAdapter::with_base_url(server.uri(), 8.0, 10_000, 1);
        let cand = CandidateRef {
            source_external_id: "aldous-harding-train-on-the-island".into(),
            source_url: format!("{}/reviews/albums/aldous-harding-train-on-the-island/", server.uri()),
        };
        let rec = adapter.fetch_and_extract(&cand).await.unwrap().unwrap();
        assert_eq!(rec.source_id, "pitchfork");
        assert_eq!(rec.source_external_id, "aldous-harding-train-on-the-island");
        assert_eq!(rec.artist_name, "Aldous Harding");
        assert_eq!(rec.album_name.as_deref(), Some("Train on the Island"));
        assert_eq!(rec.featured_at, NaiveDate::from_ymd_opt(2026, 5, 8).unwrap());
    }

    #[tokio::test]
    async fn fetch_and_extract_skips_low_score_review() {
        use wiremock::{matchers::path, Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        Mock::given(path("/reviews/albums/some-low-score/"))
            .respond_with(ResponseTemplate::new(200).set_body_string(fixture("review_low.html")))
            .mount(&server)
            .await;

        // score 7.5 < 8.0 → None
        let adapter = PitchforkAdapter::with_base_url(server.uri(), 8.0, 10_000, 1);
        let cand = CandidateRef {
            source_external_id: "some-low-score".into(),
            source_url: format!("{}/reviews/albums/some-low-score/", server.uri()),
        };
        assert!(adapter.fetch_and_extract(&cand).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn fetch_and_extract_skips_old_review_outside_recency_window() {
        use wiremock::{matchers::path, Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        Mock::given(path("/reviews/albums/old-artist-old-album/"))
            .respond_with(ResponseTemplate::new(200).set_body_string(fixture("review_high_old.html")))
            .mount(&server)
            .await;

        // 公開日 2020-01-01、recency_days=90 → 確実に古いとして skip
        let adapter = PitchforkAdapter::with_base_url(server.uri(), 8.0, 90, 1);
        let cand = CandidateRef {
            source_external_id: "old-artist-old-album".into(),
            source_url: format!("{}/reviews/albums/old-artist-old-album/", server.uri()),
        };
        assert!(adapter.fetch_and_extract(&cand).await.unwrap().is_none());
    }
```

- [ ] **Step 2: テスト失敗を確認**

```bash
mise run test -- fetch_and_extract 2>&1 | tail -15
```

期待: 1 件目のテストが「`rec` が None」で fail（現在は仮実装で `Ok(None)` を返している）。

- [ ] **Step 3: 実装を書く**

`src/server/adapter/pitchfork.rs` 内、`MediaSource` impl の `fetch_and_extract` を以下に置き換え：

```rust
    async fn fetch_and_extract(
        &self,
        candidate: &CandidateRef,
    ) -> AppResult<Option<NewRecommendation>> {
        let resp = self.client.get(&candidate.source_url).send().await?;
        if !resp.status().is_success() {
            return Err(AppError::Parse(format!("pitchfork detail HTTP {}", resp.status())));
        }
        let body = resp.text().await?;

        let score = match extract_score(&body) {
            Some(s) => s,
            None => {
                tracing::warn!(url = %candidate.source_url, "score not found");
                return Ok(None);
            }
        };
        if score < self.score_threshold {
            return Ok(None);
        }

        let publish_date = match extract_publish_date(&body) {
            Some(d) => d,
            None => {
                tracing::warn!(url = %candidate.source_url, "publish date not found");
                return Ok(None);
            }
        };
        let today = Utc::now().date_naive();
        let age_days = (today - publish_date).num_days();
        if age_days > self.recency_days {
            return Ok(None);
        }

        let artist = match extract_artist(&body) {
            Some(a) => a,
            None => {
                tracing::warn!(url = %candidate.source_url, "artist not found");
                return Ok(None);
            }
        };
        let album = extract_album(&body);

        Ok(Some(NewRecommendation {
            source_id: "pitchfork".into(),
            source_url: candidate.source_url.clone(),
            source_external_id: candidate.source_external_id.clone(),
            featured_at: publish_date,
            artist_name: artist,
            album_name: album,
            track_name: None,
            spotify_url: None,
            spotify_image_url: None,
            youtube_url: None,
        }))
    }
```

- [ ] **Step 4: テスト pass を確認**

```bash
mise run test -- pitchfork 2>&1 | tail -15
```

期待: 10 件全部 pass。

- [ ] **Step 5: Commit**

```bash
git add src/server/adapter/pitchfork.rs
git commit -m "feat(adapter): Pitchfork fetch_and_extract をスコア・recency フィルタで実装"
```

---

## Task 6: Config に Pitchfork 設定を追加

**Files:**
- Modify: `src/server/config.rs`

- [ ] **Step 1: 失敗するテストを追加**

`src/server/config.rs` の `mod tests` 末尾に追加：

```rust
    #[test]
    fn pitchfork_defaults_when_env_absent() {
        let saved_threshold = std::env::var("PITCHFORK_SCORE_THRESHOLD").ok();
        let saved_recency = std::env::var("PITCHFORK_RECENCY_DAYS").ok();
        let saved_pages = std::env::var("PITCHFORK_MAX_PAGES").ok();
        std::env::remove_var("PITCHFORK_SCORE_THRESHOLD");
        std::env::remove_var("PITCHFORK_RECENCY_DAYS");
        std::env::remove_var("PITCHFORK_MAX_PAGES");
        std::env::set_var("DATABASE_URL", "sqlite::memory:");
        std::env::set_var("SPOTIFY_CLIENT_ID", "x");
        std::env::set_var("SPOTIFY_CLIENT_SECRET", "y");

        let cfg = Config::from_env().unwrap();
        assert!((cfg.pitchfork_score_threshold - 8.0).abs() < f32::EPSILON);
        assert_eq!(cfg.pitchfork_recency_days, 90);
        assert_eq!(cfg.pitchfork_max_pages, 3);

        if let Some(v) = saved_threshold { std::env::set_var("PITCHFORK_SCORE_THRESHOLD", v); }
        if let Some(v) = saved_recency { std::env::set_var("PITCHFORK_RECENCY_DAYS", v); }
        if let Some(v) = saved_pages { std::env::set_var("PITCHFORK_MAX_PAGES", v); }
    }
```

- [ ] **Step 2: テスト失敗を確認**

```bash
mise run test -- pitchfork_defaults 2>&1 | tail -10
```

期待: フィールド未定義のコンパイルエラー。

- [ ] **Step 3: Config に 3 フィールドを追加**

`src/server/config.rs` を以下に書き換え：

```rust
use crate::server::error::{AppError, AppResult};

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub spotify_client_id: String,
    pub spotify_client_secret: String,
    pub pitchfork_score_threshold: f32,
    pub pitchfork_recency_days: i64,
    pub pitchfork_max_pages: u32,
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
            pitchfork_score_threshold: std::env::var("PITCHFORK_SCORE_THRESHOLD")
                .ok()
                .and_then(|v| v.parse::<f32>().ok())
                .unwrap_or(8.0),
            pitchfork_recency_days: std::env::var("PITCHFORK_RECENCY_DAYS")
                .ok()
                .and_then(|v| v.parse::<i64>().ok())
                .unwrap_or(90),
            pitchfork_max_pages: std::env::var("PITCHFORK_MAX_PAGES")
                .ok()
                .and_then(|v| v.parse::<u32>().ok())
                .unwrap_or(3),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_database_url_errors() {
        let saved = std::env::var("DATABASE_URL").ok();
        std::env::remove_var("DATABASE_URL");
        let result = Config::from_env();
        if let Some(v) = saved {
            std::env::set_var("DATABASE_URL", v);
        }
        let err = result.unwrap_err();
        assert!(err.to_string().contains("DATABASE_URL"));
    }

    #[test]
    fn pitchfork_defaults_when_env_absent() {
        let saved_threshold = std::env::var("PITCHFORK_SCORE_THRESHOLD").ok();
        let saved_recency = std::env::var("PITCHFORK_RECENCY_DAYS").ok();
        let saved_pages = std::env::var("PITCHFORK_MAX_PAGES").ok();
        std::env::remove_var("PITCHFORK_SCORE_THRESHOLD");
        std::env::remove_var("PITCHFORK_RECENCY_DAYS");
        std::env::remove_var("PITCHFORK_MAX_PAGES");
        std::env::set_var("DATABASE_URL", "sqlite::memory:");
        std::env::set_var("SPOTIFY_CLIENT_ID", "x");
        std::env::set_var("SPOTIFY_CLIENT_SECRET", "y");

        let cfg = Config::from_env().unwrap();
        assert!((cfg.pitchfork_score_threshold - 8.0).abs() < f32::EPSILON);
        assert_eq!(cfg.pitchfork_recency_days, 90);
        assert_eq!(cfg.pitchfork_max_pages, 3);

        if let Some(v) = saved_threshold { std::env::set_var("PITCHFORK_SCORE_THRESHOLD", v); }
        if let Some(v) = saved_recency { std::env::set_var("PITCHFORK_RECENCY_DAYS", v); }
        if let Some(v) = saved_pages { std::env::set_var("PITCHFORK_MAX_PAGES", v); }
    }
}
```

備考: 既存テスト `missing_database_url_errors` は他テストと共有 env を触るため flaky になり得るが、本タスクでは挙動変更しない（既存問題を持ち込まない）。

- [ ] **Step 4: テスト pass を確認**

```bash
mise run test -- config 2>&1 | tail -10
```

期待: 2 件 pass。

- [ ] **Step 5: Commit**

```bash
git add src/server/config.rs
git commit -m "feat(config): Pitchfork のしきい値設定を環境変数で受け取る"
```

---

## Task 7: scheduler を「複数パイプライン × 個別 cron」に汎用化

**Files:**
- Modify: `src/server/scheduler.rs`

現状の `install_daily_scrape(pipeline)` は 1 個前提。Pitchfork 追加に向けて、`install_scrape_job(scheduler, pipeline, cron_spec)` を切り出して再利用可能にする。

- [ ] **Step 1: scheduler.rs を以下に書き換える**

```rust
use crate::server::error::{AppError, AppResult};
use crate::server::scrape::ScrapePipeline;
use crate::server::scrape_log::ScrapeLog;
use std::sync::Arc;
use tokio_cron_scheduler::{Job, JobScheduler};

/// 空の `JobScheduler` を作成して start する。後段で `add_scrape_job` を呼んでジョブを登録する。
pub async fn new_scheduler() -> AppResult<JobScheduler> {
    let sched = JobScheduler::new()
        .await
        .map_err(|e| AppError::Config(e.to_string()))?;
    sched.start().await.map_err(|e| AppError::Config(e.to_string()))?;
    Ok(sched)
}

/// 指定の cron spec で `pipeline.run()` を発火するジョブを登録する。
pub async fn add_scrape_job(
    scheduler: &JobScheduler,
    pipeline: Arc<ScrapePipeline>,
    cron_spec: &str,
) -> AppResult<()> {
    let p = pipeline.clone();
    let job = Job::new_async(cron_spec, move |_uuid, _l| {
        let p = p.clone();
        Box::pin(async move {
            match p.run().await {
                Ok(o) => tracing::info!(
                    source_id = p.source.id(),
                    added = o.items_added,
                    updated = o.items_updated,
                    "scrape ok"
                ),
                Err(e) => tracing::error!(source_id = p.source.id(), error = %e, "scrape failed"),
            }
        })
    })
    .map_err(|e| AppError::Config(e.to_string()))?;
    scheduler.add(job).await.map_err(|e| AppError::Config(e.to_string()))?;
    Ok(())
}

/// scrape_runs が空の場合のみ初回スクレイプを実行（再起動ループ防止）。
pub async fn run_initial_scrape_if_empty(
    pipeline: Arc<ScrapePipeline>,
    log: Arc<ScrapeLog>,
    source_id: &str,
) -> AppResult<()> {
    if log.count(source_id).await? == 0 {
        tracing::info!(%source_id, "no prior runs; performing initial scrape");
        let _ = pipeline.run().await;
    }
    Ok(())
}
```

備考: 旧 `install_daily_scrape` は削除する。利用元は `main.rs` のみで、Task 9 で書き換える。

- [ ] **Step 2: コンパイル確認（main.rs はまだ古いシグネチャを呼ぶので失敗するはず）**

```bash
cargo check --features ssr 2>&1 | tail -20
```

期待: `main.rs` で `install_daily_scrape` が見つからないエラー。これは Task 9 で解消する。`scheduler.rs` 単体はコンパイル通る。

- [ ] **Step 3: Commit（中間状態だが意味単位でコミットする）**

```bash
git add src/server/scheduler.rs
git commit -m "refactor(scheduler): scrape ジョブ登録を pipeline と cron spec で汎用化"
```

---

## Task 8: home.rs を `AlbumCard` 駆動に変更し、複数ソース時は hover ドロップダウンを描画

**Files:**
- Modify: `src/pages/home.rs`

view レイヤのため自動テストは省略。`cargo leptos build` でコンパイルとアセット出力を検証する。

- [ ] **Step 1: home.rs を以下に書き換える**

```rust
use leptos::prelude::*;

/// `https://open.spotify.com/{kind}/{id}?...` を `spotify:{kind}:{id}` に変換する。
/// Spotify アプリがインストールされとれば URI スキームで直接アプリが開く。
/// 変換できんかったら元の URL をそのまま返す。
fn spotify_app_uri(web_url: &str) -> String {
    let Some(rest) = web_url.strip_prefix("https://open.spotify.com/") else {
        return web_url.to_string();
    };
    let path = rest.split('?').next().unwrap_or(rest);
    let mut parts = path.splitn(2, '/');
    match (parts.next(), parts.next()) {
        (Some(kind), Some(id)) if !kind.is_empty() && !id.is_empty() => {
            format!("spotify:{kind}:{id}")
        }
        _ => web_url.to_string(),
    }
}

/// `source_id` を表示用ラベルに写像する。未知の id はそのまま返す。
fn source_label(source_id: &str) -> &str {
    match source_id {
        "rokinon" => "ロキノン",
        "pitchfork" => "Pitchfork",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spotify_app_uri_converts_album_url() {
        assert_eq!(
            spotify_app_uri("https://open.spotify.com/album/3BU6KQBgOikCUw"),
            "spotify:album:3BU6KQBgOikCUw"
        );
    }

    #[test]
    fn spotify_app_uri_strips_query_string() {
        assert_eq!(
            spotify_app_uri("https://open.spotify.com/track/abc?si=xyz"),
            "spotify:track:abc"
        );
    }

    #[test]
    fn spotify_app_uri_returns_input_for_non_spotify_url() {
        assert_eq!(spotify_app_uri("https://example.com/foo"), "https://example.com/foo");
    }

    #[test]
    fn source_label_known_ids() {
        assert_eq!(source_label("rokinon"), "ロキノン");
        assert_eq!(source_label("pitchfork"), "Pitchfork");
    }

    #[test]
    fn source_label_unknown_id_passthrough() {
        assert_eq!(source_label("nme"), "nme");
    }
}

#[server(ListAlbums, "/api")]
pub async fn list_albums() -> Result<Vec<AlbumCardView>, ServerFnError> {
    use crate::server::store::RecommendationRepo;
    use std::sync::Arc;
    let repo = use_context::<Arc<RecommendationRepo>>()
        .ok_or_else(|| ServerFnError::new("repo missing"))?;
    let cards = repo
        .list_recent_albums(100)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    Ok(cards.into_iter().map(AlbumCardView::from).collect())
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct AlbumCardView {
    pub artist_name: String,
    pub album_name: Option<String>,
    pub spotify_url: Option<String>,
    pub spotify_image_url: Option<String>,
    pub youtube_url: Option<String>,
    pub featured_at: String,
    pub sources: Vec<SourceLinkView>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SourceLinkView {
    pub source_id: String,
    pub source_url: String,
}

#[cfg(feature = "ssr")]
impl From<crate::domain::album_card::AlbumCard> for AlbumCardView {
    fn from(c: crate::domain::album_card::AlbumCard) -> Self {
        Self {
            artist_name: c.artist_name,
            album_name: c.album_name,
            spotify_url: c.spotify_url,
            spotify_image_url: c.spotify_image_url,
            youtube_url: c.youtube_url,
            featured_at: c.featured_at.format("%Y-%m").to_string(),
            sources: c
                .sources
                .into_iter()
                .map(|s| SourceLinkView {
                    source_id: s.source_id,
                    source_url: s.source_url,
                })
                .collect(),
        }
    }
}

#[component]
pub fn Home() -> impl IntoView {
    let cards = Resource::new(|| (), |_| async { list_albums().await });
    view! {
        <header class="border-b-4 border-double border-ink pb-2 mb-6">
            <h1 class="font-zine italic font-bold text-3xl text-ink m-0">
                "i am rockin on"
            </h1>
        </header>
        <Suspense fallback=|| view! { <p class="text-sepia">"loading..."</p> }>
            {move || cards.get().map(|r| match r {
                Ok(items) => view! { <AlbumGrid items=items/> }.into_any(),
                Err(e) => view! {
                    <p class="text-err">{format!("error: {e}")}</p>
                }.into_any(),
            })}
        </Suspense>
    }
}

#[component]
fn AlbumGrid(items: Vec<AlbumCardView>) -> impl IntoView {
    view! {
        <ul class="tilt-cycle list-none p-0 m-0 grid grid-cols-2 tab:grid-cols-3 pc:grid-cols-4 gap-5">
            {items.into_iter().map(|item| view! {
                <li class="bg-card shadow-zine p-3 flex flex-col gap-2">
                    {match item.spotify_image_url.as_ref() {
                        Some(src) => view! {
                            <img
                                class="w-full aspect-square object-cover bg-paper"
                                src=src.clone()
                                alt=""
                                loading="lazy"
                            />
                        }.into_any(),
                        None => view! {
                            <div
                                class="w-full aspect-square bg-placeholder flex items-center justify-center text-sepia text-4xl font-zine"
                                aria-hidden="true"
                            >"♪"</div>
                        }.into_any(),
                    }}
                    <div class="flex flex-col gap-0.5">
                        <div class="font-zine font-bold text-[0.95rem] text-ink leading-tight">
                            {item.artist_name.clone()}
                        </div>
                        {item.album_name.clone().map(|a| view! {
                            <div class="font-zine italic text-[0.8rem] text-sepia leading-tight">{a}</div>
                        })}
                        <div class="text-[0.7rem] text-sepia mt-1">
                            {item.featured_at.clone()}
                        </div>
                    </div>
                    <div class="flex flex-wrap gap-1.5 mt-auto">
                        {item.spotify_url.clone().map(|u| view! {
                            <a
                                class="text-xs font-semibold px-2.5 py-1 rounded-full bg-spotify text-white no-underline"
                                href=spotify_app_uri(&u)
                            >"Spotify"</a>
                            <a
                                class="text-[0.7rem] font-semibold px-2 py-0.5 rounded-full border border-spotify text-spotify no-underline"
                                href=u
                                target="_blank"
                                rel="noopener"
                                title="Web で開く"
                            >"web"</a>
                        })}
                        {item.youtube_url.clone().map(|u| view! {
                            <a
                                class="text-xs font-semibold px-2.5 py-1 rounded-full bg-youtube text-white no-underline"
                                href=u
                                target="_blank"
                                rel="noopener"
                            >"YouTube"</a>
                        })}
                        <SourceMenu sources=item.sources/>
                    </div>
                </li>
            }).collect_view()}
        </ul>
    }
}

#[component]
fn SourceMenu(sources: Vec<SourceLinkView>) -> impl IntoView {
    if sources.len() <= 1 {
        let only = sources.into_iter().next();
        return view! {
            {only.map(|s| view! {
                <a
                    class="text-xs font-semibold px-2.5 py-1 rounded-full border border-ink text-ink no-underline"
                    href=s.source_url
                    target="_blank"
                    rel="noopener"
                >"記事"</a>
            })}
        }.into_any();
    }
    view! {
        <details class="relative group [&[open]>summary]:bg-ink [&[open]>summary]:text-paper">
            <summary
                class="text-xs font-semibold px-2.5 py-1 rounded-full border border-ink text-ink cursor-pointer list-none select-none group-hover:bg-ink group-hover:text-paper"
                role="button"
                aria-haspopup="true"
            >
                "記事"
            </summary>
            <ul class="absolute right-0 mt-1 z-10 bg-card border border-ink shadow-zine min-w-[10rem] list-none p-1 m-0">
                {sources.into_iter().map(|s| view! {
                    <li>
                        <a
                            class="block px-3 py-1.5 text-xs text-ink no-underline hover:bg-paper"
                            href=s.source_url
                            target="_blank"
                            rel="noopener"
                        >{source_label(&s.source_id).to_string()}</a>
                    </li>
                }).collect_view()}
            </ul>
        </details>
    }.into_any()
}
```

備考:
- `<details>` を使うことでキーボード操作（Enter/Space）で開閉できる。`group-hover` は単一ソース時のスタイルとも整合し、Tailwind の `group` 機構で hover も拾う。
- `<details>` の `summary` のリストマーカーは `list-none` で消す（Safari は別途 `summary::-webkit-details-marker { display: none; }` が要る場合がある。問題が出たら `style/tailwind.css` の `@layer base` で対応）。
- `prefers-reduced-motion` 配慮は今回は transition を入れないことで満たす。

- [ ] **Step 2: 純粋関数のテスト pass を確認**

```bash
mise run test -- pages::home 2>&1 | tail -10
```

期待: 5 件 pass（`spotify_app_uri` 3 件 + `source_label` 2 件）。

- [ ] **Step 3: Leptos ビルドが通ることを確認**

```bash
cargo leptos build 2>&1 | tail -20
```

期待: エラー無くビルド完了。

備考: `cargo leptos build` が初回は時間かかる（5〜10 分）。タイムアウトに注意。

- [ ] **Step 4: Commit**

```bash
git add src/pages/home.rs
git commit -m "feat(home): AlbumCard 駆動に変更し、複数ソース時は記事 hover メニューを表示"
```

---

## Task 9: main.rs に Pitchfork パイプラインを登録

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: main.rs を以下に書き換える**

```rust
#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use axum::Router;
    use i_am_rockin_on::server::adapter::pitchfork::PitchforkAdapter;
    use i_am_rockin_on::server::adapter::rokinon::RokinonAdapter;
    use i_am_rockin_on::server::adapter::source::MediaSource;
    use i_am_rockin_on::server::config::Config;
    use i_am_rockin_on::server::resolver::spotify::SpotifyResolver;
    use i_am_rockin_on::server::scheduler::{add_scrape_job, new_scheduler, run_initial_scrape_if_empty};
    use i_am_rockin_on::server::scrape::ScrapePipeline;
    use i_am_rockin_on::server::scrape_log::ScrapeLog;
    use i_am_rockin_on::server::store::RecommendationRepo;
    use i_am_rockin_on::{shell, App};
    use leptos::prelude::*;
    use leptos_axum::{generate_route_list, LeptosRoutes};
    use std::str::FromStr;
    use std::sync::Arc;

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let _ = dotenvy::dotenv();

    let cfg = Config::from_env()?;
    let connect_opts = sqlx::sqlite::SqliteConnectOptions::from_str(&cfg.database_url)?
        .create_if_missing(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(connect_opts)
        .await?;
    sqlx::migrate!().run(&pool).await?;

    let resolver = Arc::new(SpotifyResolver::new(
        cfg.spotify_client_id.clone(),
        cfg.spotify_client_secret.clone(),
    ));
    let repo = Arc::new(RecommendationRepo::new(pool.clone()));
    let log = Arc::new(ScrapeLog::new(pool.clone()));

    // Rokinon
    let rokinon_source: Arc<dyn MediaSource> = Arc::new(RokinonAdapter::new());
    let rokinon_pipeline = Arc::new(ScrapePipeline {
        source: rokinon_source,
        resolver: resolver.clone(),
        repo: repo.clone(),
        log: log.clone(),
    });

    // Pitchfork
    let pitchfork_source: Arc<dyn MediaSource> = Arc::new(PitchforkAdapter::new(
        cfg.pitchfork_score_threshold,
        cfg.pitchfork_recency_days,
        cfg.pitchfork_max_pages,
    ));
    let pitchfork_pipeline = Arc::new(ScrapePipeline {
        source: pitchfork_source,
        resolver: resolver.clone(),
        repo: repo.clone(),
        log: log.clone(),
    });

    // 初回スクレイプ（バックグラウンド）
    {
        let p = rokinon_pipeline.clone();
        let l = log.clone();
        tokio::spawn(async move {
            if let Err(e) = run_initial_scrape_if_empty(p, l, "rokinon").await {
                tracing::error!(error = %e, "initial rokinon scrape failed");
            }
        });
    }
    {
        let p = pitchfork_pipeline.clone();
        let l = log.clone();
        tokio::spawn(async move {
            if let Err(e) = run_initial_scrape_if_empty(p, l, "pitchfork").await {
                tracing::error!(error = %e, "initial pitchfork scrape failed");
            }
        });
    }

    // 日次 cron（時刻をずらして同時走行を避ける。Rokinon: JST 04:00 = UTC 19:00、Pitchfork: JST 16:00 = UTC 07:00）
    let scheduler = new_scheduler().await?;
    add_scrape_job(&scheduler, rokinon_pipeline.clone(), "0 0 19 * * *").await?;
    add_scrape_job(&scheduler, pitchfork_pipeline.clone(), "0 0 7 * * *").await?;
    let _sched = scheduler;

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
                move || shell(opts.clone())
            },
        )
        .route("/healthz", axum::routing::get(|| async { "ok" }))
        .fallback(leptos_axum::file_and_error_handler(shell))
        .with_state(leptos_options);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("listening on http://{}", &addr);
    let shutdown = async {
        let _ = tokio::signal::ctrl_c().await;
        tracing::info!("shutdown signal received");
    };
    axum::serve(listener, app.into_make_service())
        .with_graceful_shutdown(shutdown)
        .await?;
    Ok(())
}

#[cfg(not(feature = "ssr"))]
pub fn main() {}
```

- [ ] **Step 2: ビルドが通ることを確認**

```bash
cargo check --features ssr 2>&1 | tail -10
```

期待: warnings 程度、エラー無し。

- [ ] **Step 3: 全テスト pass を確認**

```bash
mise run test 2>&1 | tail -15
```

期待: 既存全テスト + 新規テスト全部 pass。

- [ ] **Step 4: Commit**

```bash
git add src/main.rs
git commit -m "feat: Pitchfork パイプラインを起動時に登録し別時刻で cron 配線"
```

---

## Task 10: 動作確認とドキュメント更新

**Files:**
- Modify: `docs/TODO.md` (関連項目を消化済みに更新)

- [ ] **Step 1: ローカル DB で動作確認**

```bash
DATABASE_URL=sqlite:data/app.db mise run scrape -- --source pitchfork 2>&1 | tail -30
```

期待:
- Pitchfork 一覧ページ・詳細ページが取得される（ログ）
- スコア < 8.0 や recency 外は skipped としてカウントされる
- スコア 8.0+ 直近 90 日のレビューが items_added/items_updated される

備考: `src/bin/scrape.rs` のフラグ仕様によっては `--source` の代わりに別の指定方法がある。実際のコマンドを確認して読み替える（`mise run scrape -- --help`）。Pitchfork が CLI で起動できない場合は、`mise run dev` を立ち上げて初回起動時の Pitchfork 自動スクレイプログを `RUST_LOG=info` で観察する代替でよい。

- [ ] **Step 2: Home 画面を視認確認**

```bash
mise run dev
```

別ターミナルで `open http://localhost:3000` し、以下を目視確認：

1. Pitchfork レビュー由来のカードが表示される（DB に既存の Rokinon と混在）
2. 同一 Spotify URL で Rokinon と Pitchfork 両方が拾った場合、カードが 1 枚に統合され、「記事」ボタン hover で「ロキノン」「Pitchfork」が選べる
3. 単独ソースのカードは「記事」ボタンが直接リンクのまま（hover で展開しない）
4. キーボード操作（Tab で「記事」フォーカス → Enter で開閉）が機能する

- [ ] **Step 3: TODO.md を更新**

`docs/TODO.md` の "機能拡張（要相談）" セクションの「他メディアソース追加」項目に Pitchfork 完了の追記を入れる。具体例：

```
- [x] **他メディアソース追加（Pitchfork）** — `src/server/adapter/pitchfork.rs` で 8.0+ かつ直近 90 日を取得
```

該当行を編集（既存表現に合わせる）。

- [ ] **Step 4: Commit**

```bash
git add docs/TODO.md
git commit -m "docs(TODO): Pitchfork ソース追加を完了として更新"
```

- [ ] **Step 5: 最終確認**

```bash
mise run test 2>&1 | tail -10 && cargo leptos build 2>&1 | tail -5
```

期待: 全テスト pass、ビルド成功。

---

## 完了の定義

以下がすべて満たされていれば本プランは完了：

- [ ] `cargo test --features ssr` がすべて pass（新規 12+ 件、既存全件）
- [ ] `cargo leptos build` が成功する
- [ ] ローカル `mise run dev` で Pitchfork レビュー由来カードが表示される
- [ ] 同一アルバム（Spotify URL 一致 もしくは 正規化 artist+album 一致）が 1 カードにマージされる
- [ ] 「記事」ボタン hover で複数ソースの選択メニューが出る／単一ソースは直接リンクのまま
- [ ] スコア 8.0+ かつ公開日 90 日以内のレビューだけが保存されている
- [ ] 各タスクの commit が意味単位で分割されている（マイグレーション無いがアダプタ／ストア／設定／scheduler／view／main の 9 commit + ドキュメント 1 commit）
