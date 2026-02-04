use yew::prelude::*;
use yew_router::prelude::*;

use crate::Route;

#[function_component(Header)]
pub fn header() -> Html {
    html! {
        <header class="header">
            <nav>
                <Link<Route> to={Route::Home}>
                    <h1>{ "Action Tracker" }</h1>
                </Link<Route>>
            </nav>
        </header>
    }
}
