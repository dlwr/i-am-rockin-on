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

/// `source_id` を表示用ラベルに写像する。未知の id はそのまま返す。
fn source_label(source_id: &str) -> &str {
    match source_id {
        "rokinon" => "ロキノン",
        "pitchfork" => "Pitchfork",
        other => other,
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

    #[test]
    fn source_label_known_ids() {
        assert_eq!(source_label("rokinon"), "ロキノン");
        assert_eq!(source_label("pitchfork"), "Pitchfork");
    }

    #[test]
    fn source_label_unknown_id_passthrough() {
        assert_eq!(source_label("nme"), "nme");
    }
}

#[server(ListAlbums, "/api")]
pub async fn list_albums() -> Result<Vec<AlbumCardView>, ServerFnError> {
    use crate::server::store::RecommendationRepo;
    use std::sync::Arc;
    let repo = use_context::<Arc<RecommendationRepo>>()
        .ok_or_else(|| ServerFnError::new("repo missing"))?;
    let cards = repo
        .list_recent_albums(100)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    Ok(cards.into_iter().map(AlbumCardView::from).collect())
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct AlbumCardView {
    pub artist_name: String,
    pub album_name: Option<String>,
    pub spotify_url: Option<String>,
    pub spotify_image_url: Option<String>,
    pub youtube_url: Option<String>,
    pub featured_at: String,
    pub sources: Vec<SourceLinkView>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SourceLinkView {
    pub source_id: String,
    pub source_url: String,
}

#[cfg(feature = "ssr")]
impl From<crate::domain::album_card::AlbumCard> for AlbumCardView {
    fn from(c: crate::domain::album_card::AlbumCard) -> Self {
        Self {
            artist_name: c.artist_name,
            album_name: c.album_name,
            spotify_url: c.spotify_url,
            spotify_image_url: c.spotify_image_url,
            youtube_url: c.youtube_url,
            featured_at: c.featured_at.format("%Y-%m").to_string(),
            sources: c
                .sources
                .into_iter()
                .map(|s| SourceLinkView {
                    source_id: s.source_id,
                    source_url: s.source_url,
                })
                .collect(),
        }
    }
}

#[component]
pub fn Home() -> impl IntoView {
    let cards = Resource::new(|| (), |_| async { list_albums().await });
    view! {
        <header class="border-b-4 border-double border-ink pb-2 mb-6">
            <h1 class="font-zine italic font-bold text-3xl text-ink m-0">
                "i am rockin on"
            </h1>
        </header>
        <Suspense fallback=|| view! { <p class="text-sepia">"loading..."</p> }>
            {move || cards.get().map(|r| match r {
                Ok(items) => view! { <AlbumGrid items=items/> }.into_any(),
                Err(e) => view! {
                    <p class="text-err">{format!("error: {e}")}</p>
                }.into_any(),
            })}
        </Suspense>
    }
}

#[component]
fn AlbumGrid(items: Vec<AlbumCardView>) -> impl IntoView {
    view! {
        <ul class="tilt-cycle list-none p-0 m-0 grid grid-cols-2 tab:grid-cols-3 pc:grid-cols-4 gap-5">
            {items.into_iter().map(|item| view! {
                <li class="bg-card shadow-zine p-3 flex flex-col gap-2">
                    {match item.spotify_image_url.as_ref() {
                        Some(src) => view! {
                            <img
                                class="w-full aspect-square object-cover bg-paper"
                                src=src.clone()
                                alt=""
                                loading="lazy"
                            />
                        }.into_any(),
                        None => view! {
                            <div
                                class="w-full aspect-square bg-placeholder flex items-center justify-center text-sepia text-4xl font-zine"
                                aria-hidden="true"
                            >"♪"</div>
                        }.into_any(),
                    }}
                    <div class="flex flex-col gap-0.5">
                        <div class="font-zine font-bold text-[0.95rem] text-ink leading-tight">
                            {item.artist_name.clone()}
                        </div>
                        {item.album_name.clone().map(|a| view! {
                            <div class="font-zine italic text-[0.8rem] text-sepia leading-tight">{a}</div>
                        })}
                        <div class="text-[0.7rem] text-sepia mt-1">
                            {item.featured_at.clone()}
                        </div>
                    </div>
                    <div class="flex flex-wrap gap-1.5 mt-auto">
                        {item.spotify_url.clone().map(|u| view! {
                            <a
                                class="text-xs font-semibold px-2.5 py-1 rounded-full bg-spotify text-white no-underline"
                                href=spotify_app_uri(&u)
                            >"Spotify"</a>
                            <a
                                class="text-[0.7rem] font-semibold px-2 py-0.5 rounded-full border border-spotify text-spotify no-underline"
                                href=u
                                target="_blank"
                                rel="noopener"
                                title="Web で開く"
                            >"web"</a>
                        })}
                        {item.youtube_url.clone().map(|u| view! {
                            <a
                                class="text-xs font-semibold px-2.5 py-1 rounded-full bg-youtube text-white no-underline"
                                href=u
                                target="_blank"
                                rel="noopener"
                            >"YouTube"</a>
                        })}
                        <SourceMenu sources=item.sources/>
                    </div>
                </li>
            }).collect_view()}
        </ul>
    }
}

#[component]
fn SourceMenu(sources: Vec<SourceLinkView>) -> impl IntoView {
    if sources.len() <= 1 {
        let only = sources.into_iter().next();
        return view! {
            {only.map(|s| view! {
                <a
                    class="text-xs font-semibold px-2.5 py-1 rounded-full border border-ink text-ink no-underline"
                    href=s.source_url
                    target="_blank"
                    rel="noopener"
                >"記事"</a>
            })}
        }.into_any();
    }
    view! {
        <details class="relative group [&[open]>summary]:bg-ink [&[open]>summary]:text-paper">
            <summary
                class="text-xs font-semibold px-2.5 py-1 rounded-full border border-ink text-ink cursor-pointer list-none select-none group-hover:bg-ink group-hover:text-paper"
                role="button"
                aria-haspopup="true"
            >
                "記事"
            </summary>
            <ul class="absolute right-0 mt-1 z-10 bg-card border border-ink shadow-zine min-w-[10rem] list-none p-1 m-0">
                {sources.into_iter().map(|s| view! {
                    <li>
                        <a
                            class="block px-3 py-1.5 text-xs text-ink no-underline hover:bg-paper"
                            href=s.source_url
                            target="_blank"
                            rel="noopener"
                        >{source_label(&s.source_id).to_string()}</a>
                    </li>
                }).collect_view()}
            </ul>
        </details>
    }.into_any()
}
