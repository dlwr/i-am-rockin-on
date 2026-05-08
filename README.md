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

# 手動スクレイプ
mise run scrape

# テスト
mise run test
```

## デプロイ

`docs/deploy.md` を参照。

## 設計

- 設計書: `docs/superpowers/specs/2026-05-08-music-recommendations-aggregator-design.md`
- 実装プラン: `docs/superpowers/plans/2026-05-08-music-recommendations-aggregator.md`
