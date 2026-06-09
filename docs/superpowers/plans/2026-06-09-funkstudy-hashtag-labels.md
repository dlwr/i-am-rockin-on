# funkstudy ハッシュタグ別ラベル ＋ #FUNKStudy 取り込み Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** funkstudy 投稿を実際のハッシュタグ名（`#yetanotherfunkstudy` / `#yetanotherbachstudy` / `#FUNKStudy`）でドロップダウン表示し、`#FUNKStudy` も取り込み対象に加える。

**Architecture:** 保存行の `source_id` を投稿ごとの実ハッシュタグにする（スキーマ変更なし）。検出は `list_candidates`（検索結果 text に該当タグが構造上必ず含まれる）で行い、`CandidateRef.source_id_override` で `fetch_and_extract` へ運ぶ。表示は `source_label` を invert 方式にして未知 id を `#tag` 表示にフォールバック。scrape 追跡は従来どおりアダプタ `id() = "funkstudy"` のままで dedup は安定。

**Tech Stack:** Rust / async-trait / sqlx(SQLite) / Leptos / wiremock（テスト）。

参照 spec: `docs/superpowers/specs/2026-06-09-funkstudy-hashtag-labels-design.md`

---

## File Structure

- `src/server/adapter/source.rs` — `CandidateRef` に `source_id_override: Option<String>` 追加（全アダプタ共通の運搬チャネル）
- `src/server/adapter/funkstudy.rs` — `match_configured_hashtag` 純粋関数 / `list_candidates` で検出 / `fetch_and_extract` で override→source_id / 既定タグに FUNKStudy
- `src/server/config.rs` — `parse_funkstudy_hashtags` 既定に FUNKStudy
- `src/pages/home.rs` — `source_label` を invert 方式（`&str`→`String`）に変更 + 呼び出し側
- `src/server/adapter/pitchfork.rs`, `src/server/adapter/rokinon.rs`, `src/server/scrape.rs` — 既存 `CandidateRef` リテラルに `source_id_override: None` 追加（コンパイル維持）
- `README.md` — `FUNKSTUDY_HASHTAGS` 既定値とソース説明の更新

---

## Task 1: CandidateRef に source_id_override フィールド追加（構造プレップ）

新しい振る舞いは無い。フィールドを足し、既存の全 `CandidateRef` リテラルを `source_id_override: None` で埋めてコンパイル・既存テストを維持するだけの機械的ステップ。

**Files:**
- Modify: `src/server/adapter/source.rs`
- Modify: `src/server/adapter/pitchfork.rs`, `src/server/adapter/rokinon.rs`, `src/server/adapter/funkstudy.rs`, `src/server/scrape.rs`

- [ ] **Step 1: 構造体にフィールド追加**

`src/server/adapter/source.rs` の `CandidateRef` を次に置き換える:

```rust
#[derive(Debug, Clone)]
pub struct CandidateRef {
    pub source_external_id: String,
    pub source_url: String,
    /// 保存行の source_id 上書き。 None ならアダプタ既定（fetch_and_extract が決める）を使う。
    /// funkstudy が「どのハッシュタグでマッチしたか」を list_candidates → fetch_and_extract へ運ぶ。
    pub source_id_override: Option<String>,
}
```

- [ ] **Step 2: 既存の全リテラルに `source_id_override: None` を追加**

次の各 `CandidateRef { ... }` リテラルに `source_id_override: None,` を1行足す（`source_url: ...` の直後）:

- `src/server/adapter/pitchfork.rs`: `list_candidates` 内の `out.push(CandidateRef { source_external_id: slug..., source_url: self.make_absolute(&path) })` と、テストの `cand`（`aldous-harding-...` / `some-low-score` / `old-artist-old-album` の3箇所）
- `src/server/adapter/rokinon.rs`: `list_candidates` 内の `out.push(CandidateRef { source_external_id: entry_id, source_url: self.make_absolute(href) })` と、テストの `candidate`（`12966301740` の2箇所）
- `src/server/adapter/funkstudy.rs`: `list_candidates` 内の `out.push(CandidateRef { source_external_id: t.id, source_url })`（※ Task 3 で値を入れるが、ここでは一旦 `None`）と、テストの `cand`（`1001` / `2042213253716341074` 系の4箇所）
- `src/server/scrape.rs`: テスト内の `FakeSource::list_candidates` の `.map(|i| CandidateRef { ... })`、`CountingSource`（`e1`）、`NoneSource`（`e2`）、`FailingSource`（`ok`/`fail`/`ok2` の3箇所）

