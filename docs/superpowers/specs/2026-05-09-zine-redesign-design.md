# 設計書: ZINE 風リデザイン（v1）

- 作成日: 2026-05-09
- ステータス: ドラフト

## 1. 目的とスコープ

`/`（一覧ページ）の見た目を、現在のミニマル・ニュートラルな白背景グリッドから、ZINE（手作り音楽雑誌）のテイストに刷新する。同時にスマホで2列固定にしてジャケットを並べて見やすくする。あわせて、将来のページ追加に備えてスタイル基盤を Tailwind CSS に移行する（リデザインと同時実施）。

### スコープ内

- **Tailwind CSS 導入**（cargo-leptos の Tailwind 統合を使用）
- `style/main.css` の廃止 → `style/tailwind.css` ＋ `tailwind.config.js` ＋ `package.metadata.leptos` 設定変更
- `src/pages/home.rs` の view! クラス名を Tailwind utility に書き換え（ロジック・データ構造・Server Function は触らん）
- スマホ2列固定 → タブレット3列 → PC 4列のレスポンシブ
- カードに「貼った紙」感を出す（傾き・ハードシャドウ）
- ホバーで傾きを戻して持ち上げるマイクロインタラクション

### スコープ外

- データモデル変更
- Server Function 変更
- フィルタ／ソート UI 追加
- 詳細ページ追加
- ダークモード切替
- その他 TODO.md に列挙された残タスク（性能・運用・テスト等）

## 2. ビジュアル方向性

「ZINE / Indie」路線（控えめ寄り）。

- クラフト紙風のオフホワイト背景に、白めの紙片を貼り付けたようなカードを並べる
- セリフ体イタリックのタイトルで「手作り雑誌」感
- カードを微妙に傾け、ハードシャドウで紙の浮きを表現
- ホバーで傾き 0° に戻して持ち上がる（紙が手に取られる）

派手な演出（ネオン・パンクスタンプ・★・グラデーション）は採用しない。`# i-am-rockin-on` のロゴ的シニカルさと相性のいい「落ち着いた手仕事感」を狙う。

## 3. カラーパレット

| 用途 | 値 |
|---|---|
| 背景（ページ） | `#f4ecd8` クラフト紙 |
| 紙（カード） | `#fffaf0` |
| 文字（メイン） | `#2a2418` 焦げ茶 |
| 文字（サブ・キャプション） | `#6a5a3a` くすんだ茶 |
| エラー文字 | `#b00020`（既存） |
| Spotify ボタン（アプリ起動） | 背景 `#1db954` ／ 文字 白（既存・媒体識別として維持） |
| Spotify web ボタン（外枠） | 文字・枠線 `#1db954` ／ 透明背景（既存） |
| YouTube ボタン | 背景 `#ff0000` ／ 文字 白（既存） |
| 記事ボタン | 文字・枠線 `#2a2418`（1px）／ 透明背景 |
| エラー文字 (`.error`) | `#b00020`（既存維持） |

## 4. タイポグラフィ

| 用途 | フォント | スタイル |
|---|---|---|
| ページタイトル | Georgia, "Hiragino Mincho ProN", serif | italic / bold / 2rem |
| カード アーティスト名 | Georgia, serif | bold / 0.95rem |
| カード アルバム名 | Georgia, serif | italic / 0.8rem / `#6a5a3a` |
| 月（meta） | system-ui | 0.7rem / `#6a5a3a` |
| ボタン | system-ui | 0.75rem / weight 600 |

本文 system-ui は既存の `system-ui, -apple-system, "Hiragino Kaku Gothic ProN", sans-serif` を流用。

## 5. レイアウト

### 5.1 ヘッダー
```
i am rockin on            ← セリフ体イタリック太字、2rem
══════════════════        ← 二重ボーダー（border-bottom: double 4px #2a2418）
```

サブコピーは入れない。

### 5.2 グリッド breakpoints

| 画面幅 | 列数 |
|---|---|
| 〜599px（スマホ） | **2列固定** |
| 600–999px（タブレット） | 3列 |
| 1000px〜（PC） | 4列 |

`grid-template-columns: repeat(2, 1fr)` を起点にメディアクエリで切替える。`auto-fill, minmax()` は使わん（小さなスマホで1列に潰れるため）。

最大幅は `main { max-width: 1200px }` を維持。

### 5.3 カード構造

```
┌─────────────────┐
│                 │
│   ジャケット      │  ← aspect-ratio: 1/1
│   (1:1)         │
│                 │
├─────────────────┤
│ Artist Name     │  ← セリフ体太字
│ Album Name      │  ← セリフ体イタリック
│ 2026-04         │  ← 月のみ（サブ色）
│ [SP][YT][記事]   │  ← ボタン行
└─────────────────┘
   ↘ 2px 3px 0 #2a2418 （ハードシャドウ）
```

カード内には**媒体タグ（"ROKINON" バッジ）を表示しない**。第1イテレーションでは媒体は1つのみで冗長なため。媒体が複数になった段階で再検討。

ジャケット画像が無い場合（`spotify_image_url is None`）は、同サイズのプレースホルダ枠（`#e8dcc4` ベタ＋センターに `♪` 文字）を表示。空きで崩れんようにする。

### 5.4 カードの傾き

4枚周期で繰り返す回転角度を CSS の `:nth-child(4n+k)` で適用。

| nth-child | 角度 |
|---|---|
| 4n+1 | -0.7deg |
| 4n+2 | +0.5deg |
| 4n+3 | +0.3deg |
| 4n+4 | -0.5deg |

### 5.5 マイクロインタラクション

