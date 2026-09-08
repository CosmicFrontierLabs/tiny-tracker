//! Banner warning staff when outbound mail is not known to be working.
//!
//! Shows for three of the four states, because the dangerous one is not
//! "failing" — it is **untested**. An unverified sending domain produces a
//! perfectly healthy container that rejects every message, so a portal nobody
//! has tried to send from must not look the same as one that works.

use gloo_net::http::Request;
use shared::{MailHealth, MailStatus};
use yew::prelude::*;

#[function_component(MailBanner)]
pub fn mail_banner() -> Html {
    let status = use_state(|| None::<MailStatus>);

    {
        let status = status.clone();
        use_effect_with((), move |_| {
            wasm_bindgen_futures::spawn_local(async move {
                if let Ok(resp) = Request::get("/api/mail/status").send().await {
                    if let Ok(data) = resp.json::<MailStatus>().await {
                        status.set(Some(data));
                    }
                }
            });
            || ()
        });
    }

    let Some(status) = (*status).clone() else {
        return html! {};
    };

    // Nothing to say when the last real send worked.
    if status.health == MailHealth::Ok {
        return html! {};
    }

    let (class, message) = match status.health {
        MailHealth::NotDelivering => (
            "mail-banner mail-banner-warn",
            format!(
                "Outbound email is not being delivered. The \"{}\" transport writes messages \
                 to the server log instead of sending them, so vendor portal sign-in links \
                 will never arrive. Set MAIL_BACKEND to smtp or resend.",
                status.backend
            ),
        ),
        MailHealth::Untested => (
            "mail-banner mail-banner-warn",
            format!(
                "Outbound email is configured (\"{}\") but has never been tried. A sending \
                 domain that is not yet verified looks exactly like this and rejects every \
                 message. Use Test Email to confirm before relying on vendor links.",
                status.backend
            ),
        ),
        MailHealth::Failing => (
            "mail-banner mail-banner-error",
            format!(
                "The last outbound email failed: {}",
                status
                    .last_error
                    .clone()
                    .unwrap_or_else(|| "unknown error".to_string())
            ),
        ),
        MailHealth::Ok => unreachable!("returned early above"),
    };

    html! { <div class={class}>{ message }</div> }
}
