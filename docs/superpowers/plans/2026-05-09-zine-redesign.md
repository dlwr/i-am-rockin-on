# ZINE 風リデザイン実装プラン

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `/`（一覧ページ）を ZINE テイストにリデザインし、スマホ2列固定 + クラフト紙＋傾けたカードのビジュアルに刷新する。同時に Tailwind CSS を導入してスタイル基盤を移行する。

**Architecture:** cargo-leptos の Tailwind 統合を使い、`style/main.css` を廃止して `style/tailwind.css` ＋ `tailwind.config.js` 構成に切替える。`src/pages/home.rs` の `view!` は Tailwind utility と一部 component class（4枚周期の傾きだけ）で組む。ロジック・データ層は触らん。

**Tech Stack:** Rust / Leptos 0.7 (SSR + hydrate) / cargo-leptos 0.3.6 / Tailwind CSS（cargo-leptos 同梱、Node.js 不要）

**関連 spec:** `docs/superpowers/specs/2026-05-09-zine-redesign-design.md`

---

## ファイル構成

| 区分 | パス | 役割 |
|---|---|---|
| 新規 | `tailwind.config.js` | テーマトークン（色・フォント・傾き角度・シャドウ） |
| 新規 | `style/tailwind.css` | `@tailwind` ディレクティブ ＋ `@layer components` で 4枚周期傾きとモーション制御 |
| 修正 | `Cargo.toml` | `[package.metadata.leptos]` から `style-file` を外し `tailwind-input-file` / `tailwind-config-file` を追加 |
| 修正 | `src/pages/home.rs` | `view!` のクラス名を Tailwind utility に置換、サブコピー削除、ジャケ画像 None 時のフォールバック追加 |
| 削除 | `style/main.css` | 役割を tailwind.css に譲る |

タスクごとに上記ファイルを 1 つ〜数個ずつ触る。各タスク末で `cargo leptos build` または `cargo leptos watch` を走らせビルドが通ることを確認してからコミットする。

---

## Task 1: Tailwind 配線（ビルドが通る無味無臭の状態を作る）

**Files:**
- Create: `tailwind.config.js`
- Create: `style/tailwind.css`
- Modify: `Cargo.toml` (`[package.metadata.leptos]` セクション、79行目周辺)
- Delete: `style/main.css`

このタスクのゴールは「Tailwind ありのビルドが成功する」だけ。見た目は壊れる（既存 CSS を消すため）。後続タスクで埋める。

- [ ] **Step 1: `tailwind.config.js` を新規作成**

```js
// tailwind.config.js
/** @type {import('tailwindcss').Config} */
module.exports = {
  content: ["./src/**/*.rs"],
  theme: {
    // spec のブレイクポイント（600px / 1000px）に合わせて Tailwind デフォルトを上書き。
    // 既定の sm/md/lg/xl/2xl は使わん（spec で必要な 2段だけに絞る）。
    screens: {
      tab: "600px",  // タブレット閾値
      pc: "1000px",  // PC 閾値
    },
    extend: {
      colors: {
        paper: "#f4ecd8",
        card: "#fffaf0",
        ink: "#2a2418",
        sepia: "#6a5a3a",
        placeholder: "#e8dcc4",
        spotify: "#1db954",
        youtube: "#ff0000",
        err: "#b00020",
      },
      fontFamily: {
        zine: ['Georgia', '"Hiragino Mincho ProN"', 'serif'],
      },
      boxShadow: {
        zine: "2px 3px 0 #2a2418",
      },
    },
  },
  plugins: [],
};
```

- [ ] **Step 2: `style/tailwind.css` を新規作成（最小版）**

```css
@tailwind base;
@tailwind components;
@tailwind utilities;
```

Task 3 で `@layer components` を追加するけぇ、ここでは directives だけ。

- [ ] **Step 3: `Cargo.toml` の Leptos metadata を切替え**

`[package.metadata.leptos]` の中身を以下のように修正する（既存の `style-file = "style/main.css"` を削除し、2行追加）。

修正前（78行目）:
```toml
style-file = "style/main.css"
```

修正後:
```toml
tailwind-input-file = "style/tailwind.css"
tailwind-config-file = "tailwind.config.js"
```

他のキー（`output-name`, `site-root`, `bin-target`, `assets-dir`, etc.）は触らん。

- [ ] **Step 4: `style/main.css` を削除**

```bash
rm style/main.css
```

- [ ] **Step 5: ビルド確認**

```bash
cargo leptos build
```

Expected: 成功して `target/site/pkg/i-am-rockin-on.css` が生成される。初回は cargo-leptos が tailwindcss バイナリを fetch するため数十秒かかる場合あり。失敗時のよくある原因:
- `tailwind.config.js` の構文エラー → エラーメッセージで行番号確認
- `content` パスの typo → `./src/**/*.rs` であること

- [ ] **Step 6: コミット**

```bash
git add tailwind.config.js style/tailwind.css Cargo.toml
git rm style/main.css
git commit -m "feat(style): introduce Tailwind via cargo-leptos

style/main.css を廃止し style/tailwind.css ＋ tailwind.config.js 構成に
移行。テーマトークン（paper/card/ink/sepia/spotify/youtube/err 色、
zine フォント、zine シャドウ）を tailwind.config.js に定義。
view! のクラス置換は次タスクで実施。"
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

- [ ] **Step 3: ページ背景・ベース色を `body` に当てる**

`Home` コンポーネントは body スタイルを直接当てられんけぇ、Tailwind の `@layer base` で対応する。`style/tailwind.css` の `@tailwind base;` の直下に追加する。

`style/tailwind.css` 全体を以下に書き換える:

```css
@tailwind base;
@tailwind components;
@tailwind utilities;

@layer base {
  body {
    @apply bg-paper text-ink font-sans;
    margin: 0;
    padding: 1rem;
  }
  main {
    max-width: 1200px;
    margin: 0 auto;
  }
}
```

`font-sans` は Tailwind デフォルト（system-ui を含む）。本文はサンセリフ、見出しだけ `font-zine` で上書きする。

- [ ] **Step 4: 開発サーバで確認**

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

- [ ] **Step 5: コミット**

```bash
git add src/pages/home.rs style/tailwind.css
git commit -m "feat(home): apply ZINE Tailwind utilities to view!

クラフト紙背景・セリフ体タイトル・グリッド（スマホ2列／タブ3列／PC4列）
・カード形状・ボタン（既存意味色維持）に書き換え。サブコピーと
.lede 削除。傾きは次タスクで .tilt-cycle として追加。"
```

---

## Task 3: 4枚周期の傾きとマイクロインタラクションを CSS に追加

**Files:**
- Modify: `style/tailwind.css`（Task 2 で書いた内容に `@layer components` ブロックを追加）

Tailwind の `nth-child` ベースの周期回転は arbitrary group / variant では表現しにくいため、`@layer components` で書く。

- [ ] **Step 1: `style/tailwind.css` に component layer を追加**

`style/tailwind.css` 末尾（`@layer base { … }` の後ろ）に追加:

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
