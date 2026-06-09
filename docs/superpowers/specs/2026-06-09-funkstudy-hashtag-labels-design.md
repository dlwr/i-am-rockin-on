# funkstudy ハッシュタグ別ラベル ＋ #FUNKStudy 取り込み 設計

taizooo の funkstudy ソースは現在 `#yetanotherfunkstudy` / `#yetanotherbachstudy` を OR 検索で拾い、全部 `source_id = "funkstudy"` に畳んでいる。表示も単一ラベル `yetanother(funk|bach)study`。これを **投稿ごとに実際のハッシュタグ名で表示**できるようにし、さらに **`#FUNKStudy`** も取り込み対象に加える。

## 確定仕様（ユーザー合意済み）

- 表示は **複数ソースが付いたカードの「記事」ドロップダウン内のラベルのみ**を直す。単一ソースカードに常時バッジを出すような UX 変更はしない（現状の表示箇所のまま）。
- ラベルは **`#` 付き**：`#yetanotherfunkstudy` / `#yetanotherbachstudy` / `#FUNKStudy`。
- `#FUNKStudy` の投稿構造は yetanother 系と **同じ**（本体ポスト＋自己リプに Spotify アルバム URL）。よって既存の抽出ロジックをそのまま流用し、検索ハッシュタグに足すだけ。

## アーキテクチャ方針

保存行の `source_id` を投稿ごとに「実際に使われたハッシュタグ（設定上の正準表記）」にする。**スキーマ変更なし**。

根拠（既存コードで確認済み）:

- `scrape.rs` の dedup / 処理済み追跡（`is_scraped` / `mark_scraped`）は **アダプタの `id()`（固定 `"funkstudy"`）**を使う。保存行は `NewRecommendation.source_id` を使う（`upsert`）。両者は**完全に分離**しており、行の `source_id` を変えても scrape 追跡は `funkstudy` のまま安定する。
- `SourceLink.source_id` は `recommendations.source_id` を**そのまま**読む（`store.rs`）。集約クエリに `WHERE source_id = 'funkstudy'` 等の絞り込みは**無い**。よって行の source_id を per-hashtag にしても集約・dedup（アルバム単位）は壊れない。

```
src/server/adapter/source.rs    CandidateRef に source_id_override: Option<String> 追加
src/server/adapter/funkstudy.rs ハッシュタグ検出 + FUNKStudy 既定追加 + override→source_id
src/server/config.rs            既定ハッシュタグに "FUNKStudy" 追加
src/pages/home.rs               source_label を invert 方式（&str→String）に変更
src/server/adapter/pitchfork.rs CandidateRef リテラルに source_id_override: None 追加
src/server/adapter/rokinon.rs   同上
src/server/scrape.rs            テスト内 CandidateRef リテラルに None 追加
README.md                       FUNKSTUDY_HASHTAGS 既定値の記載更新
```

## 1. ハッシュタグ検出（`list_candidates` で）

検出点は **`list_candidates`**。検索結果の各 tweet は「そのハッシュタグで `advanced_search` にマッチした本体ポスト」なので、`text` に該当タグが**構造上必ず含まれる**（データ保証）。

- `fetch_and_extract` 側で返信から検出する案は退ける。返信レスポンスに本体ポストが含まれるのは**実 API の観測挙動**でしかなく（コード内コメントが根拠）、外れると旧結合ラベルへ**無言フォールバック**する。これは滅多に開かないドロップダウン内なので気付けない＝サイレント契約ズレになる。
- `CandidateRef` に `source_id_override: Option<String>` を追加。funkstudy は検出したタグを載せ、他アダプタ（rokinon / pitchfork / scrape テストの Fake）は `None`。
- `fetch_and_extract` は `candidate.source_id_override` を `NewRecommendation.source_id` に使い、`None` なら `"funkstudy"` にフォールバック。

検出は純粋関数に切り出す:

```
fn match_configured_hashtag(text: &str, configured: &[String]) -> Option<String>
  configured を順に走査し、text に `#<tag>` が
  case-insensitive かつ `#` アンカー一致で含まれる最初の tag（configured の正準表記）を返す。
```

- `#` アンカーにより `#funkstudy` が `#yetanotherfunkstudy` に誤マッチしない（直前が `#` でなく `r`）。
- 複数タグを含む稀な投稿は configured 順（funk → bach → FUNKStudy）で先頭を採用＝決定的。
- 戻り値は **configured 側の表記**（text の casing でなく）。よって `#funkstudy` でも `#FUNKStudy` でも source_id は `"FUNKStudy"` に正規化される。

（堅牢化の余地: 検索結果 tweet は `entities.hashtags`（構造化）も持つはず。text が極端に整形/切詰めされる環境では構造化フィールド優先が無難。ただし taizooo の本体ポストは text がほぼハッシュタグそのもの〔fixture: `"#yetanotherfunkstudy"`〕で切詰めリスクが無いため、初期実装は text 走査とする。実物の entities 形状を確認できたら structured 優先へ上げる。）