例（funkstudy list_candidates）:

```rust
        out.push(CandidateRef {
            source_external_id: t.id,
            source_url,
            source_id_override: None,
        });
```

- [ ] **Step 3: ビルド + 全テストで緑を確認**

Run: `cargo test --features ssr --quiet`
Expected: PASS（新規挙動なし。全既存テストが通る）

- [ ] **Step 4: Commit**

```bash
git add src/server/adapter/source.rs src/server/adapter/pitchfork.rs src/server/adapter/rokinon.rs src/server/adapter/funkstudy.rs src/server/scrape.rs
git commit -m "refactor(adapter): CandidateRef に source_id_override を追加"
```

---

## Task 2: match_configured_hashtag 純粋関数

設定タグ群のうち text にマッチする最初のタグを設定上の表記で返す純粋関数。

**Files:**
- Modify: `src/server/adapter/funkstudy.rs`

- [ ] **Step 1: 失敗するテストを書く**

`src/server/adapter/funkstudy.rs` の `mod tests` 内に追加:

```rust
    #[test]
    fn match_configured_hashtag_returns_first_matching_in_config_order() {
        let cfg = vec![
            "yetanotherfunkstudy".to_string(),
            "yetanotherbachstudy".to_string(),
            "FUNKStudy".to_string(),
        ];
        assert_eq!(
            match_configured_hashtag("写真 #yetanotherfunkstudy", &cfg),
            Some("yetanotherfunkstudy".to_string())
        );
        assert_eq!(
            match_configured_hashtag("#yetanotherbachstudy なう", &cfg),
            Some("yetanotherbachstudy".to_string())
        );
    }

    #[test]
    fn match_configured_hashtag_normalizes_casing_to_config_form() {
        let cfg = vec!["FUNKStudy".to_string()];
        // 大文字小文字が違っても設定側の正準表記で返す
        assert_eq!(match_configured_hashtag("#FUNKStudy", &cfg), Some("FUNKStudy".to_string()));
        assert_eq!(match_configured_hashtag("#funkstudy", &cfg), Some("FUNKStudy".to_string()));
    }

    #[test]
    fn match_configured_hashtag_anchors_on_hash_so_funkstudy_does_not_match_inside_yetanother() {
        // "#funkstudy" は "#yetanotherfunkstudy" の部分文字列ではない（直前が '#' でなく 'r'）
        let cfg = vec!["FUNKStudy".to_string(), "yetanotherfunkstudy".to_string()];
        assert_eq!(
            match_configured_hashtag("#yetanotherfunkstudy", &cfg),
            Some("yetanotherfunkstudy".to_string())
        );
    }

    #[test]
    fn match_configured_hashtag_returns_none_when_absent() {
        let cfg = vec!["yetanotherfunkstudy".to_string()];
        assert_eq!(match_configured_hashtag("ただのツイート", &cfg), None);
    }
```

- [ ] **Step 2: テストが失敗（コンパイルエラー）することを確認**

Run: `cargo test --features ssr --quiet match_configured_hashtag`
Expected: FAIL（`cannot find function match_configured_hashtag`）

- [ ] **Step 3: 関数を実装**

`src/server/adapter/funkstudy.rs` の `build_query` 付近（モジュール関数の並び）に追加:

```rust
/// `text` に `#<tag>` が含まれる最初の設定タグを、 設定側の正準表記で返す。
/// `#` アンカー + case-insensitive 一致。 `#` を前置することで `#funkstudy` が
/// `#yetanotherfunkstudy` に誤マッチしない（直前が `#` でないと一致しないため）。
fn match_configured_hashtag(text: &str, configured: &[String]) -> Option<String> {
    let lower = text.to_lowercase();
    configured
        .iter()
        .find(|tag| lower.contains(&format!("#{}", tag.to_lowercase())))
        .cloned()
}
```

- [ ] **Step 4: テストが通ることを確認**

Run: `cargo test --features ssr --quiet match_configured_hashtag`
Expected: PASS（4テスト）

- [ ] **Step 5: Commit**

```bash
git add src/server/adapter/funkstudy.rs
git commit -m "feat(funkstudy): ハッシュタグ検出の純粋関数を追加"
```

---

## Task 3: list_candidates で検出して source_id_override に載せる

**Files:**
- Modify: `src/server/adapter/funkstudy.rs`

- [ ] **Step 1: 失敗するテストを書く（既存テストに assert 追加）**

`src/server/adapter/funkstudy.rs` の `list_candidates_returns_funkstudy_posts` テストの末尾（最後の `assert_eq!` の後）に追加:

```rust
        assert_eq!(
            cands[0].source_id_override.as_deref(),
            Some("yetanotherfunkstudy"),
            "search.json の text '#yetanotherfunkstudy' から検出して載せる"
        );
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `cargo test --features ssr --quiet list_candidates_returns_funkstudy_posts`
Expected: FAIL（`source_id_override` が `None` のまま）

