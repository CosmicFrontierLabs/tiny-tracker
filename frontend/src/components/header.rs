use gloo_net::http::Request;
use web_sys::window;
use yew::prelude::*;
use yew_router::prelude::*;

use crate::Route;

#[function_component(Header)]
pub fn header() -> Html {
    let logging_out = use_state(|| false);

    let on_logout = {
        let logging_out = logging_out.clone();
        Callback::from(move |_: MouseEvent| {
            let logging_out = logging_out.clone();
            logging_out.set(true);
            wasm_bindgen_futures::spawn_local(async move {
                let _ = Request::post("/auth/logout").send().await;
                if let Some(w) = window() {
                    let _ = w.location().reload();
                }
            });
        })
    };

    html! {
        <header class="header">
            <nav>
                <Link<Route> to={Route::Home}>
                    <h1>{ "Action Tracker" }</h1>
                </Link<Route>>
                <button class="btn-logout" onclick={on_logout} disabled={*logging_out}>
                    { if *logging_out { "Logging out..." } else { "Logout" } }
                </button>
            </nav>
        </header>
    }
}
