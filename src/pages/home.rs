use leptos::prelude::*;

/// ジャケ画像の `alt` テキストを組み立てる。 album があれば "Artist - Album"、
/// 無い／空白のみなら artist のみ。 末尾の "- " 残りを避けるため album のトリム判定する。
fn image_alt(artist: &str, album: Option<&str>) -> String {
    match album {
        Some(a) if !a.trim().is_empty() => format!("{artist} - {a}"),
        _ => artist.to_string(),
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
    fn image_alt_combines_artist_and_album() {
        assert_eq!(
            image_alt("Aldous Harding", Some("Train on the Island")),
            "Aldous Harding - Train on the Island",
        );
    }

    #[test]
    fn image_alt_uses_only_artist_when_album_is_none() {
        assert_eq!(image_alt("Bon Iver", None), "Bon Iver");
    }

    #[test]
    fn image_alt_uses_only_artist_when_album_is_blank() {
        // album が空文字や空白のみの時に "Artist - " と末尾ダッシュが残らんよう
        assert_eq!(image_alt("Phoebe Bridgers", Some("")), "Phoebe Bridgers");
        assert_eq!(image_alt("Phoebe Bridgers", Some("   ")), "Phoebe Bridgers");
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

    #[cfg(feature = "ssr")]
    #[test]
    fn selector_card_view_from_domain_formats_added_at_as_yyyy_mm_dd() {
        use chrono::{TimeZone, Utc};
        use crate::domain::selector_card::SelectorCard;

        let card = SelectorCard {
            artist_name: "Aldous Harding".into(),
            album_name: Some("Train on the Island".into()),
            spotify_url: Some("https://open.spotify.com/album/abc".into()),
            spotify_image_url: None,
            youtube_url: None,
            added_at: Utc.with_ymd_and_hms(2026, 5, 10, 12, 0, 0).unwrap(),
            sources: vec![],
        };
        let view = SelectorCardView::from(card);
        assert_eq!(view.artist_name, "Aldous Harding");
        assert_eq!(view.album_name.as_deref(), Some("Train on the Island"));
        assert_eq!(view.added_at, "2026-05-10");
    }

    #[cfg(feature = "ssr")]
    #[test]
    fn selector_card_view_from_domain_carries_sources() {
        use chrono::{NaiveDate, TimeZone, Utc};
        use crate::domain::album_card::SourceLink;
        use crate::domain::selector_card::SelectorCard;

        let card = SelectorCard {
            artist_name: "Foo".into(),
            album_name: Some("Bar".into()),
            spotify_url: None,
            spotify_image_url: None,
            youtube_url: None,
            added_at: Utc.with_ymd_and_hms(2026, 5, 10, 12, 0, 0).unwrap(),
            sources: vec![
                SourceLink {
                    source_id: "pitchfork".into(),
                    source_url: "https://pitchfork.com/x".into(),
                    featured_at: NaiveDate::from_ymd_opt(2026, 5, 8).unwrap(),
                },
                SourceLink {
                    source_id: "rokinon".into(),
                    source_url: "https://ameblo.jp/y".into(),
                    featured_at: NaiveDate::from_ymd_opt(2026, 5, 1).unwrap(),
                },
            ],
        };
        let view = SelectorCardView::from(card);
        assert_eq!(view.sources.len(), 2);
        assert_eq!(view.sources[0].source_id, "pitchfork");
        assert_eq!(view.sources[0].source_url, "https://pitchfork.com/x");
        assert_eq!(view.sources[1].source_id, "rokinon");
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

#[server(Selector, "/api")]
pub async fn selector() -> Result<Option<SelectorCardView>, ServerFnError> {
    use crate::server::store::RecommendationRepo;
    use chrono::{Duration, Utc};
    use std::sync::Arc;

    let repo = use_context::<Arc<RecommendationRepo>>()
        .ok_or_else(|| ServerFnError::new("repo missing"))?;
    let since = Utc::now() - Duration::days(30);
    let picked = repo
        .pick_recent_addition(since)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    Ok(picked.map(SelectorCardView::from))
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

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SelectorCardView {
    pub artist_name: String,
    pub album_name: Option<String>,
    pub spotify_url: Option<String>,
    pub spotify_image_url: Option<String>,
    pub youtube_url: Option<String>,
    pub added_at: String, // YYYY-MM-DD
    pub sources: Vec<SourceLinkView>,
}

#[cfg(feature = "ssr")]
impl From<crate::domain::selector_card::SelectorCard> for SelectorCardView {
    fn from(c: crate::domain::selector_card::SelectorCard) -> Self {
        Self {
            artist_name: c.artist_name,
            album_name: c.album_name,
            spotify_url: c.spotify_url,
            spotify_image_url: c.spotify_image_url,
            youtube_url: c.youtube_url,
            added_at: c.added_at.format("%Y-%m-%d").to_string(),
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
    let selector_action = Action::new(|_: &()| async { selector().await });
    let selector_pending = selector_action.pending();

    view! {
        <header class="flex items-baseline justify-between border-b-4 border-double border-ink pb-2 mb-6">
            <h1 class="font-zine italic font-bold text-3xl text-ink m-0">
                "i am rockin on"
            </h1>
            <button
                class="font-zine font-bold text-sm px-3 py-1.5 bg-ink text-paper border border-ink cursor-pointer hover:bg-paper hover:text-ink disabled:opacity-50 disabled:cursor-not-allowed"
                disabled=move || selector_pending.get()
                on:click=move |_| { selector_action.dispatch(()); }
            >
                "Selector"
            </button>
        </header>
        <SelectorSlot action=selector_action/>
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
fn SelectorSlot(action: Action<(), Result<Option<SelectorCardView>, ServerFnError>>) -> impl IntoView {
    let pending = action.pending();
    let value = action.value();

    view! {
        {move || {
            if pending.get() {
                view! {
                    <section class="my-6">
                        <p class="font-zine italic text-sepia">"選んどるよ…"</p>
                    </section>
                }.into_any()
            } else {
                match value.get() {
                    None => ().into_any(),
                    Some(Ok(None)) => view! {
                        <section class="my-6">
                            <p class="font-zine italic text-sepia">
                                "直近1ヶ月で追加された一枚はまだないずら"
                            </p>
                        </section>
                    }.into_any(),
                    Some(Ok(Some(card))) => view! {
                        <section class="my-6">
                            <SelectorPick card=card.clone()/>
                            <button
                                class="mt-3 font-zine text-xs px-2.5 py-1 border border-ink text-ink cursor-pointer hover:bg-ink hover:text-paper disabled:opacity-50 disabled:cursor-not-allowed"
                                disabled=move || pending.get()
                                on:click=move |_| { action.dispatch(()); }
                            >
                                "もう一度"
                            </button>
                        </section>
                    }.into_any(),
                    Some(Err(e)) => view! {
                        <p class="text-err">{format!("error: {e}")}</p>
                    }.into_any(),
                }
            }
        }}
    }
}

#[component]
fn SelectorPick(card: SelectorCardView) -> impl IntoView {
    let alt = image_alt(&card.artist_name, card.album_name.as_deref());
    view! {
        <article class="bg-card shadow-zine p-4 max-w-md flex flex-col gap-3">
            {match card.spotify_image_url.as_ref() {
                Some(src) => view! {
                    <img
                        class="w-full aspect-square object-cover bg-paper"
                        src=src.clone()
                        alt=alt
                        loading="lazy"
                    />
                }.into_any(),
                None => view! {
                    <div
                        class="w-full aspect-square bg-placeholder flex items-center justify-center text-sepia text-6xl font-zine"
                        aria-hidden="true"
                    >"♪"</div>
                }.into_any(),
            }}
            <div class="flex flex-col gap-1">
                <div class="font-zine font-bold text-lg text-ink leading-tight">
                    {card.artist_name.clone()}
                </div>
                {card.album_name.clone().map(|a| view! {
                    <div class="font-zine italic text-base text-sepia leading-tight">{a}</div>
                })}
            </div>
            <div class="flex flex-wrap gap-2 items-center">
                {card.spotify_url.clone().map(|u| view! {
                    <a
                        class="text-xs font-semibold px-2.5 py-1 rounded-full bg-spotify text-white no-underline"
                        href=u
                        target="_blank"
                        rel="noopener"
                    >"Spotify"</a>
                })}
                {card.youtube_url.clone().map(|u| view! {
                    <a
                        class="text-xs font-semibold px-2.5 py-1 rounded-full bg-youtube text-white no-underline"
                        href=u
                        target="_blank"
                        rel="noopener"
                    >"YouTube"</a>
                })}
                <SourceMenu sources=card.sources.clone()/>
                <span class="ml-auto text-[0.7rem] text-sepia">{card.added_at.clone()}</span>
            </div>
        </article>
    }
}

#[component]
fn AlbumGrid(items: Vec<AlbumCardView>) -> impl IntoView {
    if items.is_empty() {
        return view! {
            <p class="text-sepia font-zine italic text-center my-12">
                "まだ推しが集まっとらんずら"
            </p>
        }
        .into_any();
    }
    view! {
        <ul class="tilt-cycle list-none p-0 m-0 grid grid-cols-2 tab:grid-cols-3 pc:grid-cols-4 gap-5">
            {items.into_iter().map(|item| view! {
                <li class="bg-card shadow-zine p-3 flex flex-col gap-2">
                    {match item.spotify_image_url.as_ref() {
                        Some(src) => view! {
                            <img
                                class="w-full aspect-square object-cover bg-paper"
                                src=src.clone()
                                alt=image_alt(&item.artist_name, item.album_name.as_deref())
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
                                href=u
                                target="_blank"
                                rel="noopener"
                            >"Spotify"</a>
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
    .into_any()
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
        <details class="relative group">
            <summary
                class="text-xs font-semibold px-2.5 py-1 rounded-full border border-ink text-ink cursor-pointer list-none select-none group-hover:bg-ink group-hover:text-paper"
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
