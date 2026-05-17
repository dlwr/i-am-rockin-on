# Selector — 直近1ヶ月で DB に追加されたアルバムをランダムに 1 枚

- **作成日**: 2026-05-15
- **改訂**:
  - 2026-05-15: 「リリース日」 → 「DB 追加日」 に方針変更
  - 2026-05-15: 機能名を 「Hey DJ」 → 「Selector」 に変更
- **対象**: ホームに「Selector」ボタンを追加し、 直近 30 日以内に DB へ追加されたアルバムからランダムに 1 枚を提示する
- **スコープ**: 単一ボタン + 1 枚カード表示。 履歴管理・フィルタリング・凝った演出は v1 では入れない

## 命名 — 「Selector」

レゲエ・サウンドシステム文化で 「DJ / 選盤者」 を指す語。 機能の役割 (うちの crate から 1 枚選ぶ) と文化的ルーツの両方が一致する。 ZINE / インディー寄りのサイトトーンと相性が良く、 「セレクタ = 選ぶ人」 で意味が推測しやすい。

## 動機

ホームの grid は 「媒体に featured された順」 で並んでいて、 量が増えると 「最近 crate に入った 1 枚」 へのアクセスが薄まる。 Selector ボタンは 「最近うちが拾った中から selector が 1 枚抜く」 体験を提供する。

## 「最近」 の定義 — `created_at` を採用

初版では Spotify の `release_date` を候補にしたが採らない。 理由:

- リリース日基準だと旧譜は永久に拾えない。 ユーザの要望は 「昔のアルバムでも最近 DB に追加されたなら拾いたい」
- `recommendations.created_at` は既存カラム。 DB マイグレーション・Spotify resolver 変更・backfill が全て不要
- 「Selector = 最近 crate に追加した 1 枚を選ぶ」 は実装意図がそのまま機能名と一致して説明不要

「DB 追加日」 は dedup group 単位では **MIN(created_at)** を使う。 同じアルバムが複数 source から入って来た場合、 「うちが最初に知った日」 が 「追加日」 になる。 後から別 source で再 featured されても 「再追加」 ではない。

## アーキテクチャ

### DB

**マイグレーション不要**。 既存の `recommendations.created_at TEXT DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))` をそのまま使う。

### Domain 層

変更なし。 `Recommendation.created_at: chrono::DateTime<chrono::Utc>` は既に存在。 `AlbumCard` に新フィールドは足さない (Selector 専用の domain 型で持つ)。

### Resolver

変更なし。 Spotify resolver は触らない。

### Store

`src/server/store.rs` に新メソッド:

```rust
pub async fn pick_recent_addition(
    &self,
    since: chrono::DateTime<chrono::Utc>,
) -> AppResult<Option<SelectorCard>>
```

`since` は呼び出し側で `Utc::now() - Duration::days(30)` を渡す。

実装は 2-step:

1. **dedup_key を 1 つ選ぶ**: 既存の dedup ロジック (CTE で `spotify_url` または 正規化 `artist+album` を key にする) を流用し、 各 group の `MIN(created_at)` を計算。 `MIN(created_at) >= ?` で window 内に絞り、 `ORDER BY RANDOM() LIMIT 1` で 1 個取る
2. **その group の rows を全件取り、 head 行 (featured_at DESC, source_id ASC で並べた先頭) を `SelectorCard` に組み立てる**。 dedup group 内で各 optional フィールド (album_name, spotify_url, spotify_image_url, youtube_url) は `iter().find_map` で最初に存在する値を採る (現行 `list_recent_albums` と同じルール)
3. group の `MIN(created_at)` を `added_at` として `SelectorCard` (domain) に乗せる。 view 層では既存 `AlbumCard` → `AlbumCardView` と同じく `From` impl で `YYYY-MM-DD` 文字列に整形する

window 内に該当 group が 0 件なら `Ok(None)`。

`SelectorCard` (domain) の型:

```rust
pub struct SelectorCard {
    pub artist_name: String,
    pub album_name: Option<String>,
    pub spotify_url: Option<String>,
    pub spotify_image_url: Option<String>,
    pub youtube_url: Option<String>,
    pub added_at: chrono::DateTime<chrono::Utc>,
}
```

`SourceLink` を持たない。 Selector カードは 1 枚の音楽体験に集中する設計で、 媒体記事リンクは grid 側に任せる。

### Server fn

`src/pages/home.rs`:

```rust
#[server(Selector, "/api")]
pub async fn selector() -> Result<Option<SelectorCardView>, ServerFnError>
```

