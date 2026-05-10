# Pitchfork ソース追加 ＋ カードマージ設計

- ステータス: ドラフト
- 作成日: 2026-05-10
- 関連: `docs/superpowers/specs/2026-05-08-music-recommendations-aggregator-design.md`（v1 全体設計）

## 背景

現状、推し記事の収集源は「ロキノンには騙されないぞ」ブログ 1 つだけ。ユーザの希望は「Metacritic で 90 点超えたメディアがある新作」を対象に追加すること。実機調査で Metacritic は detail ページが 403 で個別メディアの点数が取れないことが判明したため、最初の個別メディアとして **Pitchfork** を採用する（Pitchfork スコア 8.0+ ≒ Metacritic 80+ 換算で curation 価値が高い／ JSON-LD で機械的に拾える／ペイウォール無し）。将来的に他メディアを足す素地も同じ `MediaSource` trait で確保する。

## ゴール

1. Pitchfork のアルバムレビューから、直近 90 日で **スコア 8.0 以上**のものを拾い、Spotify 解決して `recommendations` に保存する。
2. 同一アルバムが Rokinon と Pitchfork の両方で取り上げられていた場合、ホーム画面ではカードを 1 枚にマージし、「記事」ボタンの hover で各メディアへのリンクを選べる UI にする。
3. 上記を、既存の `MediaSource` パイプラインを壊さずに実現する。

## 非ゴール（v1 では扱わない）

- アルバム個別の数値スコア表示（"Pitchfork 8.5" のようなバッジ）
- Pitchfork "Best New Music" タグの判定（純粋なスコア閾値で判定）
- Rokinon / Pitchfork 以外のメディア追加
- `recommendations` テーブルを `albums` + `album_features` に分割するスキーマ正規化
- ヘッドレスブラウザでの Metacritic スクレイプ

## 全体アーキテクチャ

`MediaSource` trait に `PitchforkAdapter` を 1 個追加し、既存の `ScrapePipeline` / `RecommendationRepo` / `SpotifyResolver` をそのまま流用する。`main.rs` でアダプタ生成 1 行と scheduler 登録 1 行を追加。表示側だけは "1 source per row" 前提を破る変更が入る。

```
PitchforkAdapter ─┐
                   ├─→ ScrapePipeline ─→ RecommendationRepo ─→ SQLite
RokinonAdapter   ─┘                                           │
                                                              ▼
                                                  Home view (AlbumCard で集約)
```

## Pitchfork アダプタ

### list_candidates

- GET `https://pitchfork.com/reviews/albums/?page=1` ... `?page=N`（N は `Config.pitchfork_max_pages`、既定 3）
- HTML から `/reviews/albums/<slug>/` パターンの URL を正規表現で全部抜く（重複は dedup）
- `source_external_id` はレビュー slug（例: `aldous-harding-train-on-the-island`）
- User-Agent は Chrome ライク（既存の Rokinon と同じく定数で持つ）

### fetch_and_extract

- GET `https://pitchfork.com/reviews/albums/<slug>/`
- `<script type="application/ld+json">` のうち `"@type":"Review"` のブロックをパース。
- 抽出フィールド:
  - score: `Review.reviewRating.ratingValue`（無ければ `score` キーを fallback）。Pitchfork は 0〜10 の 1 桁小数。
  - publish date: `Review.datePublished`（ISO8601）
  - artist: `Review.itemReviewed.byArtist.name`
  - album: `Review.itemReviewed.name`
- フィルタ:
  - `score < pitchfork_score_threshold`（既定 8.0） → `Ok(None)`
  - `today - publish_date > pitchfork_recency_days`（既定 90） → `Ok(None)`
- 通った場合、`NewRecommendation { source_id: "pitchfork", featured_at: publish_date.date(), ... spotify は None }` を返す。
- pipeline の Spotify resolver がアーティスト＋アルバム名で解決する（既存パスをそのまま）。
- 800 ms throttle は既存の pipeline 側で適用される。

