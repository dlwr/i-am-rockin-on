# rokinon entrylist バックフィル設計

日付: 2026-05-25

## 背景 / 問題

ロキノン (`RokinonAdapter`) は ameblo (`stamedba`) ブログから「推し」記事を取り込んでいるが、`https://ameblo.jp/stamedba/entry-12966301740.html`（`202605推し` = 2026年5月推し、取り込み対象）が取りこぼされている。

調査で確定した根本原因:

- `list_candidates()` は ameblo の RSS (`/rss20.xml`) **のみ**をソースにしている。
- RSS は**最新10件しか公開しない**。現在の RSS は全て `12967xxx` 系 ID で、対象 `12966301740` は既にフィードから押し出されている。
- rokinon の scrape は**1日1回 (cron `0 0 19 * * *`)** しか走らない。
- stamedba は1日に10件以上投稿し得る（entrylist 1ページ20件、複数ページに渡る投稿ペース）。

結果として「RSS 最新10件 × 日次スクレイプ × 速い投稿ペース」の掛け算で、scrape 間隔中に10件を超えて流れた推し記事は RSS の窓から出て**二度と候補にならない**。これはコードのバグではなく設計上の穴。対象記事は現状のどの経路（cron / `bin/scrape` CLI）でも取り込めない。

## ゴール

1. 対象記事を含む、RSS の窓から出た過去記事を取り込めるようにする（救済）。
2. 同種の取りこぼしを構造的に防止する（自己修復）。日次 cron に統合し、別途手動実行に依存しない。
3. entrylist 走査は記事ごとに HTTP fetch が必要なため、既処理分を fetch 前にスキップしてコストを抑える。

## 採用アプローチ: seen-set テーブル + entrylist 走査

記事ごとに「処理済みか」を明示追跡する `scraped_entries` テーブルを導入し、rokinon の候補列挙を RSS から entrylist ページネーションに切り替える。

### 却下した代替案

- **entry ID の高水位カーソル（forward-only cursor）**: 「処理済みの最大 entry ID を記録し、それより大きい ID のみ処理」。シンプルだが、entrylist の表示順は sticky 記事や事後編集で前後する（観測: entrylist page 2 の最小 ID `12961001026` が page 3〜5 のどれよりも古い）。ID 順の前進カーソルでは浮き沈みした記事を取りこぼす。記事ごとの明示追跡の方が堅牢。
- **追跡なしで毎回 N ページ全件 re-fetch**: スキーマ変更不要だが、毎日 N×20 件を HTTP fetch + Spotify 再解決するのが無駄。却下。

## アーキテクチャ

### 1. マイグレーション: `scraped_entries`

```sql
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

> bootstrap は `recommendations` に入っている記事（＝過去の推し）のみを seen にする。非推し記事は元々 DB に無いので初回 cron で一度 fetch される（これがバックフィル本体の走査コスト。一度きり）。2回目以降は全エントリが seen になり安価。

### 2. `RecommendationRepo`（store.rs）に追加

```rust
async fn is_scraped(&self, source_id: &str, external_id: &str) -> AppResult<bool>;
async fn mark_scraped(&self, source_id: &str, external_id: &str) -> AppResult<()>; // INSERT OR IGNORE
```

### 3. `RokinonAdapter::list_candidates` — entrylist ページネーション

- RSS 取得・RSS キャッシュ機構 (`CachedItem` / `cache: Mutex<HashMap>`) を撤去。
- `entrylist.html` / `entrylist-2.html` / ... を `ROKINON_MAX_PAGES`（環境変数、デフォルト 5）まで走査。対象記事は page 4 にあるためデフォルト5で到達。
- 各ページから `entry-(\d+)\.html` リンクを抽出して `CandidateRef { source_external_id, source_url }` を作る（重複排除）。
- list ページ取得の間に throttle を挟む（連続 GET でブロックされないよう、`Config.scrape_throttle_ms` を流用）。
- pitchfork の `max_pages` ページネーションパターンに倣う（空ページ / 非200 で打ち切り）。

### 4. `RokinonAdapter::fetch_and_extract` — フル記事ページ抽出

- キャッシュ参照をやめ、`candidate.source_url` を HTTP fetch。
- **`.articleText` コンテナにスコープを絞って**抽出する。フル記事ページには本文の前に `<h2 class="skinDescriptionArea">`（ブログ説明文）があり、現行の「最初の h2」ロジックだと誤取得する。YouTube 埋め込みもサイドバー等に複数あるため本文スコープ必須。
  - `detect_oshi`: `.articleText` のテキストに対して実行。
  - `extract_album_from_html`: `.articleText` 内の最初の h2 / `.ogpCard_title`。
  - `extract_youtube_url`: `.articleText` 内の最初の YouTube リンク。
- `detect_oshi` が None（非推し）なら `Ok(None)`。

### 5. パイプライン（scrape.rs）に fetch 前スキップ

`process_candidate` を以下に変更（**全ソース共通**＝pitchfork にも適用）:

1. `repo.is_scraped(source_id, external_id)` が true → `ProcessResult::Skipped`（**fetch しない**）。
2. `fetch_and_extract`:
   - `Ok(None)`（非推し / 抽出対象外）→ `mark_scraped` して `Skipped`。
   - `Ok(Some(rec))` → Spotify 解決へ。
3. Spotify 解決:
   - matched → `upsert` → `mark_scraped` → `Inserted`/`Updated`。
   - `Ok(None)`（マッチ無し）/ `Err`（transient）→ `Skipped`、**mark しない**（次回リトライ温存）。
4. HTTP fetch error（transient）→ 既存の error ハンドリング、**mark しない**。

> **pitchfork への影響（承認済み）**: 共通適用により pitchfork も既処理レビューを再 fetch しなくなる。レビューは事後編集されない前提なので帯域削減で有益。transient 失敗時は mark しないため Spotify リトライは温存される。レビューの事後編集（スコア訂正等）には追従できなくなるが受容する。

## データフロー

```
cron (日次) → pipeline.run()
  → rokinon.list_candidates()        # entrylist N ページ走査 → entry URL 列挙
  → 各 candidate:
      is_scraped? ── true ──→ skip（fetch なし）
            │ false
            ▼
      fetch_and_extract()            # 記事ページ HTTP fetch → .articleText 抽出
            │
       非推し → mark_scraped → skip
            │ 推し
            ▼
      spotify.resolve()
        matched → upsert → mark_scraped
        no-match/err → skip（mark しない＝リトライ）
