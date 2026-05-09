# ZINE 風リデザイン実装プラン

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `/`（一覧ページ）を ZINE テイストにリデザインし、スマホ2列固定 + クラフト紙＋傾けたカードのビジュアルに刷新する。同時に Tailwind CSS を導入してスタイル基盤を移行する。

**Architecture:** cargo-leptos の Tailwind 統合を使い、`style/main.css` を廃止して `style/tailwind.css` ＋ `tailwind.config.js` 構成に切替える。`src/pages/home.rs` の `view!` は Tailwind utility と一部 component class（4枚周期の傾きだけ）で組む。ロジック・データ層は触らん。

**Tech Stack:** Rust / Leptos 0.7 (SSR + hydrate) / cargo-leptos 0.3.6 / Tailwind CSS v4（cargo-leptos 同梱、Node.js 不要）

**Tailwind v4 注記:** cargo-leptos 0.3.6 は Tailwind v4 を取得する。v4 では JS config (`tailwind.config.js`) は非推奨で、CSS-first 設定（`@theme` ブロック内に `--color-*` `--font-*` `--breakpoint-*` 等を定義）が標準。本プランは v4 前提で記述しとる。`.rs` ファイルは v4 の自動検出対象外のため、`@source "./src/**/*.rs";` を CSS で明示する必要がある。

**関連 spec:** `docs/superpowers/specs/2026-05-09-zine-redesign-design.md`

---

## ファイル構成

| 区分 | パス | 役割 |
|---|---|---|
| 新規 | `style/tailwind.css` | v4 CSS-first 設定（`@import "tailwindcss"` ＋ `@source` ＋ `@theme` ＋ `@layer base/components`）でテーマトークン・ベース・傾き周期・モーション制御を全部入れる |
| 修正 | `Cargo.toml` | `[package.metadata.leptos]` から `style-file` を外し `tailwind-input-file` を追加（v4 では `tailwind-config-file` 不要） |
| 修正 | `src/pages/home.rs` | `view!` のクラス名を Tailwind utility に置換、サブコピー削除、ジャケ画像 None 時のフォールバック追加 |
| 削除 | `style/main.css` | 役割を tailwind.css に譲る |

タスクごとに上記ファイルを 1 つ〜数個ずつ触る。各タスク末で `cargo leptos build` または `cargo leptos watch` を走らせビルドが通ることを確認してからコミットする。

---

## Task 1: Tailwind v4 配線（テーマトークンが効く状態を作る）

**Files:**
- Create: `style/tailwind.css`（v4 CSS-first 設定一式）
- Modify: `Cargo.toml` (`[package.metadata.leptos]` セクション、79行目周辺)
- Delete: `style/main.css`

このタスクのゴールは「Tailwind v4 のビルドが成功し、`bg-paper` などのカスタム utility が CSS に出力される状態を作る」こと。Task 2 でユーティリティを `view!` に当てるけぇ、ここでは検証用の捨て HTML を src 下に置いて出力を確認する。

- [ ] **Step 1: `style/tailwind.css` を新規作成（v4 CSS-first 設定一式）**

`@theme` で全テーマトークンを定義し、`@source` で `.rs` の class 文字列も検出対象にする。`@layer base` でページ背景・main 幅をここに集約。

```css
@import "tailwindcss";

/* .rs ファイルは v4 の自動検出対象外のため明示的に追加 */
@source "./src/**/*.rs";

@theme {
  /* spec の色トークン */
  --color-paper: #f4ecd8;
  --color-card: #fffaf0;
  --color-ink: #2a2418;
  --color-sepia: #6a5a3a;
  --color-placeholder: #e8dcc4;
  --color-spotify: #1db954;
  --color-youtube: #ff0000;
  --color-err: #b00020;

  /* タイポ */
  --font-zine: Georgia, "Hiragino Mincho ProN", serif;

  /* シャドウ */
  --shadow-zine: 2px 3px 0 #2a2418;

  /* ブレイクポイント。tab=600 / pc=1000 のみ採用、デフォルトは封じる */
  --breakpoint-tab: 600px;
  --breakpoint-pc: 1000px;
  --breakpoint-sm: initial;
  --breakpoint-md: initial;
  --breakpoint-lg: initial;
  --breakpoint-xl: initial;
  --breakpoint-2xl: initial;
}

@layer base {
  body {
    background: var(--color-paper);
    color: var(--color-ink);
    margin: 0;
    padding: 1rem;
    font-family: ui-sans-serif, system-ui, sans-serif;
  }
  main {
    max-width: 1200px;
    margin: 0 auto;
  }
}
```

`@layer components`（傾き周期）は Task 3 で追記する。

