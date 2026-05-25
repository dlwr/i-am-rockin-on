-- 記事ごとの処理済み追跡。非推し記事も含めて記録し、entrylist 走査での re-fetch を防ぐ。
CREATE TABLE scraped_entries (
    source_id   TEXT NOT NULL,
    external_id TEXT NOT NULL,
    scraped_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (source_id, external_id)
);

-- bootstrap: 既存 recommendations を処理済みとして登録。
-- これを忘れると初回 cron で既知の推し記事まで全件 re-fetch + Spotify 再解決の嵐になる。
INSERT OR IGNORE INTO scraped_entries (source_id, external_id, scraped_at)
SELECT source_id, source_external_id, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
FROM recommendations;
