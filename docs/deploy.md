# デプロイ手順

## 初回セットアップ

1. `flyctl auth login`
2. `flyctl launch --no-deploy --copy-config --name i-am-rockin-on --region nrt`
3. ボリューム作成: `flyctl volumes create rockin_data --region nrt --size 1`
4. シークレット設定:
   ```
   flyctl secrets set SPOTIFY_CLIENT_ID=xxx SPOTIFY_CLIENT_SECRET=yyy
   ```
5. デプロイ: `flyctl deploy`

## 確認

- `flyctl logs` でログ確認
- 起動初回は `scrape_runs` が空ぃけぇ自動で1回スクレイプが走る
- 以降は JST 04:00（UTC 19:00）に日次

## DB 直接確認

```
flyctl ssh console
sqlite3 /data/app.db
.tables
SELECT count(*) FROM recommendations;
```

## 手動スクレイプ実行

```
flyctl ssh console -C "/app/scrape --source rokinon"
```