- [ ] **Step 2: `Cargo.toml` の Leptos metadata を切替え**

`[package.metadata.leptos]` の中身を以下のように修正する（既存の `style-file = "style/main.css"` を削除し、1行追加）。

修正前（78行目）:
```toml
style-file = "style/main.css"
```

修正後:
```toml
tailwind-input-file = "style/tailwind.css"
```

v4 では `tailwind-config-file` は使わん（`@theme` で完結）。他のキー（`output-name`, `site-root`, `bin-target`, `assets-dir`, etc.）は触らん。

- [ ] **Step 3: `style/main.css` を削除**

```bash
rm style/main.css
```

- [ ] **Step 4: ビルド確認**

```bash
cargo leptos build
```

Expected: 成功して `target/site/pkg/i-am-rockin-on.css` が生成される。初回は cargo-leptos が tailwindcss バイナリを fetch するため数十秒かかる場合あり。

ビルド成功後、以下を確認:

```bash
grep -E '(--color-paper|--color-card|--color-ink|--color-sepia|--color-placeholder|--shadow-zine|--font-zine)' target/tmp/tailwind.css | head
```

Expected: テーマトークンが `:root` の CSS 変数として出力されとる。出力に該当行がなければ `@theme` ブロックの記法ミスを疑う。

- [ ] **Step 5: コミット**

```bash
git add style/tailwind.css Cargo.toml
git rm style/main.css
git commit -m "feat(style): introduce Tailwind v4 via cargo-leptos

style/main.css を廃止し style/tailwind.css に v4 CSS-first 設定を集約。
@theme でテーマトークン（paper/card/ink/sepia/placeholder/spotify/
youtube/err 色、zine フォント、zine シャドウ、tab/pc ブレイクポイント）
を定義、@source で .rs ファイルを検出対象に追加、@layer base で
ページ背景と main 幅を設定。view! のクラス置換は次タスクで実施。"
```

---

## Task 2: home.rs の `view!` を Tailwind utility に書き換え

**Files:**
- Modify: `src/pages/home.rs` (90-132行目、`Home` と `RecommendationGrid` コンポーネント)

このタスクで見た目が ZINE になる。傾きは Task 3 で `.tilt-cycle` を CSS に追加するまで効かんが、それ以外は完成形に近づく。

- [ ] **Step 1: `Home` コンポーネントを修正**

`src/pages/home.rs:89-102` を以下に置換える:

```rust
#[component]
pub fn Home() -> impl IntoView {
    let recs = Resource::new(|| (), |_| async { list_recommendations().await });
    view! {
        <header class="border-b-4 border-double border-ink pb-2 mb-6">
            <h1 class="font-zine italic font-bold text-3xl text-ink m-0">
                "i am rockin on"
            </h1>
        </header>
        <Suspense fallback=|| view! { <p class="text-sepia">"loading..."</p> }>
            {move || recs.get().map(|r| match r {
                Ok(items) => view! { <RecommendationGrid items=items/> }.into_any(),
                Err(e) => view! {
                    <p class="text-err">{format!("error: {e}")}</p>
                }.into_any(),
            })}
        </Suspense>
    }
}
```

変更点:
- `<h1>` 単独 → `<header>` でラップし二重ボーダー（`border-b-4 border-double`）
- `<p class="lede">` 行を削除
- `loading` / `error` テキストにクラス追加

- [ ] **Step 2: `RecommendationGrid` コンポーネントを修正**

`src/pages/home.rs:104-132` を以下に置換える（ジャケ画像 None 時のフォールバックは Task 4 で追加するけぇ、ここでは既存の `.map(...)` を維持）:

```rust
#[component]
fn RecommendationGrid(items: Vec<RecommendationView>) -> impl IntoView {
    view! {
        <ul class="tilt-cycle list-none p-0 m-0 grid grid-cols-2 tab:grid-cols-3 pc:grid-cols-4 gap-5">
            {items.into_iter().map(|item| view! {
                <li class="bg-card shadow-zine p-3 flex flex-col gap-2">
                    {item.spotify_image_url.as_ref().map(|src| view! {
                        <img
                            class="w-full aspect-square object-cover bg-paper"
                            src=src.clone()
                            alt=""
                            loading="lazy"
                        />
                    })}
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
                        <a
                            class="text-xs font-semibold px-2.5 py-1 rounded-full border border-ink text-ink no-underline"
                            href=item.source_url
                            target="_blank"
                            rel="noopener"
                        >"記事"</a>
                    </div>
                </li>
            }).collect_view()}
        </ul>
    }
}
```

