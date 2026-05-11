-- Seed 3 rows for the "with data" visual regression case.
-- All rows use NULL spotify_image_url so the placeholder render path runs
-- without external image fetches (deterministic in CI).
INSERT INTO recommendations
  (source_id, source_url, source_external_id, featured_at, artist_name, album_name, spotify_url, spotify_image_url)
VALUES
  ('rokinon',   'https://example.com/r/1', 'visual-r-1', '2026-05-01', 'Aldous Harding',  'Train on the Island', 'https://open.spotify.com/album/visual1', NULL),
  ('rokinon',   'https://example.com/r/2', 'visual-r-2', '2026-04-15', 'Phoebe Bridgers', 'Punisher',            'https://open.spotify.com/album/visual2', NULL),
  ('pitchfork', 'https://example.com/p/3', 'visual-p-3', '2026-04-01', 'Bon Iver',        '22, A Million',       'https://open.spotify.com/album/visual3', NULL);
