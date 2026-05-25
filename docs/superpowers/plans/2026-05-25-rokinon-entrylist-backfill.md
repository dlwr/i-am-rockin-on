# rokinon entrylist バックフィル Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** rokinon の取り込みを RSS（最新10件）から entrylist 走査に切り替え、`scraped_entries` テーブルで記事ごとに既処理追跡することで、RSS の窓から出た過去の推し記事（例: `entry-12966301740`）を取り込み、同種の取りこぼしを構造的に防止する。

**Architecture:** 記事ごとの処理済みを `scraped_entries` テーブルで明示追跡。パイプラインは fetch 前に `is_scraped` でスキップ（全ソース共通）。rokinon は entrylist を `ROKINON_MAX_PAGES`（デフォルト5）まで走査して候補列挙し、各記事はフル HTML ページを fetch して `#entryBody` にスコープした抽出を行う。bootstrap マイグレーションで既存 recommendations を seen 登録し、初回 cron での全件 re-fetch を防ぐ。

**Tech Stack:** Rust, sqlx (SQLite, コンパイル時検証マクロ + `.sqlx` オフラインキャッシュ), scraper, reqwest, async-trait, wiremock（テスト）, tokio。

設計の根拠は `docs/superpowers/specs/2026-05-25-rokinon-entrylist-backfill-design.md` を参照。

---

## ファイル構成

- `migrations/20260525000001_scraped_entries.sql` — 新規。`scraped_entries` テーブル + 既存 recommendations からの bootstrap。
- `src/server/store.rs` — `is_scraped` / `mark_scraped` を追加。
- `src/server/scrape.rs` — `process_candidate` に fetch 前スキップ + mark を追加。
- `src/server/adapter/rokinon.rs` — `list_candidates`（entrylist 走査）/ `fetch_and_extract`（フルページ）書き換え、抽出関数の `#entryBody` スコープ化、og:title からの artist 抽出、RSS キャッシュ撤去。
- `src/server/config.rs` — `rokinon_max_pages` 追加。
- `src/main.rs` / `src/bin/scrape.rs` — `RokinonAdapter` 構築箇所に max_pages / throttle を渡す。
- `tests/fixtures/rokinon/entry-12966301740.html` — 追加済み（ゴールデンフィクスチャ）。
- `tests/fixtures/rokinon/entrylist.html` / `entrylist-2.html` — entrylist テスト用フィクスチャ（Task 6 で作成）。
- `README.md` — `ROKINON_MAX_PAGES` 環境変数を追記。

各ステップで CLAUDE.md ルールを遵守: アソシエーションのテストは書かない / 1 テスト関数 = 1 振る舞い / 順序検証は3件以上 / TDD（t_wada）。

---

## Task 1: `scraped_entries` マイグレーション

**Files:**
- Create: `migrations/20260525000001_scraped_entries.sql`

- [ ] **Step 1: マイグレーションファイルを作成**

`migrations/20260525000001_scraped_entries.sql`:

```sql
-- 記事ごとの処理済み追跡。非推し記事も含めて記録し、entrylist 走査での re-fetch を防ぐ。
CREATE TABLE scraped_entries (
    source_id   TEXT NOT NULL,
    external_id TEXT NOT NULL,
    scraped_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (source_id, external_id)
);

-- bootstrap: 既存 recommendations を処理済みとして登録。
-- これを忘れると初回 cron で既知の推し記事まで全件 re-fetch + Spotify 再解決の嵐になる。
INSERT INTO scraped_entries (source_id, external_id, scraped_at)
SELECT source_id, source_external_id, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
FROM recommendations;
```

- [ ] **Step 2: マイグレーションが適用できることを確認**

Run: `DATABASE_URL=sqlite:data/app.db cargo sqlx migrate run`
Expected: `Applied 20260525000001/migrate scraped_entries` のような成功出力。エラーが出ないこと。

- [ ] **Step 3: bootstrap が効いているか確認**

Run: `sqlite3 data/app.db "SELECT count(*) FROM scraped_entries;"` と `sqlite3 data/app.db "SELECT count(*) FROM recommendations;"`
Expected: scraped_entries の件数 == recommendations の件数（bootstrap で全件コピーされた）。

- [ ] **Step 4: コミット**

```bash
git add migrations/20260525000001_scraped_entries.sql
git commit -m "feat(db): scraped_entries テーブル追加 + 既存 recommendations から bootstrap"
```

---

## Task 2: repo に `is_scraped` / `mark_scraped`

**Files:**
- Modify: `src/server/store.rs`
- Test: `src/server/store.rs`（同ファイル内 `#[cfg(test)] mod tests`）