### スコア精度の備考

JSON-LD のフィールドが整数に丸まっているケースが観測されたため、実装中に precision を verify する。1 桁小数が取れない場合は DOM の `ScoreCircle` 近傍テキストを fallback パースする。spec 段階では「8.0+ 判定が壊れない精度を保つ」という要件にとどめる。

## DB スキーマ

**変更なし**。`recommendations` テーブルに `source_id="pitchfork"` の行が増えるだけ。`UNIQUE (source_id, source_external_id)` 制約で同一レビューの重複保存を防ぐ。

## カードマージ（重複処理）

### 同一アルバム判定

`dedup_key`：
1. `spotify_url IS NOT NULL` の行 → `spotify_url` をキーにする（同じ Spotify URL = 同じアルバム）
2. `spotify_url IS NULL` の行 → `lower(trim(artist_name)) || '|' || lower(trim(coalesce(album_name,'')))` をキーにする

### list_recommendations サーバ関数

現状は `RecommendationRepo::list_recent(limit)` が `Vec<Recommendation>` を返す。これを `list_recent_albums(limit)` に置き換え、SQLite の `json_group_array` で 1 アルバム = 1 行にまとめる。返り値は新規型 `AlbumCard`。

```sql
WITH grouped AS (
  SELECT
    COALESCE(spotify_url, lower(trim(artist_name)) || '|' || lower(trim(coalesce(album_name,'')))) AS dedup_key,
    artist_name,
    album_name,
    spotify_url,
    spotify_image_url,
    youtube_url,
    featured_at,
    source_id,
    source_url
  FROM recommendations
)
SELECT
  dedup_key,
  -- 代表値: 最新 featured_at を持つ行のもの
  -- SQLite の集約上、artist_name / album_name / image 等の代表値選びはアプリ側でやる方がシンプル
  MAX(featured_at) AS latest_featured_at,
  json_group_array(json_object(
    'source_id', source_id,
    'source_url', source_url,
    'featured_at', featured_at
  )) AS sources_json,
  -- 代表行用に group の min(rowid) で 1 つ拾う
  ...
FROM grouped
GROUP BY dedup_key
ORDER BY latest_featured_at DESC
LIMIT ?
```

代表値（artist / album / image / youtube_url）の選び方は「同 dedup_key 内で `featured_at DESC, source_id ASC` で最初の行」を採用する。SQLite 単発クエリでこれを実装するのが煩雑なら、2 段階クエリ（dedup_key 一覧を取る → 各 key の代表行を取る）でも良い。実装方針は plan 段階で決める。

### 新規型

```rust
pub struct AlbumCard {
    pub artist_name: String,
    pub album_name: Option<String>,
    pub spotify_url: Option<String>,
    pub spotify_image_url: Option<String>,
    pub youtube_url: Option<String>,
    pub featured_at: NaiveDate,           // 最新ソースの featured_at
    pub sources: Vec<SourceLink>,          // featured_at DESC で並ぶ
}

pub struct SourceLink {
    pub source_id: String,        // "rokinon" / "pitchfork"
    pub source_url: String,
    pub featured_at: NaiveDate,
}
```

### Home ビューの変更

`src/pages/home.rs`:

- `Recommendation` 駆動を `AlbumCard` 駆動に書き換え。
- カード上の「記事」ボタン:
  - `sources.len() == 1` → 今まで通り単一リンク（変更なし）。
  - `sources.len() >= 2` → ボタン要素を `<details>` ベースに切り替え（クリックで開く／hover でも開くように `summary:hover + .menu` の Tailwind ユーティリティ）。menu 内に各 `SourceLink` を「ロキノン」「Pitchfork」のラベルで列挙。