- [ ] **Step 3: list_candidates で検出値を載せる**

`src/server/adapter/funkstudy.rs` の `list_candidates` の push 部分を次に置き換える（Task 1 で `None` にした箇所）:

```rust
        for t in resp.tweets {
            let source_url = if t.url.is_empty() {
                format!("https://x.com/{}/status/{}", self.screen_name, t.id)
            } else {
                t.url
            };
            let source_id_override = match_configured_hashtag(&t.text, &self.hashtags);
            out.push(CandidateRef {
                source_external_id: t.id,
                source_url,
                source_id_override,
            });
        }
```

- [ ] **Step 4: テストが通ることを確認**

Run: `cargo test --features ssr --quiet list_candidates_returns_funkstudy_posts`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/server/adapter/funkstudy.rs
git commit -m "feat(funkstudy): list_candidates でハッシュタグを検出し candidate に載せる"
```

---

## Task 4: fetch_and_extract で override を source_id に使う

**Files:**
- Modify: `src/server/adapter/funkstudy.rs`

- [ ] **Step 1: 失敗するテストを書く**

`src/server/adapter/funkstudy.rs` の `mod tests` 内に追加（`fetch_and_extract_pulls_spotify_from_self_reply` を参考にした override 版）:

```rust
    #[tokio::test]
    async fn fetch_and_extract_uses_source_id_override_for_source_id() {
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
            source_id_override: Some("yetanotherbachstudy".into()),
        };
        let rec = adapter.fetch_and_extract(&cand).await.unwrap().unwrap();
        assert_eq!(rec.source_id, "yetanotherbachstudy");
    }

    #[tokio::test]
    async fn fetch_and_extract_falls_back_to_funkstudy_when_override_is_none() {
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
            source_id_override: None,
        };
        let rec = adapter.fetch_and_extract(&cand).await.unwrap().unwrap();
        assert_eq!(rec.source_id, "funkstudy");
    }
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `cargo test --features ssr --quiet fetch_and_extract_uses_source_id_override`
Expected: FAIL（`source_id` が常に `"funkstudy"` で `yetanotherbachstudy` と一致しない）

- [ ] **Step 3: source_id を override から取るよう実装**

`src/server/adapter/funkstudy.rs` の `fetch_and_extract` 内、`NewRecommendation` 構築の `source_id: "funkstudy".into(),` を次に置き換える:

```rust
                    source_id: candidate
                        .source_id_override
                        .clone()
                        .unwrap_or_else(|| "funkstudy".to_string()),
```

- [ ] **Step 4: テストが通ることを確認**

Run: `cargo test --features ssr --quiet fetch_and_extract`
Expected: PASS（override 版・fallback 版・既存の自己リプ抽出テストすべて）

- [ ] **Step 5: Commit**

```bash
git add src/server/adapter/funkstudy.rs
git commit -m "feat(funkstudy): source_id_override を保存 source_id に反映（既定は funkstudy）"
```

---

## Task 5: #FUNKStudy を既定ハッシュタグに追加

**Files:**
- Modify: `src/server/adapter/funkstudy.rs`（`new()` 既定 + `build_query` テスト）
- Modify: `src/server/config.rs`（`parse_funkstudy_hashtags` 既定 + テスト2件）

- [ ] **Step 1: 失敗するテストを書く（config + build_query）**

`src/server/config.rs` の `parse_funkstudy_hashtags_handles_defaults_and_custom` の None/空 ケース2つの期待値を次に更新:

```rust
        // 未設定・空 → 既定 (funk + bach + FUNKStudy)
        assert_eq!(
            parse_funkstudy_hashtags(None),
            vec![
                "yetanotherfunkstudy".to_string(),
                "yetanotherbachstudy".to_string(),
                "FUNKStudy".to_string()
            ]
        );
        assert_eq!(
            parse_funkstudy_hashtags(Some("  ,  ".into())),
            vec![
                "yetanotherfunkstudy".to_string(),
                "yetanotherbachstudy".to_string(),
                "FUNKStudy".to_string()
            ]
        );
```