既存テストは `setup_pool()`（インメモリ SQLite + `sqlx::migrate!()` 実行）と `sample(id)` ヘルパを使う。それに倣う。

- [ ] **Step 1: 失敗するテストを書く**

`src/server/store.rs` の `mod tests` 末尾に追加:

```rust
#[tokio::test]
async fn is_scraped_returns_false_for_unknown_entry() {
    let pool = setup_pool().await;
    let repo = RecommendationRepo::new(pool);
    assert!(!repo.is_scraped("rokinon", "999999").await.unwrap());
}

#[tokio::test]
async fn mark_scraped_then_is_scraped_returns_true() {
    let pool = setup_pool().await;
    let repo = RecommendationRepo::new(pool);
    repo.mark_scraped("rokinon", "12345").await.unwrap();
    assert!(repo.is_scraped("rokinon", "12345").await.unwrap());
}

#[tokio::test]
async fn is_scraped_is_scoped_by_source_id() {
    let pool = setup_pool().await;
    let repo = RecommendationRepo::new(pool);
    repo.mark_scraped("rokinon", "12345").await.unwrap();
    assert!(!repo.is_scraped("pitchfork", "12345").await.unwrap());
}

#[tokio::test]
async fn mark_scraped_is_idempotent() {
    let pool = setup_pool().await;
    let repo = RecommendationRepo::new(pool);
    repo.mark_scraped("rokinon", "12345").await.unwrap();
    // 二度目でも PRIMARY KEY 衝突エラーにならない
    repo.mark_scraped("rokinon", "12345").await.unwrap();
    assert!(repo.is_scraped("rokinon", "12345").await.unwrap());
}
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `cargo test --features ssr is_scraped`
Expected: コンパイルエラー（`is_scraped` / `mark_scraped` メソッドが存在しない）。

- [ ] **Step 3: 最小実装**

`src/server/store.rs` の `impl RecommendationRepo` 内（`upsert` の後あたり）に追加:

```rust
/// (source_id, external_id) が scraped_entries に存在するか。
pub async fn is_scraped(&self, source_id: &str, external_id: &str) -> AppResult<bool> {
    let row = sqlx::query_scalar!(
        r#"SELECT EXISTS(
               SELECT 1 FROM scraped_entries
               WHERE source_id = ? AND external_id = ?
           ) as "exists!: i64""#,
        source_id,
        external_id,
    )
    .fetch_one(&self.pool)
    .await?;
    Ok(row != 0)
}

/// (source_id, external_id) を処理済みとして記録。既存なら何もしない（冪等）。
pub async fn mark_scraped(&self, source_id: &str, external_id: &str) -> AppResult<()> {
    sqlx::query!(
        r#"INSERT OR IGNORE INTO scraped_entries (source_id, external_id)
           VALUES (?, ?)"#,
        source_id,
        external_id,
    )
    .execute(&self.pool)
    .await?;
    Ok(())
}
```

- [ ] **Step 4: sqlx オフラインキャッシュを再生成**

Run: `DATABASE_URL=sqlite:data/app.db cargo sqlx prepare -- --features ssr`
Expected: `.sqlx/` に新クエリの json が生成され、エラーが出ないこと。

- [ ] **Step 5: テストが通ることを確認**

Run: `cargo test --features ssr is_scraped mark_scraped`
Expected: 4テストすべて PASS。

- [ ] **Step 6: コミット**

```bash
git add src/server/store.rs .sqlx
git commit -m "feat(store): is_scraped / mark_scraped を追加"
```

---

## Task 3: パイプラインに fetch 前スキップ

**Files:**
- Modify: `src/server/scrape.rs`（`process_candidate` と `ScrapePipeline`）
- Test: `src/server/scrape.rs`（`mod tests`）

`process_candidate` は現在 `fetch_and_extract` → resolve → `upsert` の順。これを「先に `is_scraped` でスキップ、処理後に `mark_scraped`、ただし transient 失敗では mark しない」に変更する。`process_candidate` は `repo` にアクセスできる（`self.repo`）。

既存テストの `FakeSource` は `items: Vec<NewRecommendation>` を返す（呼ばれた候補すべてを推しとして返す）。fetch 呼び出し回数を観測するため、呼び出しカウンタ付きの新しいフェイクを使う。

- [ ] **Step 1: 失敗するテストを書く**

`src/server/scrape.rs` の `mod tests` に追加（既存の `FakeSource` 等の近く）:

```rust
use std::sync::atomic::{AtomicUsize, Ordering};

// fetch_and_extract の呼び出し回数を記録するフェイク。常に推しを1件返す。
struct CountingSource {
    fetch_calls: Arc<AtomicUsize>,
}

