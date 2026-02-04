use yew::prelude::*;

#[function_component(Login)]
pub fn login() -> Html {
    html! {
        <div class="login-container">
            <div class="login-card">
                <h1>{ "Action Tracker" }</h1>
                <p>{ "Sign in to continue" }</p>
                <a href="/auth/login" class="login-button">
                    { "Sign in with Google" }
                </a>
            </div>
        </div>
    }
}