`src/server/config.rs` の `funkstudy_defaults_when_env_absent` の `cfg.funkstudy_hashtags` の assert を次に更新:

```rust
        assert_eq!(
            cfg.funkstudy_hashtags,
            vec![
                "yetanotherfunkstudy".to_string(),
                "yetanotherbachstudy".to_string(),
                "FUNKStudy".to_string()
            ]
        );
```

`src/server/adapter/funkstudy.rs` の `build_query_single_vs_multiple_hashtags` に3タグの assert を追加:

```rust
        assert_eq!(
            build_query(
                "taizooo",
                &[
                    "yetanotherfunkstudy".into(),
                    "yetanotherbachstudy".into(),
                    "FUNKStudy".into()
                ],
                "2026-05-03"
            ),
            "from:taizooo (#yetanotherfunkstudy OR #yetanotherbachstudy OR #FUNKStudy) since:2026-05-03"
        );
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `cargo test --features ssr --quiet parse_funkstudy_hashtags funkstudy_defaults_when_env_absent build_query`
Expected: FAIL（既定が funk+bach の2件のまま / build_query は既存 `many` アームで通るので3タグ assert は実は PASS する。config 2件が FAIL）

- [ ] **Step 3: 既定にFUNKStudyを追加**

`src/server/adapter/funkstudy.rs` の `new()` 内 `hashtags` 既定を更新:

```rust
            hashtags: vec![
                "yetanotherfunkstudy".into(),
                "yetanotherbachstudy".into(),
                "FUNKStudy".into(),
            ],
```

`src/server/config.rs` の `parse_funkstudy_hashtags` の fallback を更新:

```rust
    if parsed.is_empty() {
        vec![
            "yetanotherfunkstudy".into(),
            "yetanotherbachstudy".into(),
            "FUNKStudy".into(),
        ]
    } else {
        parsed
    }
```

- [ ] **Step 4: テストが通ることを確認**

Run: `cargo test --features ssr --quiet`
Expected: PASS（config・build_query 含む全テスト）

- [ ] **Step 5: Commit**

```bash
git add src/server/adapter/funkstudy.rs src/server/config.rs
git commit -m "feat(funkstudy): #FUNKStudy を既定ハッシュタグに追加"
```

---

## Task 6: source_label を invert 方式に変更（表示ラベル）

**Files:**
- Modify: `src/pages/home.rs`

- [ ] **Step 1: 失敗するテストを書く**

`src/pages/home.rs` の `mod tests` 内、`source_label_unknown_id_passthrough` を次に置き換え、ハッシュタグ用テストを追加:

```rust
    #[test]
    fn source_label_hashtag_sources_get_hash_prefix() {
        assert_eq!(source_label("yetanotherfunkstudy"), "#yetanotherfunkstudy");
        assert_eq!(source_label("yetanotherbachstudy"), "#yetanotherbachstudy");
        assert_eq!(source_label("FUNKStudy"), "#FUNKStudy");
    }

    #[test]
    fn source_label_unknown_id_falls_back_to_hash_prefix() {
        // サイト系ソースは必ず明示アームを持つ。 other に落ちるのは funkstudy 系タグのみという前提。
        assert_eq!(source_label("nme"), "#nme");
    }
```

既存の `source_label_known_ids` はそのまま（`String` vs `&str` の `assert_eq!` は通る）。

- [ ] **Step 2: テストが失敗することを確認**

Run: `cargo test --features ssr --quiet source_label`
Expected: FAIL（現状 `source_label` は `&str` 返しで `"yetanotherfunkstudy"` をそのまま返す → `#` 付きと一致しない）

- [ ] **Step 3: source_label を invert 方式に実装**

`src/pages/home.rs` の `source_label` を次に置き換える:

```rust
/// `source_id` を表示用ラベルに写像する。
/// サイト系ソースは明示マッピング。 それ以外（funkstudy 系ハッシュタグ、
/// FUNKSTUDY_HASHTAGS で env 追加した分も含む）は `#tag` 表示にフォールバックする。
fn source_label(source_id: &str) -> String {
    match source_id {
        "rokinon" => "ロキノンには騙されないぞ".into(),
        "pitchfork" => "Pitchfork".into(),
        // デプロイ前にスクレイプ済みの旧 funkstudy 行（タグ未分離）向けの残置ラベル
        "funkstudy" => "yetanother(funk|bach)study".into(),
        other => format!("#{other}"),
    }
}
```

- [ ] **Step 4: 呼び出し側を修正**

`src/pages/home.rs` の `SourceMenu` 内 `{source_label(&s.source_id).to_string()}` を次に変更（戻り値が既に `String`）:

```rust
                        >{source_label(&s.source_id)}</a>