主な変更点:
- グリッド: `grid grid-cols-2 md:grid-cols-3 lg:grid-cols-4 gap-5` でスマホ2列・タブレット3列・PC4列
- `tilt-cycle` クラス（CSS は Task 3 で追加）
- カード: `bg-card shadow-zine p-3 flex flex-col gap-2`
- メタ情報: ZINE 風の Georgia セリフ体、サブ情報は `text-sepia`
- ボタン: 既存の意味色（spotify 緑 / youtube 赤）を Tailwind トークンで再定義

- [ ] **Step 3: 開発サーバで確認**

```bash
cargo leptos watch
```

ブラウザで `http://localhost:3000` を開く。期待される表示:
- クラフト紙背景（`#f4ecd8`）
- セリフ体イタリックの "i am rockin on" タイトルに二重ボーダー
- カードはクラフト紙白 (`#fffaf0`) ＋ ハードシャドウ
- スマホ幅で2列、ワイドで4列
- まだ傾いとらん（Task 3 で追加）

ビルドエラー時の確認: `cargo leptos watch` のターミナル出力に Tailwind の warning（"No utility class found"）が出たらクラス名の typo を疑う。

- [ ] **Step 4: コミット**

```bash
git add src/pages/home.rs
git commit -m "feat(home): apply ZINE Tailwind utilities to view!

セリフ体タイトル・二重ボーダー・グリッド（スマホ2列／タブ3列／PC4列）
・カード形状・ボタン（既存意味色維持）に書き換え。サブコピーと
.lede 削除。傾きは次タスクで .tilt-cycle として追加。"
```

---

## Task 3: 4枚周期の傾きとマイクロインタラクションを CSS に追加

**Files:**
- Modify: `style/tailwind.css`（Task 2 で書いた内容に `@layer components` ブロックを追加）

Tailwind の `nth-child` ベースの周期回転は arbitrary group / variant では表現しにくいため、`@layer components` で書く。

- [ ] **Step 1: `style/tailwind.css` に component layer を追加**

Task 1 で書いた `style/tailwind.css` の末尾（`@layer base { … }` の後ろ）に追加:

```css
@layer components {
  /* ZINE: 4枚周期で傾けて貼り付けた紙 */
  .tilt-cycle > * {
    transition: transform 0.2s ease;
  }
  .tilt-cycle > :nth-child(4n+1) { transform: rotate(-0.7deg); }
  .tilt-cycle > :nth-child(4n+2) { transform: rotate(0.5deg); }
  .tilt-cycle > :nth-child(4n+3) { transform: rotate(0.3deg); }
  .tilt-cycle > :nth-child(4n+4) { transform: rotate(-0.5deg); }

  .tilt-cycle > *:hover,
  .tilt-cycle > *:focus-within {
    transform: rotate(0deg) translateY(-3px);
  }

  @media (prefers-reduced-motion: reduce) {
    .tilt-cycle > *,
    .tilt-cycle > *:hover,
    .tilt-cycle > *:focus-within {
      transition: none;
      transform: none;
    }
  }
}
```

- [ ] **Step 2: 開発サーバで確認**

`cargo leptos watch` が watch 中なら自動でリロードされる。ブラウザで:
- 各カードが微妙に傾いとる（4枚周期）
- カードにマウスオーバーで傾き 0° に戻り `-3px` 浮く
- カード内ボタンに Tab フォーカスで同様に浮く（focus-within）

OS の「視差を減らす」設定で transition が止まることも確認できれば◎（任意）。

- [ ] **Step 3: コミット**

```bash
git add style/tailwind.css
git commit -m "feat(style): add tilt-cycle for ZINE card rotation

@layer components で 4枚周期回転 + hover/focus-within で 0°復帰＋浮き、
prefers-reduced-motion で無効化。"
```

---

## Task 4: ジャケ画像 None 時のプレースホルダ

**Files:**
- Modify: `src/pages/home.rs`（`RecommendationGrid` 内のジャケ画像分岐）

Spotify でマッチしなかったレコードは `spotify_image_url` が `None`。現状は何も表示せんため、ZINE デザインだとカードに穴が開く。プレースホルダを出す。

- [ ] **Step 1: `RecommendationGrid` のジャケ画像分岐を修正**

Task 2 で書いた `.map(|src| view! { <img ... /> })` の部分（`src/pages/home.rs` 中の該当箇所）を以下のような `match` に置き換える:

```rust
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
```

`into_any()` は左右の view! の型を揃えるために必要。

- [ ] **Step 2: 動作確認**

DB に Spotify マッチなしのレコードがあれば自動で出る。なければ確認用に sqlite で1件 update して確認:

