use crate::server::error::AppResult;
use reqwest::Client;
use serde::Deserialize;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

const TOKEN_URL: &str = "https://accounts.spotify.com/api/token";
const SEARCH_URL: &str = "https://api.spotify.com/v1/search";

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
}

pub struct SpotifyResolver {
    client_id: String,
    client_secret: String,
    http: Client,
    token_url: String,
    search_url: String,
    token: Mutex<Option<(String, Instant)>>,
}

impl SpotifyResolver {
    pub fn new(client_id: String, client_secret: String) -> Self {
        Self {
            client_id,
            client_secret,
            http: Client::builder()
                .timeout(Duration::from_secs(20))
                .build()
                .unwrap(),
            token_url: TOKEN_URL.into(),
            search_url: SEARCH_URL.into(),
            token: Mutex::new(None),
        }
    }

    pub fn with_endpoints(mut self, token_url: String, search_url: String) -> Self {
        self.token_url = token_url;
        self.search_url = search_url;
        self
    }

    async fn access_token(&self) -> AppResult<String> {
        let mut guard = self.token.lock().await;
        if let Some((tok, exp)) = guard.as_ref() {
            if Instant::now() < *exp - Duration::from_secs(30) {
                return Ok(tok.clone());
            }
        }
        let resp = self
            .http
            .post(&self.token_url)
            .basic_auth(&self.client_id, Some(&self.client_secret))
            .form(&[("grant_type", "client_credentials")])
            .send()
            .await?
            .error_for_status()?
            .json::<TokenResponse>()
            .await?;
        let exp = Instant::now() + Duration::from_secs(resp.expires_in);
        *guard = Some((resp.access_token.clone(), exp));
        Ok(resp.access_token)
    }

    pub async fn resolve(&self, artist: &str, album: Option<&str>) -> AppResult<Option<SpotifyMatch>> {
        let token = self.access_token().await?;
        if let Some(album) = album {
            let q = format!("artist:\"{}\" album:\"{}\"", artist, album);
            let resp: AlbumsResp = self
                .http
                .get(&self.search_url)
                .bearer_auth(&token)
                .query(&[("q", q.as_str()), ("type", "album"), ("limit", "1")])
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            if let Some(first) = resp.albums.items.into_iter().next() {
                return Ok(Some(SpotifyMatch {
                    url: first.external_urls.spotify,
                    image_url: first.images.into_iter().next().map(|i| i.url),
                    track_name: None,
                }));
            }
        }
        let q = format!(
            "artist:\"{}\"{}",
            artist,
            album.map(|a| format!(" {}", a)).unwrap_or_default()
        );
        let resp: TracksResp = self
            .http
            .get(&self.search_url)
            .bearer_auth(&token)
            .query(&[("q", q.as_str()), ("type", "track"), ("limit", "1")])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        if let Some(t) = resp.tracks.items.into_iter().next() {
            return Ok(Some(SpotifyMatch {
                url: t.external_urls.spotify,
                image_url: t.album.images.into_iter().next().map(|i| i.url),
                track_name: Some(t.name),
            }));
        }
        Ok(None)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpotifyMatch {
    pub url: String,
    pub image_url: Option<String>,
    pub track_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AlbumsResp { albums: AlbumPage }
#[derive(Debug, Deserialize)]
struct AlbumPage { items: Vec<AlbumItem> }
#[derive(Debug, Deserialize)]
struct AlbumItem {
    external_urls: ExternalUrls,
    images: Vec<Image>,
}
#[derive(Debug, Deserialize)]
struct ExternalUrls { spotify: String }
#[derive(Debug, Deserialize)]
struct Image { url: String }

#[derive(Debug, Deserialize)]
struct TracksResp { tracks: TrackPage }
#[derive(Debug, Deserialize)]
struct TrackPage { items: Vec<TrackItem> }
#[derive(Debug, Deserialize)]
struct TrackItem {
    name: String,
    external_urls: ExternalUrls,
    album: TrackAlbum,
}
#[derive(Debug, Deserialize)]
struct TrackAlbum { images: Vec<Image> }

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        matchers::{method, path, query_param},
        Mock, MockServer, ResponseTemplate,
    };

    // Task 14: token caching
    #[tokio::test]
    async fn access_token_caches_value() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "tok-1", "token_type": "Bearer", "expires_in": 3600
            })))
            .expect(1)
            .mount(&server)
            .await;

        let r = SpotifyResolver::new("id".into(), "secret".into()).with_endpoints(
            format!("{}/token", server.uri()),
            format!("{}/search", server.uri()),
        );
        let t1 = r.access_token().await.unwrap();
        let t2 = r.access_token().await.unwrap();
        assert_eq!(t1, "tok-1");
        assert_eq!(t2, "tok-1");
    }

    // Task 15 tests
    #[tokio::test]
    async fn resolve_returns_album_when_search_hits() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "tok", "token_type": "Bearer", "expires_in": 3600
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/search"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "albums": {
                    "items": [{
                        "external_urls": { "spotify": "https://open.spotify.com/album/abc" },
                        "images": [{ "url": "https://i.scdn.co/image/abc.jpg" }]
                    }]
                }
            })))
            .mount(&server)
            .await;

        let r = SpotifyResolver::new("id".into(), "sec".into()).with_endpoints(
            format!("{}/token", server.uri()),
            format!("{}/search", server.uri()),
        );
        let m = r
            .resolve("Angelo De Augustine", Some("Angel in Plainclothes"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(m.url, "https://open.spotify.com/album/abc");
        assert_eq!(m.image_url.unwrap(), "https://i.scdn.co/image/abc.jpg");
    }

    #[tokio::test]
    async fn resolve_falls_back_to_track_when_album_empty() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "tok", "token_type": "Bearer", "expires_in": 3600
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/search"))
            .and(query_param("type", "album"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "albums": { "items": [] }
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/search"))
            .and(query_param("type", "track"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "tracks": { "items": [{
                    "name": "Some Track",
                    "external_urls": { "spotify": "https://open.spotify.com/track/xyz" },
                    "album": { "images": [{ "url": "https://i.scdn.co/image/xyz.jpg" }] }
                }]}
            })))
            .mount(&server)
            .await;

        let r = SpotifyResolver::new("id".into(), "sec".into()).with_endpoints(
            format!("{}/token", server.uri()),
            format!("{}/search", server.uri()),
        );
        let m = r.resolve("Foo", Some("Bar")).await.unwrap().unwrap();
        assert_eq!(m.url, "https://open.spotify.com/track/xyz");
        assert_eq!(m.track_name.unwrap(), "Some Track");
    }

    #[tokio::test]
    async fn resolve_returns_none_when_nothing_found() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "tok", "token_type": "Bearer", "expires_in": 3600
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/search"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "albums": { "items": [] },
                "tracks": { "items": [] }
            })))
            .mount(&server)
            .await;

        let r = SpotifyResolver::new("id".into(), "sec".into()).with_endpoints(
            format!("{}/token", server.uri()),
            format!("{}/search", server.uri()),
        );
        let m = r.resolve("Nope", Some("Nope")).await.unwrap();
        assert!(m.is_none());
    }
}
