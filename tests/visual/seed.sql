-- Seed 5 rows producing 4 cards for the "with data" visual regression case.
-- All rows use NULL spotify_image_url so the placeholder render path runs
-- without external image fetches (deterministic in CI).
--
-- Aldous Harding の Train on the Island は 2 source (rokinon + pitchfork) +
-- youtube_url 持ちで、 mobile (375px) で grid-cols-2 の左カラムに行き、
-- Spotify + YouTube + 記事 の 3 pill が flex-wrap で折り返すケースを再現する。
-- これで SourceMenu の <details> が card 左端に来て dropdown の右-anchor が
-- 画面外へ滑り落ちるバグを露呈させる。
INSERT INTO recommendations
  (source_id, source_url, source_external_id, featured_at, artist_name, album_name, spotify_url, spotify_image_url, youtube_url)
VALUES
  ('rokinon',   'https://example.com/r/1', 'visual-r-1', '2026-05-01', 'Aldous Harding',  'Train on the Island', 'https://open.spotify.com/album/visual1', NULL, 'https://www.youtube.com/watch?v=visual-aldous'),
  ('pitchfork', 'https://example.com/p/1', 'visual-p-1', '2026-04-30', 'Aldous Harding',  'Train on the Island', 'https://open.spotify.com/album/visual1', NULL, NULL),
  ('rokinon',   'https://example.com/r/2', 'visual-r-2', '2026-04-15', 'Phoebe Bridgers', 'Punisher',            'https://open.spotify.com/album/visual2', NULL, NULL),
  ('pitchfork', 'https://example.com/p/3', 'visual-p-3', '2026-04-01', 'Bon Iver',        '22, A Million',       'https://open.spotify.com/album/visual3', NULL, NULL),
  ('rokinon',   'https://example.com/r/4', 'visual-r-4', '2026-03-20', 'Big Thief',       'U.F.O.F.',            'https://open.spotify.com/album/visual4', NULL, NULL);
