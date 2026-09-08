//! Read-only vendor portal.
//!
//! Rendered outside the staff app: it never mounts the router, header or any
//! `/api/*` call, so a vendor session cannot accidentally reach staff data. The
//! component has two states — request a link, or view the vendor's open items —
//! chosen by whether `/vendor/api/me` accepts the session cookie.

use gloo_net::http::Request;
use shared::{VendorPortalItem, VendorPortalMe};
use wasm_bindgen::JsCast;
use web_sys::HtmlInputElement;
use yew::prelude::*;

use crate::pages::status_style::{priority_class, status_class};

/// Read `?error=` off the current URL, so `/vendor/verify` can explain why it
/// bounced the visitor back here.
fn link_error_message() -> Option<String> {
    let search = web_sys::window()?.location().search().ok()?;
    if search.contains("error=invalid_link") {
        Some(
            "That sign-in link is no longer valid. Links expire after 15 minutes \
             and can only be used once — request a new one below."
                .to_string(),
        )
    } else {
        None
    }
}

#[function_component(VendorPortal)]
pub fn vendor_portal() -> Html {
    let session = use_state(|| None::<Option<VendorPortalMe>>);
    let items = use_state(Vec::<VendorPortalItem>::new);

    // Resolve the session once on mount. `None` = still checking, `Some(None)` =
    // no session, `Some(Some(..))` = signed in.
    {
        let session = session.clone();
        let items = items.clone();
        use_effect_with((), move |_| {
            wasm_bindgen_futures::spawn_local(async move {
                match Request::get("/vendor/api/me").send().await {
                    Ok(resp) if resp.ok() => {
                        let me = resp.json::<VendorPortalMe>().await.ok();
                        if me.is_some() {
                            if let Ok(resp) = Request::get("/vendor/api/items").send().await {
                                if let Ok(data) = resp.json::<Vec<VendorPortalItem>>().await {
                                    items.set(data);
                                }
                            }
                        }
                        session.set(Some(me));
                    }
                    _ => session.set(Some(None)),
                }
            });
            || ()
        });
    }

    match &*session {
        None => html! {
            <div class="login-container">
                <div class="login-card"><p>{ "Loading..." }</p></div>
            </div>
        },
        Some(None) => html! { <RequestLinkForm /> },
        Some(Some(me)) => html! { <OpenItems me={me.clone()} items={(*items).clone()} /> },
    }
}

// ============================================================================
// Request a link
// ============================================================================

#[function_component(RequestLinkForm)]
fn request_link_form() -> Html {
    let email = use_state(String::new);
    let submitting = use_state(|| false);
    let sent = use_state(|| false);
    let link_error = use_state(link_error_message);

    let on_email_input = {
        let email = email.clone();
        Callback::from(move |e: InputEvent| {
            let input: HtmlInputElement = e.target().unwrap().dyn_into().unwrap();
            email.set(input.value());
        })
    };

    let on_submit = {
        let email = email.clone();
        let submitting = submitting.clone();
        let sent = sent.clone();
        let link_error = link_error.clone();

        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();

            let address = (*email).trim().to_string();
            if address.is_empty() {
                return;
            }

            let submitting = submitting.clone();
            let sent = sent.clone();
            let link_error = link_error.clone();
            submitting.set(true);
            link_error.set(None);

            wasm_bindgen_futures::spawn_local(async move {
                let body = serde_json::json!({ "email": address });
                let _ = Request::post("/vendor/request-link")
                    .header("Content-Type", "application/json")
                    .body(body.to_string())
                    .unwrap()
                    .send()
                    .await;

                // The server answers identically for known and unknown addresses,
                // so there is nothing to branch on here by design.
                submitting.set(false);
                sent.set(true);
            });
        })
    };

    html! {
        <div class="login-container">
            <div class="login-card">
                <h1 class="login-title">{ "Cosmic Frontier" }</h1>
                <p class="login-subtitle">{ "Vendor Portal" }</p>

                if let Some(err) = (*link_error).clone() {
                    <p class="error">{ err }</p>
                }

                if *sent {
                    <p>{ "If that address is registered, a sign-in link is on its way. \
                          It expires in 15 minutes." }</p>
                } else {
                    <p>{ "Enter your work email to get a link to your organisation's open items." }</p>
                    <form onsubmit={on_submit}>
                        <div class="form-group">
                            <label>{ "Work email" }</label>
                            <input
                                type="email"
                                placeholder="you@yourcompany.com"
                                value={(*email).clone()}
                                oninput={on_email_input}
                                required=true
                            />
                        </div>
                        <button type="submit" class="btn btn-primary" disabled={*submitting}>
                            { if *submitting { "Sending..." } else { "Email me a link" } }
                        </button>
                    </form>
                }
            </div>
        </div>
    }
}

// ============================================================================
// Open items
// ============================================================================

#[derive(Properties, PartialEq)]
struct OpenItemsProps {
    me: VendorPortalMe,
    items: Vec<VendorPortalItem>,
}

#[function_component(OpenItems)]
fn open_items(props: &OpenItemsProps) -> Html {
    let signing_out = use_state(|| false);

    let on_sign_out = {
        let signing_out = signing_out.clone();
        Callback::from(move |_: MouseEvent| {
            let signing_out = signing_out.clone();
            signing_out.set(true);
            wasm_bindgen_futures::spawn_local(async move {
                let _ = Request::post("/vendor/logout").send().await;
                if let Some(w) = web_sys::window() {
                    let _ = w.location().assign("/vendor");
                }
            });
        })
    };

    html! {
        <div class="container">
            <header class="header">
                <nav>
                    <h1>
                        { &props.me.vendor_name }
                        <span class="header-subtitle">{ " Open Items" }</span>
                    </h1>
                    <button class="btn-logout" onclick={on_sign_out} disabled={*signing_out}>
                        { if *signing_out { "Signing out..." } else { "Sign out" } }
                    </button>
                </nav>
            </header>

            <main>
                <div class="page-header">
                    <h2>{ format!("{} open item(s)", props.items.len()) }</h2>
                    <p class="vendor-portal-viewer">{ format!("Viewing as {}", props.me.email) }</p>
                </div>

                if props.items.is_empty() {
                    <p>{ "Nothing open right now." }</p>
                } else {
                    <table class="table items-table">
                        <thead>
                            <tr>
                                <th>{ "ID" }</th>
                                <th>{ "Title" }</th>
                                <th>{ "Category" }</th>
                                <th>{ "Priority" }</th>
                                <th>{ "Status" }</th>
                                <th>{ "Due" }</th>
                            </tr>
                        </thead>
                        <tbody>
                            { for props.items.iter().map(|item| html! {
                                <tr key={item.id.clone()}>
                                    <td>{ &item.id }</td>
                                    <td class="item-title">{ &item.title }</td>
                                    <td>{ &item.category }</td>
                                    <td>
                                        <span class={priority_class(&item.priority)}>
                                            { &item.priority }
                                        </span>
                                    </td>
                                    <td>
                                        <span class={status_class(&item.status)}>
                                            { &item.status }
                                        </span>
                                    </td>
                                    <td>
                                        { item.due_date
                                            .map(|d| d.format("%Y-%m-%d").to_string())
                                            .unwrap_or_else(|| "-".to_string()) }
                                    </td>
                                </tr>
                            })}
                        </tbody>
                    </table>
                }

                <p class="vendor-portal-footer">
                    { "This is a read-only view. Reply to your usual contact to add or update an item." }
                </p>
            </main>
        </div>
    }
}
