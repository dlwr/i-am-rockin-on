use leptos::prelude::*;

/// `https://open.spotify.com/{kind}/{id}?...` を `spotify:{kind}:{id}` に変換する。
/// Spotify アプリがインストールされとれば URI スキームで直接アプリが開く。
/// 変換できんかったら元の URL をそのまま返す。
fn spotify_app_uri(web_url: &str) -> String {
    let Some(rest) = web_url.strip_prefix("https://open.spotify.com/") else {
        return web_url.to_string();
    };
    let path = rest.split('?').next().unwrap_or(rest);
    let mut parts = path.splitn(2, '/');
    match (parts.next(), parts.next()) {
        (Some(kind), Some(id)) if !kind.is_empty() && !id.is_empty() => {
            format!("spotify:{kind}:{id}")
        }
        _ => web_url.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spotify_app_uri_converts_album_url() {
        assert_eq!(
            spotify_app_uri("https://open.spotify.com/album/3BU6KQBgOikCUw"),
            "spotify:album:3BU6KQBgOikCUw"
        );
    }

    #[test]
    fn spotify_app_uri_strips_query_string() {
        assert_eq!(
            spotify_app_uri("https://open.spotify.com/track/abc?si=xyz"),
            "spotify:track:abc"
        );
    }

    #[test]
    fn spotify_app_uri_returns_input_for_non_spotify_url() {
        assert_eq!(spotify_app_uri("https://example.com/foo"), "https://example.com/foo");
    }
}

#[server(ListRecommendations, "/api")]
pub async fn list_recommendations() -> Result<Vec<RecommendationView>, ServerFnError> {
    use crate::server::store::RecommendationRepo;
    use std::sync::Arc;
    let repo = use_context::<Arc<RecommendationRepo>>()
        .ok_or_else(|| ServerFnError::new("repo missing"))?;
    let rows = repo
        .list_recent(100)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    Ok(rows.into_iter().map(RecommendationView::from).collect())
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct RecommendationView {
    pub id: i64,
    pub source_id: String,
    pub source_url: String,
    pub featured_at: String,
    pub artist_name: String,
    pub album_name: Option<String>,
    pub spotify_url: Option<String>,
    pub spotify_image_url: Option<String>,
    pub youtube_url: Option<String>,
}

#[cfg(feature = "ssr")]
impl From<crate::domain::recommendation::Recommendation> for RecommendationView {
    fn from(r: crate::domain::recommendation::Recommendation) -> Self {
        Self {
            id: r.id,
            source_id: r.source_id,
            source_url: r.source_url,
            featured_at: r.featured_at.format("%Y-%m").to_string(),
            artist_name: r.artist_name,
            album_name: r.album_name,
            spotify_url: r.spotify_url,
            spotify_image_url: r.spotify_image_url,
            youtube_url: r.youtube_url,
        }
    }
}

#[component]
pub fn Home() -> impl IntoView {
    let recs = Resource::new(|| (), |_| async { list_recommendations().await });
    view! {
        <h1>"I am rockin on"</h1>
        <p class="lede">"音楽メディアの『推し』を集めたページずら"</p>
        <Suspense fallback=|| view! { <p>"loading..."</p> }>
            {move || recs.get().map(|r| match r {
                Ok(items) => view! { <RecommendationGrid items=items/> }.into_any(),
                Err(e) => view! { <p class="error">{format!("error: {e}")}</p> }.into_any(),
            })}
        </Suspense>
    }
}

#[component]
fn RecommendationGrid(items: Vec<RecommendationView>) -> impl IntoView {
    view! {
        <ul class="grid">
            {items.into_iter().map(|item| view! {
                <li class="card">
                    {item.spotify_image_url.as_ref().map(|src| view! {
                        <img src=src.clone() alt="" loading="lazy"/>
                    })}
                    <div class="meta">
                        <div class="artist">{item.artist_name.clone()}</div>
                        {item.album_name.clone().map(|a| view! { <div class="album">{a}</div> })}
                        <div class="featured">{item.featured_at.clone()}</div>
                    </div>
                    <div class="links">
                        {item.spotify_url.clone().map(|u| view! {
                            <a class="btn spotify" href=spotify_app_uri(&u)>"Spotify"</a>
                            <a class="btn spotify-web" href=u target="_blank" rel="noopener" title="Web で開く">"web"</a>
                        })}
                        {item.youtube_url.clone().map(|u| view! {
                            <a class="btn youtube" href=u target="_blank" rel="noopener">"YouTube"</a>
                        })}
                        <a class="btn source" href=item.source_url target="_blank" rel="noopener">"記事"</a>
                    </div>
                </li>
            }).collect_view()}
        </ul>
    }
}
