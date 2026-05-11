# i-am-rockin-on

音楽メディア（まずは「ロキノンには騙されないぞ」）の「推し」記事を集約し、Spotify ジャケットとリンクを並べる Web サイト。

## 開発

前提: [mise](https://mise.jdx.dev/) がインストール済みであること。

```bash
# ツールチェーンと cargo-leptos / sqlx-cli を用意
mise install
mise run setup     # wasm32-unknown-unknown target を追加

# DB 準備
echo 'DATABASE_URL=sqlite:data/app.db' > .env
mise run db:create
mise run db:migrate

# Spotify creds（https://developer.spotify.com/ で取得）
echo 'SPOTIFY_CLIENT_ID=...' >> .env
echo 'SPOTIFY_CLIENT_SECRET=...' >> .env

# 開発サーバ
mise run dev

# 手動スクレイプ（既定: rokinon）
mise run scrape
# 別ソース指定: cargo run --features ssr --bin scrape -- --source pitchfork

# テスト (Rust)
mise run test

# 視覚回帰テスト (Playwright、 home の grid layout を 3 viewport で検証)
(cd tests/visual && npm ci && npx playwright install chromium)
mise run visual
```

## ソース

現在の取り込み元:
- **rokinon** — Ameblo の「ロキノンには騙されないぞ」RSS から `YYYYMM推し` 記事を抽出
- **pitchfork** — Pitchfork のアルバムレビューから直近 `PITCHFORK_RECENCY_DAYS` 日 ・ score `PITCHFORK_SCORE_THRESHOLD` 以上を抽出

新メディアを足す場合は `MediaSource` trait を `impl` し、 `main.rs` で `add_scrape_job` を 1 行追加する。

## 環境変数

| 変数 | 必須 | 既定 | 用途 |
|---|---|---|---|
| `DATABASE_URL` | yes | — | SQLite 接続先 (例: `sqlite:data/app.db`) |
| `SPOTIFY_CLIENT_ID` | yes | — | Spotify Web API |
| `SPOTIFY_CLIENT_SECRET` | yes | — | Spotify Web API |
| `PITCHFORK_SCORE_THRESHOLD` | no | `8.0` | Pitchfork 取り込みの下限スコア |
| `PITCHFORK_RECENCY_DAYS` | no | `90` | Pitchfork 取り込みの直近日数 |
| `PITCHFORK_MAX_PAGES` | no | `3` | Pitchfork index ページネーション上限 |
| `SCRAPE_THROTTLE_MS` | no | `800` | 候補処理の合間に挟む sleep。 `0` で skip |
| `DISABLE_SCRAPE` | no | — | `1` で initial scrape と定期スケジューラを抑止。 視覚回帰テスト向けの knob |

## ヘルスチェック

- `GET /healthz` — プロセス生存のみ確認、 fly のヘルスチェックはこちらを向ける
- `GET /readyz` — DB に `SELECT 1` を打って疎通確認、 外部監視向け。 失敗時 `503`

## デプロイ

`docs/deploy.md` を参照。

## 開発フロー

`main` は GitHub の保護ブランチ設定下にあり、 直 push は block される。 docs だけの変更も含めて feature branch を切り、 PR 経由で auto-merge する。

## 設計

- 設計書: `docs/superpowers/specs/2026-05-08-music-recommendations-aggregator-design.md`
- 実装プラン: `docs/superpowers/plans/2026-05-08-music-recommendations-aggregator.md`