#[async_trait]
impl MediaSource for CountingSource {
    fn id(&self) -> &'static str {
        "fake"
    }
    async fn list_candidates(&self) -> AppResult<Vec<CandidateRef>> {
        Ok(vec![CandidateRef {
            source_external_id: "e1".into(),
            source_url: "https://example.com/entry-e1.html".into(),
        }])
    }
    async fn fetch_and_extract(
        &self,
        _c: &CandidateRef,
    ) -> AppResult<Option<NewRecommendation>> {
        self.fetch_calls.fetch_add(1, Ordering::SeqCst);
        Ok(Some(NewRecommendation {
            source_id: "fake".into(),
            source_url: "https://example.com/entry-e1.html".into(),
            source_external_id: "e1".into(),
            featured_at: NaiveDate::from_ymd_opt(2026, 5, 1).unwrap(),
            artist_name: "Artist".into(),
            album_name: Some("Album".into()),
            track_name: None,
            spotify_url: None,
            spotify_image_url: None,
            youtube_url: None,
        }))
    }
}

#[tokio::test]
async fn process_candidate_skips_fetch_when_already_scraped() {
    let pool = SqlitePoolOptions::new().connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!().run(&pool).await.unwrap();
    let repo = Arc::new(RecommendationRepo::new(pool.clone()));
    repo.mark_scraped("fake", "e1").await.unwrap();

    let fetch_calls = Arc::new(AtomicUsize::new(0));
    let pipeline = ScrapePipeline {
        source: Arc::new(CountingSource { fetch_calls: fetch_calls.clone() }),
        resolver: Arc::new(SpotifyResolver::new("id".into(), "secret".into())),
        repo,
        log: Arc::new(ScrapeLog::new(pool)),
        cancel: CancellationToken::new(),
        throttle_ms: 0,
    };
    let outcome = pipeline.run().await.unwrap();
    assert_eq!(fetch_calls.load(Ordering::SeqCst), 0, "既処理なら fetch しない");
    assert_eq!(outcome.items_skipped, 1);
}

#[tokio::test]
async fn process_candidate_marks_scraped_after_non_oshi() {
    // fetch_and_extract が None（非推し）を返したら mark_scraped される
    let pool = SqlitePoolOptions::new().connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!().run(&pool).await.unwrap();
    let repo = Arc::new(RecommendationRepo::new(pool.clone()));

    struct NoneSource;
    #[async_trait]
    impl MediaSource for NoneSource {
        fn id(&self) -> &'static str { "fake" }
        async fn list_candidates(&self) -> AppResult<Vec<CandidateRef>> {
            Ok(vec![CandidateRef {
                source_external_id: "e2".into(),
                source_url: "https://example.com/entry-e2.html".into(),
            }])
        }
        async fn fetch_and_extract(&self, _c: &CandidateRef) -> AppResult<Option<NewRecommendation>> {
            Ok(None)
        }
    }

    let pipeline = ScrapePipeline {
        source: Arc::new(NoneSource),
        resolver: Arc::new(SpotifyResolver::new("id".into(), "secret".into())),
        repo: repo.clone(),
        log: Arc::new(ScrapeLog::new(pool)),
        cancel: CancellationToken::new(),
        throttle_ms: 0,
    };
    pipeline.run().await.unwrap();
    assert!(repo.is_scraped("fake", "e2").await.unwrap(), "非推しも mark される");
}
```

> 注: `process_candidate_marks_scraped_after_non_oshi` は Spotify 解決に到達しないので外部通信なし。`CountingSource` のテストは既処理スキップで fetch に到達しないため、Spotify 解決にも到達しない（外部通信なし）。「推し成功時に mark」「transient 失敗時に mark しない」は Spotify モックが必要で既存テスト基盤に無いため、ここでは検証しない（実装はレビューで確認）。

- [ ] **Step 2: テストが失敗することを確認**

Run: `cargo test --features ssr process_candidate_skips_fetch process_candidate_marks_scraped`
Expected: FAIL。既処理でも fetch されてしまう（`fetch_calls == 1`）、または非推しで mark されない。

- [ ] **Step 3: 実装**

`src/server/scrape.rs` の `process_candidate` を以下に置き換える:

```rust
async fn process_candidate(
    &self,
    c: &crate::server::adapter::source::CandidateRef,
) -> AppResult<ProcessResult> {
    let source_id = self.source.id();
    // 既処理ならフルページ fetch を回避してスキップ
    if self.repo.is_scraped(source_id, &c.source_external_id).await? {
        return Ok(ProcessResult::Skipped);
    }

    let extracted = match self.source.fetch_and_extract(c).await? {
        Some(item) => item,
        None => {
            // 非推し（取り込み対象外）も処理済みとして記録し、再 fetch を防ぐ
            self.repo.mark_scraped(source_id, &c.source_external_id).await?;
            return Ok(ProcessResult::Skipped);
        }
    };
    let mut new_rec = extracted;
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
            // transient の可能性があるので mark しない（次回リトライ）
            return Ok(ProcessResult::Skipped);
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                artist = %new_rec.artist_name,
                "spotify resolve failed; skipping recommendation (will retry next scrape)"
            );
            // transient。mark しない（次回リトライ）
            return Ok(ProcessResult::Skipped);
        }
    }
    let (_, inserted) = self.repo.upsert(new_rec).await?;
    // 推し + Spotify 解決成功 → 処理済みとして記録
    self.repo.mark_scraped(source_id, &c.source_external_id).await?;
    Ok(if inserted {
        ProcessResult::Inserted
    } else {
        ProcessResult::Updated
    })
}
```

> HTTP fetch error は `fetch_and_extract` が `Err` を返し、呼び出し元 `run_inner` の `Err(e) => { ... items_skipped += 1 }` で処理される（mark されない＝リトライ）。この経路は変更不要。

- [ ] **Step 4: テストが通ることを確認**

Run: `cargo test --features ssr process_candidate_skips_fetch process_candidate_marks_scraped`
Expected: 両テスト PASS。

- [ ] **Step 5: 既存パイプラインテストが壊れていないことを確認**

Run: `cargo test --features ssr --lib server::scrape`
Expected: 既存テストも含め全 PASS。

- [ ] **Step 6: コミット**

```bash
git add src/server/scrape.rs
git commit -m "feat(scrape): fetch 前に is_scraped でスキップ、処理後に mark_scraped"
```

---

## Task 4: rokinon 抽出関数を `#entryBody` スコープ化 + og:title から artist

