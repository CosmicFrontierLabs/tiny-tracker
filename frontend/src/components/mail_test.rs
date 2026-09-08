//! Staff-only dialog for sending a test email.
//!
//! Reports the transport's own error text on failure, and — importantly — warns
//! when the configured backend is `log`, which accepts the message without
//! sending it. Without that warning a green result would be misleading.

use gloo_net::http::Request;
use shared::TestEmailResponse;
use wasm_bindgen::JsCast;
use web_sys::HtmlInputElement;
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct MailTestModalProps {
    pub on_close: Callback<()>,
}

#[function_component(MailTestModal)]
pub fn mail_test_modal(props: &MailTestModalProps) -> Html {
    let email = use_state(String::new);
    let sending = use_state(|| false);
    let result = use_state(|| None::<TestEmailResponse>);
    let error = use_state(|| None::<String>);

    let on_email_input = {
        let email = email.clone();
        Callback::from(move |e: InputEvent| {
            let input: HtmlInputElement = e.target().unwrap().dyn_into().unwrap();
            email.set(input.value());
        })
    };

    let on_submit = {
        let email = email.clone();
        let sending = sending.clone();
        let result = result.clone();
        let error = error.clone();

        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();

            let address = (*email).trim().to_string();
            if address.is_empty() {
                return;
            }

            let sending = sending.clone();
            let result = result.clone();
            let error = error.clone();

            sending.set(true);
            result.set(None);
            error.set(None);

            wasm_bindgen_futures::spawn_local(async move {
                let body = serde_json::json!({ "email": address });
                match Request::post("/api/mail/test")
                    .header("Content-Type", "application/json")
                    .body(body.to_string())
                    .unwrap()
                    .send()
                    .await
                {
                    Ok(resp) if resp.ok() => match resp.json::<TestEmailResponse>().await {
                        Ok(data) => result.set(Some(data)),
                        Err(e) => error.set(Some(format!("Unexpected response: {}", e))),
                    },
                    Ok(resp) => {
                        let msg = resp
                            .json::<shared::ApiError>()
                            .await
                            .map(|e| e.error.message)
                            .unwrap_or_else(|_| "Failed to send test email".to_string());
                        error.set(Some(msg));
                    }
                    Err(e) => error.set(Some(format!("Request error: {}", e))),
                }
                sending.set(false);
            });
        })
    };

    let on_backdrop_click = {
        let on_close = props.on_close.clone();
        Callback::from(move |_| on_close.emit(()))
    };

    let on_modal_click = Callback::from(|e: MouseEvent| e.stop_propagation());

    html! {
        <div class="modal-backdrop" onclick={on_backdrop_click}>
            <div class="modal" onclick={on_modal_click}>
                <div class="modal-header">
                    <h2>{ "Send Test Email" }</h2>
                    <button class="modal-close" onclick={
                        let on_close = props.on_close.clone();
                        Callback::from(move |_: MouseEvent| on_close.emit(()))
                    }>{ "\u{00d7}" }</button>
                </div>

                <div class="modal-body">
                    <p class="mail-test-hint">
                        { "Sends a fixed test message using the server's configured mail \
                           transport. Use this to check outbound email before relying on \
                           vendor portal sign-in links." }
                    </p>

                    if let Some(err) = (*error).clone() {
                        <p class="error">{ err }</p>
                    }

                    if let Some(res) = (*result).clone() {
                        if res.delivered {
                            <p class="mail-test-ok">
                                { format!("Sent to {} via {}.", res.recipient, res.backend) }
                            </p>
                        } else {
                            <p class="mail-test-warn">
                                { format!(
                                    "Accepted by the \"{}\" backend, but NOT delivered — it only \
                                     writes messages to the server log. Nothing arrived at {}. \
                                     Set MAIL_BACKEND to smtp or resend to send real email.",
                                    res.backend, res.recipient
                                ) }
                            </p>
                        }
                    }

                    <form onsubmit={on_submit}>
                        <div class="form-group">
                            <label>{ "Send to" }</label>
                            <input
                                type="email"
                                placeholder="you@example.com"
                                value={(*email).clone()}
                                oninput={on_email_input}
                                required=true
                            />
                        </div>
                        <div class="form-actions">
                            <button type="submit" class="btn btn-primary" disabled={*sending}>
                                { if *sending { "Sending..." } else { "Send test email" } }
                            </button>
                        </div>
                    </form>
                </div>
            </div>
        </div>
    }
}