```bash
sqlite3 data/app.db "SELECT id, artist_name, spotify_image_url FROM recommendations LIMIT 5;"
# 既存の1件を選んで:
sqlite3 data/app.db "UPDATE recommendations SET spotify_image_url = NULL WHERE id = <選んだid>;"
```

ブラウザで該当カードに ♪ プレースホルダが出ることを確認。確認後は元に戻す:

```bash
# 確認用に消したやつをスクレイプし直すか、本番では戻さんでも次回スクレイプで再取得される
sqlite3 data/app.db "UPDATE recommendations SET spotify_image_url = '<元のURL>' WHERE id = <選んだid>;"
```

- [ ] **Step 3: コミット**

```bash
git add src/pages/home.rs
git commit -m "feat(home): show ♪ placeholder when Spotify image is missing

ジャケ画像 None のレコードでカードに穴が開かんよう、同サイズの
プレースホルダ枠（クラフト紙ベタ＋♪）を出す。"
```

---

## Task 5: ブレイクポイント手動検証 と reduced-motion 確認

**Files:**
- 編集なし（検証のみ）

ZINE リデザインの仕様準拠を手動でチェックリスト消化する。

- [ ] **Step 1: スマホ幅で2列**

ブラウザの DevTools で viewport 375px に設定し `http://localhost:3000` を開く。
Expected: グリッドが 2列。

- [ ] **Step 2: タブレット幅で3列**

DevTools viewport を 768px に変更。
Expected: グリッドが 3列。

- [ ] **Step 3: PC 幅で4列**

DevTools viewport を 1280px に変更。
Expected: グリッドが 4列。

- [ ] **Step 4: ホバーで傾き戻り＋浮き**

PC 幅でカードにマウスオーバー。
Expected: 傾きが 0° に戻り、`translateY(-3px)` で浮く。

- [ ] **Step 5: キーボードフォーカスで同等の挙動**

Tab キーでカード内のリンク（Spotify ボタン等）にフォーカス移動。
Expected: 親 `<li>` の傾きが戻り浮く（focus-within）。

- [ ] **Step 6: prefers-reduced-motion 設定下でアニメ停止**

macOS: システム設定 → アクセシビリティ → 視差を減らす ON
（または DevTools の Rendering タブで "Emulate CSS prefers-reduced-motion: reduce" を有効化）

カードにホバーしても transition せず、傾きも当たらない（つまり 0° のまま）ことを確認。
Expected: アニメ・傾き両方無効。

- [ ] **Step 7: ジャケ画像なしのプレースホルダ表示**

Task 4 Step 2 で確認済みなら省略可。

- [ ] **Step 8: 既存の Suspense ロード状態とエラー状態**

開発サーバ初回ロード時に `loading...` が一瞬出ることを確認。
DB を空にしてエラーを誘発する手順は不要（`error: ...` のスタイルだけ目視）。

このタスクではコミットせん。問題があれば該当タスクに戻り修正→コミットを切る。

---

## Task 6: ロジック側の既存テストが壊れとらんことを確認

**Files:**
- 編集なし（テストのみ）

ロジック・データ層は触っとらんけぇ既存テストは全件パスするはず。最終確認。

- [ ] **Step 1: テスト実行**

```bash
cargo test --features ssr
```

Expected: 既存テスト全件 PASS。`spotify_app_uri` の3テストを含む。

- [ ] **Step 2: WASM ビルド確認**

```bash
cargo leptos build --release
```

Expected: 成功。`target/site/pkg/i-am-rockin-on.css` のサイズが妥当（数KB〜数十KB）。

- [ ] **Step 3: Docker ビルド確認（任意）**

ローカルで Docker が動く環境のみ。fly.io デプロイ前に1回確認しとくと安全。

```bash
docker build -t i-am-rockin-on:zine .
```

Expected: 成功。Tailwind バイナリ取得が cargo-leptos のステップ内で走る。

このタスクではコミットせん。問題があれば該当タスクへ戻り修正→コミット。

---

## 完了条件

- [ ] Task 1〜4 のコミット 4本（Tailwind 配線 / view! 書換 / tilt-cycle / プレースホルダ）が main ブランチに乗っとる
- [ ] Task 5 のチェックリストが全項目通っとる
- [ ] `cargo test --features ssr` が PASS
- [ ] `cargo leptos build --release` が成功
- [ ] `style/main.css` が削除されとる
- [ ] DevTools での目視で、スマホ2列・PC4列・傾き＋ホバー・♪プレースホルダ・reduced-motion 全て期待通り

## 完了後

fly.io へのデプロイは別途ユーザが `fly deploy` で行う（destructive な操作のためプラン外）。デプロイ後、本番環境でも同じ手動検証チェックリストを通す。
