# funkstudy ソース設計

taizooo（x.com/taizooo）の `#yetanotherfunkstudy` 付きポストを拾い、ぶら下がる返信中の Spotify アルバム URL から推薦を取り込む新ソース。

## 背景・確定仕様

- 対象は `from:taizooo` の `#yetanotherfunkstudy` を含む「本体ポスト」（画像付き）。
- 本体ポストにぶら下がる taizooo 自身の返信に **Spotify アルバム URL**（`open.spotify.com/album/<id>`）がある。
- アルバム URL を Spotify API で直接解決するため、rokinon/pitchfork のような「アーティスト名＋アルバム名の検索」より精度が高い。
- 取得範囲は直近〜約30日のバックフィル + 以降は日次差分。
- X には無認証で安定的に拾える経路が無い（syndication endpoint は 2019〜2020 で凍結、Nitter は全滅）ことを実測で確認済み。よってサードパーティ取得 API を使う。

## アーキテクチャ

既存の `MediaSource` trait ベース拡張に乗せる。責務を分離してテスト容易性を確保する。

```
src/server/adapter/funkstudy.rs   新規 MediaSource アダプタ（Twitter 取得に専念）
src/server/resolver/spotify.rs    resolve_by_album_id() を追加
src/server/scrape.rs              spotify_url 事前セット時は ID 解決へ分岐
src/server/config.rs              env 読み込み追加
src/server/adapter/mod.rs         pub mod funkstudy
src/main.rs                       Pipeline 生成 + cron 登録 + 初回スクレイプ
src/bin/scrape.rs                 --source funkstudy 対応
```

責務:

- **`FunkstudyAdapter`** … サードパーティ Twitter API のことだけ知る。Spotify API は触らない。wiremock で Twitter API のみモックして単体テストできる。
- **Spotify 解決** … 既存どおり resolver / pipeline 層に閉じる。アダプタは「どのアルバム URL か」までを返し、メタデータ確定はピプラインに委ねる。

## データフロー（アプローチA: ルート検索 → スレッド取得の2段）

```
list_candidates():
  advanced_search に query = `from:taizooo #yetanotherfunkstudy since:<BACKFILL_DAYS 日前>` を投げ、
  ヒットした本体ポストを列挙
  → CandidateRef {
       source_external_id: <root tweet id>,
       source_url: "https://x.com/taizooo/status/<id>",
     }

fetch_and_extract(candidate):
  candidate の会話スレッド（replies）を取得
  → taizooo 自身の返信から open.spotify.com/album/<id> を抽出
  → 見つかった場合:
       NewRecommendation {
         source_id: "funkstudy",
         source_external_id: <root tweet id>,
         source_url: <root tweet url>,
         featured_at: <root tweet の JST 日付>,
         artist_name: "",          // ピプラインが ID 解決で埋める
         album_name: None,
         track_name: None,
         spotify_url: Some(<album URL>),  // 「ID 解決して」の合図
         spotify_image_url: None,
         youtube_url: None,
       }
  → 見つからない場合: Err(transient) を返す（§後追い返信の扱い）

ピプライン (scrape.rs process_candidate):
  if new_rec.spotify_url が open.spotify.com/album/<id> 形式:
       resolver.resolve_by_album_id(id)
       → artist_name / album_name / 正規 spotify_url / spotify_image_url を確定
       → 解決失敗（None/Err）は従来どおり skip（mark せず次回リトライ）
  else（rokinon/pitchfork）:
       従来の resolver.resolve(artist, album) 名前検索（挙動不変）
```

ピプライン変更は分岐1つの追加のみ。`spotify_url=None` を返す既存ソースの挙動は変わらない。

## resolver 追加メソッド

```
SpotifyResolver::resolve_by_album_id(&self, album_id: &str)
    -> AppResult<Option<SpotifyAlbumMeta>>
  GET https://api.spotify.com/v1/albums/{id}
  → SpotifyAlbumMeta { url, image_url, artist_name, album_name }
```

- 既存の access_token キャッシュ・`with_endpoints`（wiremock 用）を再利用。
- album_id は `open.spotify.com/album/<id>?si=...` から base62 id を取り出すヘルパーで抽出する（クエリ・末尾スラッシュを除去）。

## 後追い返信の扱い（確定: Err リトライ）

taizooo が画像本体を先に投稿し、Spotify 返信を後から足すパターンがある。スレッド取得時に album URL が無い瞬間が存在する。ピプラインは `fetch_and_extract` が `None` を返すと永久に `mark_scraped` して再取得しない。

- 確定方針: album URL が見つからないときは `None` ではなく **`Err`（transient）を返す**。ピプラインは Err を warn + skip 扱いにし、`mark_scraped` しないため次回スクレイプでリトライされる。
- `list_candidates` の検索窓が直近 `BACKFILL_DAYS` 日なので、永遠に解決しない本体も自然に窓から外れて候補から消える（無限リトライにはならない）。
- これにより「本体先・返信後追い」のパターンを取りこぼさない。

## サードパーティ API: twitterapi.io

- 必要な機能: advanced_search（`from:` / hashtag / `since:` 対応）と tweet replies（スレッド取得）の両方を備える。
- 認証: `X-API-Key` ヘッダ。従量課金で月数十円規模を想定。
- base URL を config 化（SpotifyResolver の `with_endpoints` と同じ手法）→ wiremock でテスト可能。プロバイダ差し替え余地を残す。

## config / env

```
FUNKSTUDY_API_KEY        必須   twitterapi.io の API キー
FUNKSTUDY_ENABLED        任意   既定 true。キー未設定なら自動で off にしてソース登録をスキップ
FUNKSTUDY_SCREEN_NAME    任意   既定 "taizooo"
FUNKSTUDY_BACKFILL_DAYS  任意   既定 30
```

- Spotify 認証情報は既存（`SPOTIFY_CLIENT_ID` / `SPOTIFY_CLIENT_SECRET`）を流用。
- 本番（Fly.io）は `flyctl secrets set FUNKSTUDY_API_KEY=...`。
- `.env.example` と README のソース一覧・env 一覧を更新する。

## スケジュール / 命名

- cron: 日次1回。rokinon（UTC 19:00 / JST 04:00）、pitchfork（UTC 07:00 / JST 16:00）とずらした時刻に登録する。
- `source_id = "funkstudy"`。固有名で、ジェネリックな "twitter"/"x" は避ける。CLI は `cargo run --features ssr --bin scrape -- --source funkstudy`。
- 初回起動時、`run_initial_scrape_if_empty` に "funkstudy" を渡して空なら初回バックフィル。

## テスト方針

- `FunkstudyAdapter`: wiremock で advanced_search と replies をモックし、
  - 本体ポスト列挙（external_id / url）
  - 返信からの album URL 抽出
  - album URL が無いとき Err を返すこと
  を検証。fixture は `tests/fixtures/funkstudy/`。
- `resolve_by_album_id`: wiremock で `/v1/albums/{id}` をモックし、メタデータ取得を検証。
- album_id 抽出ヘルパー: クエリ付き・末尾スラッシュ付き URL の純粋関数テスト。
- ピプライン分岐: 既存 `FakeSource` パターンで、spotify_url 事前セット時に ID 解決ルートへ入ることを検証。

## スコープ外（YAGNI）

- track / playlist URL への対応（現状アルバムのみ。将来 URL の type で分岐する余地は残す）。
- taizooo 以外のアカウント・複数アカウント対応。
- ピクセル単位の視覚回帰。
```
