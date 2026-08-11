use yew::prelude::*;

#[function_component(Login)]
pub fn login() -> Html {
    html! {
        <div class="login-container">
            <div class="login-card">
                <h1 class="login-title">{ "Cosmic Frontier" }</h1>
                <p class="login-subtitle">{ "Action Tracker" }</p>
                <p>{ "Sign in to continue" }</p>
                <a href="/auth/login" class="login-button">
                    { "Sign in with Google" }
                </a>
            </div>
        </div>
    }
}