## 2. #FUNKStudy 取り込み

- 既定ハッシュタグ集合に `"FUNKStudy"` を追加。追加箇所は2つ:
  - `FunkstudyAdapter::new()` の `hashtags` 既定
  - `config.rs` の `parse_funkstudy_hashtags()` 既定（env 未設定/空のときのフォールバック）
- OR 検索は `from:taizooo (#yetanotherfunkstudy OR #yetanotherbachstudy OR #FUNKStudy) since:<日付>` になる。`build_query` の `many` アームが3タグを処理する。
- `"FUNKStudy"`（ハッシュタグ／行 source_id）と `"funkstudy"`（アダプタ id・scrape 追跡キー）は **case-sensitive に別物で衝突しない**。

## 3. 表示ラベル（`source_label`、ドロップダウン内のみ）

`src/pages/home.rs` の `source_label` を **invert 方式**にする。戻り値を `&str` → `String` に変更。

```
fn source_label(source_id: &str) -> String {
    match source_id {
        "rokinon"   => "ロキノンには騙されないぞ".into(),
        "pitchfork" => "Pitchfork".into(),
        "funkstudy" => "yetanother(funk|bach)study".into(), // デプロイ前の旧データ向け残置
        other       => format!("#{other}"),                 // funkstudy 系ハッシュタグ（env 追加分含む）
    }
}
```

- `FUNKSTUDY_HASHTAGS` で env 追加したタグもコード変更/デプロイ無しに `#tag` 表示される（既存の env 拡張設計と整合）。
- 既存の `source_label_unknown_id_passthrough`（`source_label("nme") == "nme"` を期待）はこの新挙動（`"#nme"`）に更新する。実運用でサイト系ソースは必ず明示アームを持つ（rokinon / pitchfork のようにブランド名ラベル）ため、`other` アームに落ちるのは funkstudy 系ハッシュタグのみ。
- 呼び出し側 `source_label(&s.source_id).to_string()`（home.rs:444）は `source_label(&s.source_id)`（既に String）に直す。

## ⚠️ 既知の制約（バックフィル不可）

**デプロイ前にスクレイプ済みの funkstudy 投稿は、旧結合ラベル `yetanother(funk|bach)study` のまま残る。**

- `is_scraped`（`funkstudy` キー）が再 fetch をスキップし、かつ当時タグを保存していないため、過去行の per-hashtag 化は**不可能**。
- **デプロイ後に新規取得した投稿のみ** per-hashtag ラベルになる。
- Selector の直近1ヶ月窓でホーム先頭表示は自然に入れ替わるが、`list_recent_albums`（全アルバム一覧）には旧ラベルの行が残り続ける。
- これは仕様。spec に明記して「機能が効いていない」誤認を防ぐ。

## テスト方針（TDD: t_wada）

- `match_configured_hashtag` 純粋関数:
  - `#yetanotherfunkstudy` を含む text → `Some("yetanotherfunkstudy")`
  - `#FUNKStudy` / `#funkstudy`（casing 違い）→ どちらも `Some("FUNKStudy")`
  - `#funkstudy` アンカーが `#yetanotherfunkstudy` に誤マッチしないこと（funk 系 text → funk 系が返る）
  - 該当なし → `None`
- `list_candidates`: 既存 `search.json` で `cands[0].source_id_override == Some("yetanotherfunkstudy")` を検証（検出が candidate に載ること）。
- `fetch_and_extract`: `source_id_override = Some("yetanotherbachstudy")` の CandidateRef を渡し、`rec.source_id == "yetanotherbachstudy"` を検証。`None` のとき `"funkstudy"` フォールバックも検証。
- `build_query`: 3タグ（funk / bach / FUNKStudy）の OR 形を検証。
- config: `funkstudy_defaults_when_env_absent` と `parse_funkstudy_hashtags_handles_defaults_and_custom` を FUNKStudy 込みの既定に更新。
- `source_label`: 3ハッシュタグ id → `#...`、`funkstudy`（旧）→ 結合ラベル、`other`（旧 `nme` テスト）→ `#...` に更新。
- 実接続スモーク: `from:taizooo #FUNKStudy` を実 API に投げ、本番データで検出・取り込みが通ることを確認（モックだけでは契約ズレを見抜けないため必須）。

## スコープ外（YAGNI）

- 過去 funkstudy 行のバックフィル（タグ未保存のため不可・上記制約参照）。
- 単一ソースカードへの常時ソースバッジ表示（今回はドロップダウン内のみ）。
- `entities.hashtags` 構造化検出への切替（初期は text 走査。実物形状確認後の堅牢化として別途）。
