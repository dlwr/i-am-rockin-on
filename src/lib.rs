use leptos::prelude::*;
use leptos_meta::*;
use leptos_router::{components::*, StaticSegment};

pub mod pages;

#[cfg(feature = "ssr")]
pub mod server;

pub fn shell(options: LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <AutoReload options=options.clone() />
                <HydrationScripts options/>
                <MetaTags/>
            </head>
            <body>
                <App/>
            </body>
        </html>
    }
}

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();
    view! {
        <Stylesheet id="leptos" href="/pkg/i-am-rockin-on.css"/>
        <Title text="I am rockin on"/>
        <Router>
            <main>
                <Routes fallback=|| "Not found.">
                    <Route path=StaticSegment("") view=pages::home::Home/>
                </Routes>
            </main>
        </Router>
    }
}

#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    console_error_panic_hook::set_once();
    leptos::mount::hydrate_body(App);
}