```

## エラーハンドリング

- entrylist ページ取得失敗 / 非200 → そのページで打ち切り（pitchfork に倣う）、ログ警告。
- 記事ページ fetch 失敗 → 当該候補のみ skip、mark しない（リトライ）。既存の per-candidate error ハンドリングを踏襲。
- `should_warn_zero_items` の 0件警告ロジックは維持。

## テスト戦略（TDD）

現行の `extract_album_from_html` / `extract_youtube_url` テストは RSS description 風の小さい HTML 断片を入力にしている。`.articleText` スコープ化に伴い入力が「フル記事ページ」に変わるため作り直す:

- **ゴールデンフィクスチャ**: 実記事 `entry-12966301740.html`（取得済み）を `tests/fixtures/rokinon/` に保存。
- `detect_oshi` が `.articleText` から `202605推し` → 2026-05-01 を返すこと。
- `extract_album_from_html` がフル記事ページから `skinDescriptionArea` ではなく本文の正しいアルバム名を返すこと（リグレッション）。
- `extract_youtube_url` がサイドバー等ではなく本文の最初の YouTube リンクを返すこと。
- `list_candidates`: entrylist フィクスチャ（複数ページ）から entry URL を列挙、ページネーション打ち切り、重複排除を検証（wiremock）。
- パイプライン: `is_scraped` が true の候補で `fetch_and_extract` が呼ばれないこと、非推し / 成功時に `mark_scraped` され transient 失敗時は mark されないことを検証。
- `repo.is_scraped` / `mark_scraped` の単体テスト。
- マイグレーション bootstrap: 既存 recommendations が `scraped_entries` に登録されることを検証。

CLAUDE.md ルール遵守: アソシエーションのテストは書かない。1 `it`（テスト関数）= 1 振る舞い。順序検証は3件以上。

## 影響を受けるファイル

- `migrations/` — `scraped_entries` 追加 + bootstrap（新規）
- `src/server/store.rs` — `is_scraped` / `mark_scraped`
- `src/server/adapter/rokinon.rs` — list_candidates / fetch_and_extract 書き換え、RSS キャッシュ撤去
- `src/server/scrape.rs` — `process_candidate` に fetch 前スキップ
- `src/server/config.rs` — `ROKINON_MAX_PAGES`（必要なら）
- `tests/fixtures/rokinon/` — ゴールデンフィクスチャ追加
- `README.md` — `ROKINON_MAX_PAGES` 環境変数を追記（外部観測できる変更）

## YAGNI / 非対象

- 手動 URL 指定 CLI は作らない（日次 cron 統合で自動救済されるため不要）。
- 視覚的な変更なし。
- scrape 頻度の変更はしない（entrylist 走査で窓問題自体が解消するため）。