```

- [ ] **Step 5: テストが通ることを確認**

Run: `cargo test --features ssr --quiet source_label`
Expected: PASS（known_ids / hashtag_sources / unknown_fallback）

- [ ] **Step 6: ビルド全体確認（Leptos view マクロ含む）**

Run: `cargo test --features ssr --quiet`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add src/pages/home.rs
git commit -m "feat(home): source_label を invert 方式にして funkstudy 系タグを #付き表示"
```

---

## Task 7: README 更新

**Files:**
- Modify: `README.md`

- [ ] **Step 1: ソース説明（44行目付近）を更新**

`README.md` の funkstudy ソース説明行を次に変更:

```
- **funkstudy** — X の taizooo (`FUNKSTUDY_SCREEN_NAME`) の `#yetanotherfunkstudy` / `#yetanotherbachstudy` / `#FUNKStudy`（`FUNKSTUDY_HASHTAGS` で増減可）付きポストを twitterapi.io 経由で拾い、 ぶら下がる返信中の Spotify アルバム URL から取り込む。 ドロップダウンには投稿ごとに実際のハッシュタグ名を表示する
```

- [ ] **Step 2: env 表（64行目付近）の既定値を更新**

`README.md` の `FUNKSTUDY_HASHTAGS` 行の既定値を次に変更:

```
| `FUNKSTUDY_HASHTAGS` | no | `yetanotherfunkstudy,yetanotherbachstudy,FUNKStudy` | 取り込む `#…study` 系タグ（カンマ区切り・`#` 任意）。複数は OR 検索 |
```

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs(README): funkstudy のハッシュタグ既定値と表示説明を更新"
```

---

## Task 8: 実接続スモーク（本番データで検出・取り込み確認）

モックだけでは twitterapi.io の text/レスポンス形状の契約ズレを見抜けないため、実 API で1回確認する。**コードは変更しない**。

**前提:** `.env` に `FUNKSTUDY_API_KEY`（twitterapi.io）と `SPOTIFY_CLIENT_ID` / `SPOTIFY_CLIENT_SECRET` が設定済み。`DATABASE_URL` はローカル sqlite。

- [ ] **Step 1: ローカルで funkstudy スクレイプを実行**

Run:
```bash
cargo run --features ssr --bin scrape -- --source funkstudy
```
Expected: パニックや 4xx/5xx で落ちない。ログに advanced_search クエリ `(#yetanotherfunkstudy OR #yetanotherbachstudy OR #FUNKStudy)` が出る。

- [ ] **Step 2: 取り込み結果の source_id を確認**

Run（sqlite ファイルパスは `.env` の `DATABASE_URL` に合わせる）:
```bash
sqlite3 data/app.db "SELECT source_id, COUNT(*) FROM recommendations WHERE source_id IN ('yetanotherfunkstudy','yetanotherbachstudy','FUNKStudy') GROUP BY source_id;"
```
Expected: デプロイ後に新規取得された投稿があれば per-hashtag の source_id が現れる（無ければ 0 行＝新規投稿が無いだけで異常ではない。`#FUNKStudy` の取り込みは Step 1 のログでクエリ送出を確認できれば足りる）。

- [ ] **Step 3: 確認結果を記録**

スモークで分かったこと（実 text 形状・検出可否・新規取り込み件数）を一言メモして次工程（PR）へ。

---

## Self-Review メモ

- spec の各節 → タスク対応: 検出点(list_candidates)=Task 2-3 / CandidateRef 運搬=Task 1 / fetch_and_extract override=Task 4 / #FUNKStudy 既定=Task 5 / source_label invert=Task 6 / README=Task 7 / 実接続スモーク=Task 8。✅ 全カバー。
- 型整合: `source_id_override: Option<String>`（Task 1 定義）を Task 3/4 で使用。`match_configured_hashtag(&str, &[String]) -> Option<String>`（Task 2 定義）を Task 3 で使用。`source_label(&str) -> String`（Task 6）と呼び出し側修正一致。✅
- バックフィル不可の制約は spec に明記済み。Task 6 の `funkstudy` 残置アームがそれを担保。
- プレースホルダ無し（全ステップに実コード/実コマンド）。✅