**Files:**
- Modify: `src/server/adapter/rokinon.rs`
- Test: `src/server/adapter/rokinon.rs`（`mod tests`、ゴールデンフィクスチャ `entry-12966301740.html` 使用）

フルページ HTML から本文を取り出すヘルパと、og:title から記事タイトルを取り出すヘルパを追加し、ゴールデンフィクスチャで検証する。`extract_youtube_url` は `iframe[src]` フォールバックを追加。

- [ ] **Step 1: 失敗するテストを書く**

`src/server/adapter/rokinon.rs` の `mod tests` に追加:

```rust
#[test]
fn extract_article_body_scopes_to_entry_body() {
    let page = fixture("entry-12966301740.html");
    let body = extract_article_body(&page).expect("entryBody present");
    // entryBody 内の推しマーカーを検出（外側のサイドバー分は含めない）
    assert_eq!(
        detect_oshi(&body.text),
        Some(NaiveDate::from_ymd_opt(2026, 5, 1).unwrap())
    );
}

#[test]
fn extract_album_from_scoped_body_returns_ogp_title() {
    let page = fixture("entry-12966301740.html");
    let body = extract_article_body(&page).expect("entryBody present");
    // 本文に h2 は無く ogpCard_title から取得される
    assert_eq!(
        extract_album_from_html(&body.html).unwrap(),
        "The Secret To Good Living"
    );
}

#[test]
fn extract_youtube_from_scoped_body_returns_playlist_link() {
    let page = fixture("entry-12966301740.html");
    let body = extract_article_body(&page).expect("entryBody present");
    assert_eq!(
        extract_youtube_url(&body.html).unwrap(),
        "https://www.youtube.com/playlist?list=OLAK5uy_n-3r7edlfatPi4p5z1KuG-wCI-0NP88ug"
    );
}

#[test]
fn extract_youtube_url_falls_back_to_iframe() {
    let html = r#"<p>no anchor</p><iframe src="https://www.youtube.com/embed/abc123?x=1"></iframe>"#;
    assert_eq!(
        extract_youtube_url(html).unwrap(),
        "https://www.youtube.com/embed/abc123?x=1"
    );
}

#[test]
fn extract_entry_title_reads_og_title_and_strips_brackets() {
    let page = fixture("entry-12966301740.html");
    let title = extract_entry_title(&page).unwrap();
    assert_eq!(title, "Hiding Places  の新作");
}

#[test]
fn extract_artist_from_og_title_yields_clean_name() {
    let page = fixture("entry-12966301740.html");
    let title = extract_entry_title(&page).unwrap();
    assert_eq!(extract_artist_name(&title), "Hiding Places");
}
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `cargo test --features ssr --lib adapter::rokinon`
Expected: コンパイルエラー（`extract_article_body` / `ArticleBody` / `extract_entry_title` が存在しない）。

- [ ] **Step 3: 実装**

`src/server/adapter/rokinon.rs` に追加・変更:

```rust
/// 記事本文（#entryBody）のスコープ済み HTML とテキスト。
pub struct ArticleBody {
    pub html: String,
    pub text: String,
}

