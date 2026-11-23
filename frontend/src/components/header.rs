use gloo_net::http::Request;
use web_sys::window;
use yew::prelude::*;
use yew_router::prelude::*;

use crate::pages::manage_vendors::ManageVendorsModal;
use crate::Route;

#[derive(Properties, PartialEq)]
pub struct HeaderProps {
    /// Fired when the Manage Vendors dialog closes, so the page underneath
    /// can reload its vendor list. Vendors are shared by both registers,
    /// which is why the control lives up here rather than on one page.
    #[prop_or_default]
    pub on_vendors_changed: Callback<()>,
}

#[function_component(Header)]
pub fn header(props: &HeaderProps) -> Html {
    let logging_out = use_state(|| false);
    let show_manage_vendors = use_state(|| false);
    let route = use_route::<Route>();
    let on_risks = matches!(route, Some(Route::Risks) | Some(Route::Risk { .. }));

    let on_manage_vendors = {
        let show_manage_vendors = show_manage_vendors.clone();
        Callback::from(move |_: MouseEvent| show_manage_vendors.set(true))
    };
    let on_manage_vendors_close = {
        let show_manage_vendors = show_manage_vendors.clone();
        let on_vendors_changed = props.on_vendors_changed.clone();
        Callback::from(move |_| {
            show_manage_vendors.set(false);
            on_vendors_changed.emit(());
        })
    };

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
                    <h1>{ "Cosmic Frontier" }<span class="header-subtitle">{ " Action Tracker" }</span></h1>
                </Link<Route>>
                <div class="header-links">
                    <Link<Route> to={Route::Home} classes={classes!("header-link", (!on_risks).then_some("active"))}>
                        { "Action Items" }
                    </Link<Route>>
                    <Link<Route> to={Route::Risks} classes={classes!("header-link", on_risks.then_some("active"))}>
                        { "Risk Register" }
                    </Link<Route>>
                </div>
                <div class="header-tools">
                    <button type="button" class="btn-header" onclick={on_manage_vendors}>
                        { "Manage Vendors" }
                    </button>
                    <button class="btn-header btn-logout" onclick={on_logout} disabled={*logging_out}>
                        { if *logging_out { "Logging out..." } else { "Logout" } }
                    </button>
                </div>
            </nav>
            if *show_manage_vendors {
                <ManageVendorsModal on_close={on_manage_vendors_close} />
            }
        </header>
    }
}