- **hover 時**: `transform: rotate(0deg) translateY(-3px)` ／ `transition: transform 0.2s ease` で傾きが戻り紙が浮く
- **focus-within（キーボード操作）**: 同等の効果を適用しアクセシビリティ確保
- **`prefers-reduced-motion: reduce`**: transition と hover transform を無効化

## 6. 実装変更点

### 6.1 Tailwind 導入

cargo-leptos の Tailwind 統合を使う。Node.js 不要で、cargo-leptos が `tailwindcss` バイナリを自動取得する。

**`Cargo.toml` の `[package.metadata.leptos]` 変更**
- 削除: `style-file = "style/main.css"`
- 追加: `tailwind-input-file = "style/tailwind.css"`
- 追加: `tailwind-config-file = "tailwind.config.js"`

**新規ファイル `tailwind.config.js`**
```js
module.exports = {
  content: ["./src/**/*.rs"],
  theme: {
    extend: {
      colors: {
        paper: "#f4ecd8",       // クラフト紙背景
        card: "#fffaf0",         // カード紙
        ink: "#2a2418",          // 文字メイン
        sepia: "#6a5a3a",        // サブ文字
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
      rotate: {
        "tilt-1": "-0.7deg",
        "tilt-2": "0.5deg",
        "tilt-3": "0.3deg",
        "tilt-4": "-0.5deg",
      },
    },
  },
};
```

**新規ファイル `style/tailwind.css`**
```css
@tailwind base;
@tailwind components;
@tailwind utilities;

/* 4枚周期の傾きを nth-child で当てる（utility では表現しにくい） */
@layer components {
  .tilt-cycle > :nth-child(4n+1) { transform: rotate(-0.7deg); }
  .tilt-cycle > :nth-child(4n+2) { transform: rotate(0.5deg); }
  .tilt-cycle > :nth-child(4n+3) { transform: rotate(0.3deg); }
  .tilt-cycle > :nth-child(4n+4) { transform: rotate(-0.5deg); }
  .tilt-cycle > * { transition: transform 0.2s ease; }
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

**削除するファイル**: `style/main.css`

### 6.2 `src/pages/home.rs`

- `view!` 内のクラス名を Tailwind utility に書き換える（例: `class="grid"` → `class="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-4 gap-5 tilt-cycle"`）
- ヘッダ: `<h1 class="font-zine italic font-bold text-3xl border-b-4 border-double border-ink pb-2 mb-6">…</h1>`
- `<p class="lede">…</p>` 行を削除（サブコピー廃止）
- カード: `class="bg-card shadow-zine p-3 flex flex-col gap-2"` 等
- ボタン: `class="bg-spotify text-white text-xs px-2 py-1 rounded-full"` 等
- ジャケ画像フォールバック分岐を追加（None の時にプレースホルダ要素を出す）
- `RecommendationView` は触らん

Tailwind の `content: ["./src/**/*.rs"]` で `view!` 内の文字列リテラルがスキャンされるけぇ、クラス名を文字列で渡しとる限り検出される。動的クラス組み立てはせん（safelist 不要）。

### 6.3 ビルド・運用

- ローカル: `cargo leptos watch` がそのまま動く（cargo-leptos が tailwind バイナリを fetch）
- Docker / fly.io: cargo-leptos のビルドステップで tailwind が走る。`Dockerfile` の cargo-leptos 設定変更は不要
- 既存の `style/main.css` への参照（README やテストなど）は無いことを実装時に確認する

### 6.4 触らないファイル

- `src/server/*`
- `src/domain/*`
- `src/bin/*`
- `migrations/*`
- `assets/*`
- ロジック・テスト
- `mise.toml`（cargo-leptos のタスク経由でビルドが走るため変更不要）

## 7. テスト

CSS は単体テスト対象外。`home.rs` のロジック（`spotify_app_uri` ）は既存テストで担保。

手動確認項目（実装プラン側で別途検証する）:
- スマホ幅（375px）で2列、タブレット幅（768px）で3列、PC幅（1280px）で4列になる
- ジャケ画像なしのレコードでプレースホルダが出る
- ホバーで傾きが戻り浮く
- `prefers-reduced-motion` 設定下でアニメが止まる

## 8. リスク・考慮事項

- **傾けたカードのレイアウト崩れ**: `transform: rotate` は親要素の幅・高さに影響せんため、グリッド枠は崩れん。隣接カードに視覚的に重なる可能性は角度を 1° 未満に抑えて回避
- **ハードシャドウの色とアクセシビリティ**: `#2a2418` シャドウは紙背景とのコントラスト比で問題なし
- **セリフ体の日本語**: アーティスト名・アルバム名はほぼ英語想定だが、邦楽が入った場合に "Hiragino Mincho ProN" にフォールバックする
- **第2イテレーションで媒体タグを戻す可能性**: 複数媒体対応時に再導入。その時はカード下部 meta-row に `[月] [媒体]` で並べる方針を踏襲する想定
- **Tailwind 初回ビルド時間**: cargo-leptos が初回に tailwindcss バイナリを fetch（数MB、数秒）。CI / Docker ビルドで初回だけ伸びるが、レイヤキャッシュで2回目以降は影響なし
- **fly.io での Tailwind 実行**: Docker のマルチステージビルド内で cargo-leptos がバイナリ取得・実行。glibc / musl の互換性に注意（cargo-leptos が自動で適切なターゲットを選ぶ想定）。実装時に `fly deploy` で1回確認する

## 9. 関連

- 既存設計: `docs/superpowers/specs/2026-05-08-music-recommendations-aggregator-design.md`
- 対象ファイル: `Cargo.toml`（leptos metadata）, `style/tailwind.css`（新規）, `tailwind.config.js`（新規）, `src/pages/home.rs`
- 削除ファイル: `style/main.css`