/// フル記事ページ HTML から本文コンテナ `#entryBody`（= `.articleText`）を取り出す。
/// サイドバーや関連記事を除外するため、抽出はこのスコープ内に限定する。
pub fn extract_article_body(page_html: &str) -> Option<ArticleBody> {
    let doc = Html::parse_document(page_html);
    let sel = Selector::parse("#entryBody").ok()?;
    let el = doc.select(&sel).next()?;
    Some(ArticleBody {
        html: el.inner_html(),
        text: el.text().collect::<String>(),
    })
}

/// フル記事ページの `<meta property="og:title">` から記事タイトルを取り出す。
/// 装飾の全角括弧 `『』` を除去し、前後空白を trim する。
pub fn extract_entry_title(page_html: &str) -> Option<String> {
    let doc = Html::parse_document(page_html);
    let sel = Selector::parse(r#"meta[property="og:title"]"#).ok()?;
    let raw = doc.select(&sel).next()?.value().attr("content")?;
    let cleaned = raw.replace(['『', '』'], "");
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}
```

`extract_youtube_url` を `a[href]` → `iframe[src]` の順に拡張:

```rust
/// 記事本文 HTML から最初の YouTube リンクを取り出す。
/// まず `<a href>`、無ければ `<iframe src>`（埋め込み）を探す。
pub fn extract_youtube_url(entry_html: &str) -> Option<String> {
    let frag = Html::parse_fragment(entry_html);
    let is_yt = |s: &str| s.contains("youtube.com") || s.contains("youtu.be");

    if let Ok(a_sel) = Selector::parse("a[href]") {
        if let Some(href) = frag
            .select(&a_sel)
            .filter_map(|el| el.value().attr("href"))
            .find(|h| is_yt(h))
        {
            return Some(href.to_string());
        }
    }
    let iframe_sel = Selector::parse("iframe[src]").ok()?;
    frag.select(&iframe_sel)
        .filter_map(|el| el.value().attr("src"))
        .find(|s| is_yt(s))
        .map(|s| s.to_string())
}
```

> `detect_oshi` / `extract_album_from_html` / `normalize_album_name` / `extract_artist_name` のシグネチャは変更しない（`extract_article_body` が返す `html` / `text` を渡して再利用する）。

- [ ] **Step 4: テストが通ることを確認**

Run: `cargo test --features ssr --lib adapter::rokinon`
Expected: 新規6テスト + 既存抽出テストすべて PASS。`extract_youtube_url_finds_link`（既存の a[href] テスト）も維持されること。

- [ ] **Step 5: コミット**

```bash
git add src/server/adapter/rokinon.rs
git commit -m "feat(rokinon): #entryBody スコープ抽出 + og:title から artist + iframe youtube 対応"
```

---

## Task 5: config に `rokinon_max_pages`

**Files:**
- Modify: `src/server/config.rs`
- Test: `src/server/config.rs`（`mod tests`）

`pitchfork_max_pages`（デフォルト3）と同じパターンで `rokinon_max_pages`（デフォルト5）を追加する。

- [ ] **Step 1: 失敗するテストを書く**

`src/server/config.rs` の `mod tests` の既存 `from_env` テスト（`scrape_throttle_ms == 800` を確認している関数）に1行追加:

```rust
assert_eq!(cfg.rokinon_max_pages, 5);
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `cargo test --features ssr --lib server::config`
Expected: コンパイルエラー（`rokinon_max_pages` フィールドが無い）。

- [ ] **Step 3: 実装**

`src/server/config.rs` の `Config` 構造体に追加（`pitchfork_max_pages` の近く）:

```rust
    pub rokinon_max_pages: u32,
```

`from_env` 内に追加（`pitchfork_max_pages` の設定行に倣う）:

```rust
            rokinon_max_pages: std::env::var("ROKINON_MAX_PAGES")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(5),
```

- [ ] **Step 4: テストが通ることを確認**

Run: `cargo test --features ssr --lib server::config`
Expected: PASS。

- [ ] **Step 5: コミット**

```bash
git add src/server/config.rs
git commit -m "feat(config): ROKINON_MAX_PAGES (デフォルト5) を追加"
```

---

## Task 6: rokinon を entrylist 走査に切り替え（list_candidates / fetch_and_extract / struct）

**Files:**
- Modify: `src/server/adapter/rokinon.rs`
- Test: `src/server/adapter/rokinon.rs`（`mod tests`、wiremock）
- Create: `tests/fixtures/rokinon/entrylist.html`, `tests/fixtures/rokinon/entrylist-2.html`

RSS キャッシュ機構を撤去し、entrylist ページネーション + フルページ fetch に変更する。`RokinonAdapter` に `max_pages` と `throttle_ms` を持たせる。

- [ ] **Step 1: entrylist テストフィクスチャを作成**

`tests/fixtures/rokinon/entrylist.html`:

```html
<!DOCTYPE html><html><body>
<ul class="skin-entryList">
  <li><a href="https://ameblo.jp/stamedba/entry-12967130684.html">A の新作</a></li>
  <li><a href="https://ameblo.jp/stamedba/entry-12967130162.html">B の新作</a></li>
  <li><a href="https://ameblo.jp/stamedba/entry-12967130684.html">A の新作（重複）</a></li>
</ul>
</body></html>
```

`tests/fixtures/rokinon/entrylist-2.html`:

```html
<!DOCTYPE html><html><body>
<ul class="skin-entryList">
  <li><a href="https://ameblo.jp/stamedba/entry-12966301740.html">Hiding Places の新作</a></li>
</ul>
</body></html>
```

- [ ] **Step 2: 失敗するテストを書く**

`src/server/adapter/rokinon.rs` の `mod tests` の RSS 系テスト（`list_candidates_parses_rss_fixture` 等）を削除し、以下に置き換える:

```rust
#[tokio::test]
async fn list_candidates_paginates_entrylist_and_dedupes() {
    use wiremock::{matchers::path, Mock, MockServer, ResponseTemplate};
    let server = MockServer::start().await;
    let p1 = std::fs::read_to_string("tests/fixtures/rokinon/entrylist.html").unwrap();
    let p2 = std::fs::read_to_string("tests/fixtures/rokinon/entrylist-2.html").unwrap();
    Mock::given(path("/entrylist.html"))
        .respond_with(ResponseTemplate::new(200).set_body_string(p1))
        .mount(&server)
        .await;
    Mock::given(path("/entrylist-2.html"))
        .respond_with(ResponseTemplate::new(200).set_body_string(p2))
        .mount(&server)
        .await;

    let adapter = RokinonAdapter::with_base_url(server.uri());
    let cands = adapter.list_candidates().await.unwrap();
    let ids: Vec<&str> = cands.iter().map(|c| c.source_external_id.as_str()).collect();
    // page1 で2件（重複排除済み）+ page2 で対象1件 = 3件
    assert_eq!(cands.len(), 3, "page をまたいで重複排除し全件列挙");
    assert!(ids.contains(&"12966301740"), "対象記事が候補に含まれる");
    assert!(ids.iter().all(|id| !id.is_empty()));
}

#[tokio::test]
async fn list_candidates_stops_pagination_on_404() {
    use wiremock::{matchers::path, Mock, MockServer, ResponseTemplate};
    let server = MockServer::start().await;
    let p1 = std::fs::read_to_string("tests/fixtures/rokinon/entrylist.html").unwrap();
    Mock::given(path("/entrylist.html"))
        .respond_with(ResponseTemplate::new(200).set_body_string(p1))
        .mount(&server)
        .await;
    // entrylist-2 以降は 404 → ページネーション打ち切り
    Mock::given(path("/entrylist-2.html"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let adapter = RokinonAdapter::with_base_url(server.uri());
    let cands = adapter.list_candidates().await.unwrap();
    assert_eq!(cands.len(), 2, "page1 の2件のみ");
}

#[tokio::test]
async fn fetch_and_extract_returns_oshi_from_full_page() {
    use wiremock::{matchers::path, Mock, MockServer, ResponseTemplate};
    let server = MockServer::start().await;
    let page = std::fs::read_to_string("tests/fixtures/rokinon/entry-12966301740.html").unwrap();
    Mock::given(path("/stamedba/entry-12966301740.html"))
        .respond_with(ResponseTemplate::new(200).set_body_string(page))
        .mount(&server)
        .await;

    let adapter = RokinonAdapter::with_base_url(server.uri());
    let candidate = CandidateRef {
        source_external_id: "12966301740".into(),
        source_url: format!("{}/stamedba/entry-12966301740.html", server.uri()),
    };
    let rec = adapter.fetch_and_extract(&candidate).await.unwrap().unwrap();
    assert_eq!(rec.artist_name, "Hiding Places");
    assert_eq!(rec.featured_at, NaiveDate::from_ymd_opt(2026, 5, 1).unwrap());
    assert_eq!(rec.album_name.as_deref(), Some("The Secret To Good Living"));
}
```

> CLAUDE.md: 1 `it` = 1 振る舞いだが、`fetch_and_extract_returns_oshi_from_full_page` は「フルページから推し1件を組み立てる」という1つの統合的振る舞いの確認とし、artist/featured_at/album を同一テストでまとめて検証する（フィールド分割よりフルページ抽出の統合確認が目的）。純粋関数のフィールド単位検証は Task 4 で分割済み。

- [ ] **Step 3: テストが失敗することを確認**

Run: `cargo test --features ssr --lib adapter::rokinon`
Expected: FAIL（list_candidates がまだ RSS を見ている / fetch_and_extract がキャッシュ参照のまま）。

- [ ] **Step 4: 実装 — struct と constructor**

`RokinonAdapter` を変更（`cache` フィールドと `CachedItem` を削除、`max_pages` / `throttle_ms` を追加）:

```rust
pub struct RokinonAdapter {
    client: Client,
    base_url: String,
    max_pages: u32,
    throttle_ms: u64,
}

impl RokinonAdapter {
    pub fn new(max_pages: u32, throttle_ms: u64) -> Self {
        let client = Client::builder()
            .user_agent(USER_AGENT)
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("reqwest client builds");
        Self {
            client,
            base_url: ROKINON_BASE_HOST.to_string(),
            max_pages,
            throttle_ms,
        }
    }

    pub fn with_base_url(base_url: impl Into<String>) -> Self {
        let mut me = Self::new(5, 0);
        me.base_url = base_url.into();
        me
    }

    fn entry_id_from_link(link: &str) -> Option<String> {
        ENTRY_ID_PATTERN.captures(link)?.get(1).map(|m| m.as_str().to_string())
    }
}
```

定数を変更（entrylist はホスト直下のパスを使う）:

```rust
const ROKINON_BASE_HOST: &str = "https://ameblo.jp";
const ROKINON_ENTRYLIST_PATH: &str = "/stamedba/entrylist.html";
```

> `with_base_url(server.uri())` のとき、entrylist は `{base}/stamedba/entrylist.html`、記事 URL はフィクスチャ内の絶対 URL ではなくテストの `CandidateRef.source_url` を直接 fetch する。テストの mock パスと整合させる（`fetch_and_extract_returns_oshi_from_full_page` は `source_url` を `{server}/stamedba/entry-...html` にしてある）。entrylist フィクスチャ内のリンクは絶対 URL（`https://ameblo.jp/...`）だが、`list_candidates` テストは `source_external_id` のみ検証するので問題ない。

`Default` 実装は削除（引数必須になったため）。`Default` を参照している箇所が無いことを `grep -rn "RokinonAdapter::default\|impl Default for RokinonAdapter" src` で確認し、あれば除去。

- [ ] **Step 5: 実装 — list_candidates（entrylist 走査）**

```rust
async fn list_candidates(&self) -> AppResult<Vec<CandidateRef>> {
    use std::collections::HashSet;
    let entry_link_sel = Selector::parse("a[href]")
        .map_err(|e| AppError::Parse(format!("selector: {e}")))?;
    let mut seen = HashSet::new();
    let mut out = Vec::new();

    for page in 1..=self.max_pages {
        let url = if page == 1 {
            format!("{}{}", self.base_url, ROKINON_ENTRYLIST_PATH)
        } else {
            format!("{}/stamedba/entrylist-{}.html", self.base_url, page)
        };
        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() {
            tracing::warn!(status = %resp.status(), %url, "entrylist fetch failed; stopping pagination");
            break;
        }
        let body = resp.text().await?;
        let doc = Html::parse_document(&body);
        let mut found_any = false;
        for el in doc.select(&entry_link_sel) {
            let href = match el.value().attr("href") {
                Some(h) => h,
                None => continue,
            };
            let entry_id = match Self::entry_id_from_link(href) {
                Some(id) => id,
                None => continue,
            };
            if seen.insert(entry_id.clone()) {
                found_any = true;
                out.push(CandidateRef {
                    source_external_id: entry_id,
                    source_url: href.to_string(),
                });
            }
        }
        if !found_any {
            break;
        }
        if self.throttle_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(self.throttle_ms)).await;
        }
    }
    Ok(out)
}
```

- [ ] **Step 6: 実装 — fetch_and_extract（フルページ）**

```rust
async fn fetch_and_extract(
    &self,
    candidate: &CandidateRef,
) -> AppResult<Option<NewRecommendation>> {
    let resp = self.client.get(&candidate.source_url).send().await?;
    if !resp.status().is_success() {
        return Err(AppError::Parse(format!(
            "entry fetch failed: {} {}",
            resp.status(),
            candidate.source_url
        )));
    }
    let page = resp.text().await?;

    let body = match extract_article_body(&page) {
        Some(b) => b,
        None => return Ok(None),
    };
    let featured_at = match detect_oshi(&body.text) {
        Some(d) => d,
        None => return Ok(None),
    };
    let title = extract_entry_title(&page).unwrap_or_default();
    let artist = extract_artist_name(&title);
    let album = extract_album_from_html(&body.html);
    let youtube = extract_youtube_url(&body.html);

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
```

不要になった import を除去（`rss`, `HashMap`, `Mutex`, `CachedItem`、`OSHI_PATTERN` は維持）。`fetch_and_extract_skips_when_not_in_cache` テストは「キャッシュ」概念が無くなるため削除する。

- [ ] **Step 7: テストが通ることを確認**

Run: `cargo test --features ssr --lib adapter::rokinon`
Expected: Task6 の新規3テスト + Task4 の6テスト + 既存純粋関数テストすべて PASS。

- [ ] **Step 8: コミット**

```bash
git add src/server/adapter/rokinon.rs tests/fixtures/rokinon/entrylist.html tests/fixtures/rokinon/entrylist-2.html
git commit -m "feat(rokinon): RSS から entrylist 走査 + フルページ fetch に切り替え"
```

---

## Task 7: 呼び出し側（main / bin/scrape）の配線

**Files:**
- Modify: `src/main.rs:45`
- Modify: `src/bin/scrape.rs`

`RokinonAdapter::new()` がシグネチャ変更（`new(max_pages, throttle_ms)`）したため呼び出し側を更新する。

- [ ] **Step 1: main.rs を更新**

`src/main.rs` の `RokinonAdapter::new()` 呼び出し（45行目付近）を:

```rust
    let rokinon_source: Arc<dyn MediaSource> =
        Arc::new(RokinonAdapter::new(cfg.rokinon_max_pages, cfg.scrape_throttle_ms));
```

（`cfg` がスコープにあることを確認。無ければ `Config::from_env()` の戻り値名に合わせる。）

- [ ] **Step 2: bin/scrape.rs を更新**

`src/bin/scrape.rs` の match 内 `"rokinon" => Arc::new(RokinonAdapter::new())` を:

```rust
        "rokinon" => Arc::new(RokinonAdapter::new(cfg.rokinon_max_pages, cfg.scrape_throttle_ms)),
```

- [ ] **Step 3: ビルドが通ることを確認**

Run: `cargo build --features ssr`
Expected: エラーなし。

- [ ] **Step 4: コミット**

```bash
git add src/main.rs src/bin/scrape.rs
git commit -m "feat: RokinonAdapter に max_pages / throttle を配線"
```

---

## Task 8: README 更新 + 全体検証

**Files:**
- Modify: `README.md`

- [ ] **Step 1: README に環境変数を追記**

`README.md` の環境変数表（`PITCHFORK_MAX_PAGES` などが載っている箇所）に `ROKINON_MAX_PAGES`（デフォルト 5。rokinon が走査する entrylist のページ数）を追加する。記載が無ければ既存の環境変数記述スタイルに合わせて追記。

- [ ] **Step 2: 全テスト実行**

Run: `cargo test --features ssr`
Expected: 全 PASS。

- [ ] **Step 3: clippy / build 確認**

Run: `cargo clippy --features ssr -- -D warnings` と `cargo build --features ssr`
Expected: 警告・エラーなし。

- [ ] **Step 4: 実記事の取り込みを手動検証（任意だが推奨）**

ローカル DB で対象記事が取り込まれることを確認:

Run: `DATABASE_URL=sqlite:data/app.db cargo run --features ssr --bin scrape -- --source rokinon`
その後: `sqlite3 data/app.db "SELECT source_external_id, artist_name, album_name, featured_at FROM recommendations WHERE source_external_id = '12966301740';"`
Expected: `12966301740 | Hiding Places | The Secret To Good Living | 2026-05-01` のような行が存在（Spotify 解決に成功すれば）。Spotify 認証情報が無い環境では recommendations には入らないが、`scraped_entries` には記録される（`SELECT * FROM scraped_entries WHERE external_id='12966301740'`）。

> 注: ROKINON_MAX_PAGES のデフォルト5で対象記事（entrylist 4ページ目）に到達する。

- [ ] **Step 5: コミット**

```bash
git add README.md
git commit -m "docs(readme): ROKINON_MAX_PAGES を追記"
```

---

## 完了基準

- 対象記事 `entry-12966301740` がローカルで取り込まれる（または Spotify 未設定時も `scraped_entries` に記録される）。
- `cargo test --features ssr` 全 PASS。
- `cargo clippy --features ssr -- -D warnings` 警告なし。
- 既存の RSS キャッシュ機構が撤去され、rokinon が entrylist 走査 + フルページ抽出になっている。
- パイプラインが fetch 前に既処理をスキップし、bootstrap マイグレーションで初回 re-fetch 嵐が防がれている。
