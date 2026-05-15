# Hey DJ — 直近1ヶ月リリースのアルバムをランダムに 1 枚

- **作成日**: 2026-05-15
- **対象**: ホームに「Hey DJ」ボタンを追加し、 直近 30 日以内に Spotify 上でリリースされたアルバムからランダムに 1 枚を提示する
- **スコープ**: 単一ボタン + 1 枚カード表示。 履歴管理・フィルタリング・凝った演出は v1 では入れない

## 動機

ホームの grid は 「媒体に featured された順」 で並んでおり、 新譜と旧譜が混在する。 「とりあえず今月の新譜を 1 枚聴きたい」 という用途に直接答える導線が無い。 Hey DJ ボタンはその穴を埋める。

## 「リリース」の定義 — Spotify `release_date` を採用

`featured_at` (媒体が記事を出した日付) を proxy にする選択肢もあったが採らない。 理由:

- ユーザの文言 (「リリースされたアルバム」) は実リリース日を指す
- ロキノンは月別 「推し」 記事の RSS なので、 旧譜が紛れる頻度が高い。 proxy だと偽陽性
- `release_date` を一度持てば 「2026 ベスト」 「リリース年表示」 など他機能で再利用可能
- 実装コストは moderate (DB 列 + Spotify field + 既存 backfill)。 hobby 規模で許容範囲

## アーキテクチャ

### DB

新マイグレーション `migrations/20260515000001_add_release_date.sql`:

```sql
ALTER TABLE recommendations ADD COLUMN release_date TEXT;
CREATE INDEX idx_recommendations_release_date ON recommendations (release_date DESC);
```

- 型は `TEXT NULL`。 形式は ISO-8601 `YYYY-MM-DD`
- Spotify の `release_date_precision` に応じた正規化:
  - `day` → そのまま
  - `month` → `YYYY-MM-01`
  - `year` → `YYYY-01-01`
- index は `release_date DESC`。 `pick_recent_release` の `WHERE release_date >= ?` で効く

### Domain 層

`src/domain/recommendation.rs`:
- `Recommendation` と `NewRecommendation` に `release_date: Option<NaiveDate>` を追加

`src/domain/album_card.rs`:
- `AlbumCard` に `release_date: Option<NaiveDate>` を追加
- 複数 source merge 時の coalesce: `raw.iter().find_map(|r| r.release_date)` で head→tail 順に最初に存在する値を採る (head が最新 featured_at の source。 後発 source が release_date 解決済みなら head に乗る)

### Resolver

`src/server/resolver/spotify.rs`:
- `AlbumItem` に `release_date: String` と `release_date_precision: String` を deserialize 追加 (Spotify API のレスポンスに必ず含まれる)
- `SpotifyMatch` に `release_date: Option<NaiveDate>` を追加
- ヘルパ純粋関数 `parse_release_date(date: &str, precision: &str) -> Option<NaiveDate>` を分離:
  - 値が空 / パース失敗 / 未知の precision → `None`
  - day / month / year の正規化を担当
  - ユニットテストはこの関数に集中

### Pipeline / Store

`src/server/store.rs`:
- `upsert`: `release_date` は **Spotify 由来として manual_override 時に保護する** (= 既存の `spotify_url` / `spotify_image_url` と同列)。 通常 row では Spotify resolver の最新値で update。 理由: manual_override が立つのは Spotify resolver の誤マッチを手動で正したケースで、 その時 release_date を勝手に上書きすると一貫性が崩れる
- `list_recent_albums` の `json_group_array` に `release_date` を含める。 RawRow にもフィールド追加
- 新メソッド:

```rust
pub async fn pick_recent_release(
    &self,
    since: NaiveDate,
) -> AppResult<Option<AlbumCard>>
```

- SQL は `recommendations` から `release_date >= ? AND release_date IS NOT NULL` で 1 件 `ORDER BY RANDOM() LIMIT 1` を取り、 同じ dedup ロジックで AlbumCard を組み立てる
  - 同一アルバムが複数 source で入っているケースは 「先に当たった 1 行から spotify_url で同一 group を集める」 で 1 枚に畳む
  - シンプル化のため: 第一段で `id` を 1 個選び、 第二段で `list_recent_albums` と同じ JOIN/grouping を「その id を含む group」 で実行する 2-step クエリにする
- `since` の計算は呼び出し側で `today - chrono::Duration::days(30)` を渡す

### Server fn

`src/pages/home.rs`:

```rust
#[server(HeyDj, "/api")]
pub async fn hey_dj() -> Result<Option<HeyDjCardView>, ServerFnError>
```

- `today = chrono::Utc::now().date_naive()`, `since = today - Duration::days(30)`
- `repo.pick_recent_release(since)` を呼んで `Option<HeyDjCardView>` にマップして返す

### View 型