- `since = Utc::now() - Duration::days(30)` を計算
- `repo.pick_recent_addition(since)` → `Option<SelectorCardView>` に map

### View 型

`SelectorCardView` は `AlbumCardView` と別型。 理由は (1) `sources` を持たない、 (2) `added_at` を `YYYY-MM-DD` 文字列で持つ ( grid の `YYYY-MM` 表記とは粒度が違う) の 2 点で意味が重ならない。

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SelectorCardView {
    pub artist_name: String,
    pub album_name: Option<String>,
    pub spotify_url: Option<String>,
    pub spotify_image_url: Option<String>,
    pub youtube_url: Option<String>,
    pub added_at: String, // YYYY-MM-DD
}
```

### UI

`pages/home.rs` の `Home` コンポーネント:

```
<header>
  <h1>"i am rockin on"</h1>
  <SelectorButton/>     // 新規。 ヘッダ右端
</header>
<SelectorSlot/>          // 新規。 Action 結果。 Action idle 時は DOM レンダーしない
<Suspense>… AlbumGrid …</Suspense>
```

- `SelectorButton`: `Action::new` で server fn を発火。 ラベル `"Selector"`、 font-zine、 既存 Spotify ボタンと同程度のサイズ、 `bg-ink text-paper` で対比強め
- `SelectorSlot`: Action 状態に応じて分岐
  - `idle` → 何もレンダーしない (DOM ごと不在 = grid の視覚回帰スナップショットに影響なし)
  - `pending` → "選んどるよ…" (font-zine italic)
  - `Ok(Some(card))` → `<SelectorCard card=…/>` を表示
  - `Ok(None)` → "直近1ヶ月で追加された一枚はまだないずら"
  - `Err(e)` → `text-err` で error message
- `SelectorCard`: AlbumGrid のカード見た目を踏襲、 単独表示なので 1.5x 程度大きく。 `added_at` (`YYYY-MM-DD`) を右下に小さく
- 「もう一度」 ボタンは `SelectorSlot` 内、 結果カードの下。 Action を re-dispatch。 連続で同じアルバムが返る可能性は許容

## テスト

### Store (sqlx::SqlitePool in-memory)

`pick_recent_addition` の検証は最低 3 件のデータで、 `created_at` を直接 SQL で UPDATE して仕込む:

- A: `created_at = now - 5 days` (window 内)
- B: `created_at = now - 25 days` (window 内)
- C: `created_at = now - 100 days` (window 外)
- D: (同一 dedup_key で 2 行) `MIN(created_at) = now - 100 days` (window 外)。 後発 row が `now - 5 days` でも MIN が古いため除外されることを assert

テスト:
1. `since = now - 30 days` を渡すと、 戻り値の `artist_name` が必ず A か B のもの (10 回試行で両方出ることは確認しない — 単発で window 外でないことだけ assert)
2. window 内が 0 件のケース (全件 C のみ) → `Ok(None)`
3. dedup group の MIN が window 外 → 後発 row が新しくても除外される (上記 D で確認)
4. 同じ dedup group は 1 枚に畳まれて返る (同一 spotify_url で複数 source、 両方 window 内) → 結果は 1 件、 head 行のフィールドが採られる

### UI (Rust unit / 純粋関数)

`SelectorSlot` の状態分岐は Leptos コンポーネントなので Rust 側ユニットテストの対象外。 視覚回帰で実機確認する。

### 視覚回帰

`tests/visual/home.spec.ts` は pixel-diff スナップショットを使わず、 `ul.tilt-cycle` の `grid-template-columns` トークン数 / 要素 count / `aspect-ratio` 等の computed style assertion のみ:

- `SelectorSlot` を Action idle 時に DOM 不在にすれば grid 兄弟要素が増えず、 既存 4 テストは未更新で通る
- ヘッダ部 (h1 + Selector ボタン) は test の対象外。 expected の更新は不要
- 「Selector ボタン押下後」 の状態の新規テストは v1 では追加しない (実機で十分)

## YAGNI で削るもの

- ジャンル / ソース / 国別の絞り込み
- 「同じアルバムが連続で出ないように」 履歴管理 (LocalStorage / Cookie)
- 凝ったアニメーション
- Selector 専用の別ページ / ルート (ホームに inline で出すほうが回遊性が高い)
- 結果の永久リンク化 (`/selector/{id}` のような shareable URL)
- 「Selector ボタン押下後」 の視覚回帰スナップショット (実機で確認すれば十分)
