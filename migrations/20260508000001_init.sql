CREATE TABLE recommendations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id TEXT NOT NULL,
    source_url TEXT NOT NULL,
    source_external_id TEXT NOT NULL,
    featured_at TEXT NOT NULL,
    artist_name TEXT NOT NULL,
    album_name TEXT,
    track_name TEXT,
    spotify_url TEXT,
    spotify_image_url TEXT,
    youtube_url TEXT,
    manual_override INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE (source_id, source_external_id)
);
CREATE INDEX idx_recommendations_featured_at ON recommendations (featured_at DESC);

CREATE TABLE scrape_runs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id TEXT NOT NULL,
    started_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    finished_at TEXT,
    status TEXT NOT NULL,
    items_added INTEGER NOT NULL DEFAULT 0,
    items_updated INTEGER NOT NULL DEFAULT 0,
    error_message TEXT
);