`HeyDjCardView` は `AlbumCardView` とは別型にする。 理由: `featured_at` は `YYYY-MM` 文字列、 `release_date` はフル日付 `YYYY-MM-DD` で表示意図が違う。 共用すると 「どちらの日付を出すか」 のロジックがコンポーネントに漏れる。

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HeyDjCardView {
    pub artist_name: String,
    pub album_name: Option<String>,
    pub spotify_url: Option<String>,
    pub spotify_image_url: Option<String>,
    pub youtube_url: Option<String>,
    pub release_date: String, // YYYY-MM-DD
}
```

`SourceLinkView` を持たないのは意図的。 Hey DJ カードは 「DJ が今かけた 1 曲」 体験で、 記事リンク群への導線は不要。 ホームの grid 側に残す。

### UI

`pages/home.rs` の `Home` コンポーネント:

```
<header>
  <h1>"i am rockin on"</h1>
  <HeyDjButton/>     // 新規。 ヘッダ右端に配置 (flex)
</header>
<HeyDjSlot/>          // 新規。 Action 結果を展開。 初期は非表示
<Suspense>… AlbumGrid …</Suspense>
```

- `HeyDjButton`: `Action::new` で server fn を発火。 ボタン文字列は「Hey DJ」 (font-zine, 既存の Spotify ボタンと同サイズ感、 `bg-ink text-paper` の対比強めにして 「DJ 機能」 を視覚的に分離)
- `HeyDjSlot`: Action の状態に応じて分岐
  - `pending` → 「DJ が選んどるよ…」 (font-zine italic)
  - `Ok(Some(card))` → `<HeyDjCard card=…/>` を表示
  - `Ok(None)` → 「直近1ヶ月の新譜はまだないずら」
  - `Err(e)` → 既存 grid 同様 `text-err` で error message
- `HeyDjCard`: AlbumGrid のカード見た目を踏襲しつつ、 単独表示なので 1.5x 程度大きく。 release_date は `2026-05-08` 形式で右下に
- 「もう一度」 ボタンは `HeyDjSlot` 内、 結果カードの下に置く。 同じ Action を re-dispatch。 同じアルバムが連続で返る可能性は許容 (hobby scale)
- HeyDjSlot は Action が `Idle` の時は `display: none` 等で完全非表示にして、 grid のレイアウトに影響を出さない

## Backfill

新カラムは既存行に対して NULL で入る。 埋まり方:

- **Pitchfork**: 毎朝 7am のスケジュール scrape で `PITCHFORK_RECENCY_DAYS=90` 範囲を再列挙する → 直近 90 日分の既存 row は 24 時間以内に `release_date` が埋まる
- **ロキノン**: 月別記事の RSS なので、 旧月の記事は再列挙されない → 旧月の row の `release_date` は永久に NULL のまま

Hey DJ は 「直近 30 日リリース」 を拾うため、 ロキノン旧月の NULL row は元々対象外で実害なし。 即時バックフィルが必要なら手動 `mise run scrape` を 1 回。

## テスト

### Unit (純粋関数)

`parse_release_date`:
- `("2026-04-15", "day")` → `Some(2026-04-15)`
- `("2026-04", "month")` → `Some(2026-04-01)`
- `("2026", "year")` → `Some(2026-01-01)`
- `("invalid", "day")` → `None`
- `("2026-04-15", "unknown")` → `None`
- `("", "day")` → `None`

### Resolver (wiremock)

- Spotify search response が `release_date: "2026-04-15"` / `release_date_precision: "day"` を返すケース → `SpotifyMatch.release_date == Some(2026-04-15)` を確認

### Store (sqlx::SqlitePool in-memory)

`pick_recent_release` の検証は最低 3 件のデータで:
- A: `release_date = today - 5 days` (window 内)
- B: `release_date = today - 25 days` (window 内)
- C: `release_date = today - 100 days` (window 外)
- D: `release_date = NULL` (除外)

テスト:
1. `since = today - 30 days` を渡すと、 戻り値の id が必ず A か B のどちらか (10 回試行して両方が出ることまでは確認しない — 単発で範囲外でないことだけ assert)
2. window 内が 0 件のケース → `Ok(None)`
3. NULL 行は決して返らない (D のみのケースで `None`)

upsert 系既存テストは `release_date` フィールド追加に伴うシグネチャ変更で fixup する。

### 視覚回帰

`tests/visual/` の home スクリーンショットは現状 grid layout を 3 viewport で検証している。 ヘッダに Hey DJ ボタンを足すと、 既存 expected の上端付近に差分が出る:

- `HeyDjSlot` を初期非表示 (Action idle 時に DOM レンダーしない) にして、 grid 部分の expected は変わらないようにする
- ヘッダ部の expected は再生成する (`npx playwright test --update-snapshots`)
- 実装 PR の中でスナップショット更新を 1 コミットに分離する

## YAGNI で削るもの

- ジャンル / ソース / 国別の絞り込み
- 「同じアルバムが連続で出ないように」 履歴管理 (LocalStorage / Cookie)
- 凝ったアニメーション (DJ がレコードに針を落とす演出など)
- 手動 backfill コマンド (`scrape` で十分)
- Hey DJ 専用の別ページ / ルート (ホームに inline で出すほうが回遊性が高い)
- 結果の永久リンク化 (`/hey-dj/{id}` のような shareable URL)