- ラベル写像は `source_id → 表示名` の小さい関数 1 個で持つ（`"rokinon" => "ロキノン"`、`"pitchfork" => "Pitchfork"`）。
- アクセシビリティ: キーボード操作で開閉できるよう `<details>` を採用。`prefers-reduced-motion` 下では transition 無効化。
- `placeholder` ・ `tilt-cycle` などの既存スタイルは AlbumCard でもそのまま動くように HTML 構造を保つ。

## Config 追加

`src/server/config.rs` に以下を追加（既存 `Config` struct）:

```rust
pub pitchfork_score_threshold: f32,   // 既定 8.0
pub pitchfork_recency_days: i64,      // 既定 90
pub pitchfork_max_pages: u32,         // 既定 3
```

env からの読み込みは既存パターンに準拠。`PITCHFORK_SCORE_THRESHOLD` などの `env!`/`std::env::var` 経路。

## Scheduler

`src/server/scheduler.rs` で Rokinon に加えて Pitchfork パイプラインを 2 個目の cron ジョブとして登録。実行時刻は重複しないようにずらす（例: Rokinon 00:30 / Pitchfork 12:30）。両者とも独立した `ScrapePipeline` インスタンス。

## main.rs

```rust
let rokinon = Arc::new(RokinonAdapter::new());
let pitchfork = Arc::new(PitchforkAdapter::new(config.pitchfork_score_threshold, config.pitchfork_recency_days, config.pitchfork_max_pages));
// scheduler.register(rokinon_pipeline);
// scheduler.register(pitchfork_pipeline);
```

詳細はパイプラインの所有関係次第（plan で詰める）。

## テスト計画

### unit

- `parse_review_jsonld` がスコア / 公開日 / アーティスト / アルバム を抜けること（fixture: 実 Pitchfork レビューの保存版 HTML）
- `parse_review_jsonld` が score < 8.0 の row を `Ok(None)` で skip すること
- `parse_review_jsonld` が古い publish_date を skip すること（モック clock を `chrono::Utc::now()` 抽象化）
- `extract_review_urls` が index ページから `/reviews/albums/<slug>/` をすべて抜くこと（fixture: index ページの保存版）

### adapter integration（wiremock）

- `list_candidates` が Pitchfork 風 index ページを fetch して候補を返すこと
- `fetch_and_extract` が個別レビュー HTML を fetch して NewRecommendation を返すこと

### store integration

- `list_recent_albums`:
  - 同 spotify_url の Rokinon + Pitchfork 行が 1 件にマージされ `sources.len() == 2` になること
  - spotify_url が NULL の Rokinon 行と (artist, album) 一致の Pitchfork 行も 1 件にマージされること
  - 異なるアルバムは独立して並ぶこと
  - `featured_at DESC` の 3 件以上での順序確認（CLAUDE.md ルール）

### view（rendering smoke）

- AlbumCard 1 ソースの場合に「記事」ボタンが直接リンクとしてレンダリングされること
- AlbumCard 複数ソースの場合に `<details>` based menu がレンダリングされること（hover 挙動は visual regression task で検証）

## マイグレーション・運用上の影響

- DB スキーマ変更なし → migrations 追加無し
- 既存 Rokinon の挙動は変更なし
- `list_recent` の戻り値型が変わるため、現 SF のシグネチャ変更は破壊的（呼び出し元は home view 1 箇所のみ）
- 本番デプロイ後の最初の Pitchfork 走行で、過去 90 日分のレビューがまとめて入る点に注意（数十件程度の見込み）

## オープンな実装判断（plan で詰める）

- SQL 1 発で代表行を選ぶか、2 段クエリにするか
- スコア精度を JSON-LD だけで賄えるか DOM fallback が要るか
- `<details>` の hover 開閉スタイルの最終形（特に ZINE 風スキューと干渉しないか）
- pipeline の所有関係（`Arc<dyn MediaSource>` を `Vec` で持って scheduler が回す形か、個別パイプラインインスタンスを 2 つ持つか）
