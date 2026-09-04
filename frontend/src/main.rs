use gloo_net::http::Request;
use yew::prelude::*;
use yew_router::prelude::*;

mod components;
mod pages;

#[derive(Clone, Routable, PartialEq)]
pub enum Route {
    #[at("/")]
    Home,
    #[at("/items/:id")]
    Item { id: String },
    #[not_found]
    #[at("/404")]
    NotFound,
}

fn switch(routes: Route) -> Html {
    match routes {
        Route::Home => html! { <pages::home::Home /> },
        Route::Item { id } => html! { <pages::home::Home initial_item_id={id} /> },
        Route::NotFound => html! { <pages::home::Home /> },
    }
}

/// True when the browser is on a `/vendor` URL.
///
/// The vendor portal is deliberately a separate app root rather than a route in
/// the staff router: it must never call `/auth/me` or mount staff components, so
/// the split happens before any of that runs.
fn is_vendor_portal_path() -> bool {
    web_sys::window()
        .and_then(|w| w.location().pathname().ok())
        .map(|path| path == "/vendor" || path.starts_with("/vendor/"))
        .unwrap_or(false)
}

/// Chooses an app root. Deliberately hook-free so the two roots can differ in
/// which hooks they use.
#[function_component(App)]
fn app() -> Html {
    if is_vendor_portal_path() {
        html! { <pages::vendor_portal::VendorPortal /> }
    } else {
        html! { <StaffApp /> }
    }
}

#[function_component(StaffApp)]
fn staff_app() -> Html {
    let auth_state = use_state(|| None::<bool>);

    {
        let auth_state = auth_state.clone();
        use_effect_with((), move |_| {
            wasm_bindgen_futures::spawn_local(async move {
                match Request::get("/auth/me").send().await {
                    Ok(resp) if resp.ok() => auth_state.set(Some(true)),
                    _ => auth_state.set(Some(false)),
                }
            });
            || ()
        });
    }

    match *auth_state {
        None => html! {
            <div class="login-container">
                <div class="login-card">
                    <p>{ "Loading..." }</p>
                </div>
            </div>
        },
        Some(false) => html! { <pages::login::Login /> },
        Some(true) => html! {
            <BrowserRouter>
                <div class="container">
                    <Switch<Route> render={switch} />
                </div>
            </BrowserRouter>
        },
    }
}

fn main() {
    wasm_logger::init(wasm_logger::Config::default());
    yew::Renderer::<App>::new().render();
}
