//! Editor for the email domains allowed to request a vendor portal link.
//!
//! Matching is exact, so `acme.com` does not admit `someone@mail.acme.com`; each
//! subdomain that should have access must be listed separately.

use gloo_net::http::Request;
use shared::VendorAllowedDomainResponse;
use wasm_bindgen::JsCast;
use web_sys::HtmlInputElement;
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct VendorDomainsProps {
    pub vendor_id: i32,
}

#[function_component(VendorDomains)]
pub fn vendor_domains(props: &VendorDomainsProps) -> Html {
    let domains = use_state(Vec::<VendorAllowedDomainResponse>::new);
    let loading = use_state(|| true);
    let error = use_state(|| None::<String>);
    let new_domain = use_state(String::new);
    let submitting = use_state(|| false);
    let refresh = use_state(|| 0u32);

    {
        let domains = domains.clone();
        let loading = loading.clone();
        let vendor_id = props.vendor_id;
        use_effect_with((vendor_id, *refresh), move |_| {
            wasm_bindgen_futures::spawn_local(async move {
                let url = format!("/api/vendors/{}/allowed-domains", vendor_id);
                if let Ok(resp) = Request::get(&url).send().await {
                    if let Ok(data) = resp.json::<Vec<VendorAllowedDomainResponse>>().await {
                        domains.set(data);
                    }
                }
                loading.set(false);
            });
            || ()
        });
    }

    let on_input = {
        let new_domain = new_domain.clone();
        Callback::from(move |e: InputEvent| {
            let input: HtmlInputElement = e.target().unwrap().dyn_into().unwrap();
            new_domain.set(input.value());
        })
    };

    let on_add = {
        let new_domain = new_domain.clone();
        let error = error.clone();
        let submitting = submitting.clone();
        let refresh = refresh.clone();
        let vendor_id = props.vendor_id;

        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();

            let domain_val = (*new_domain).trim().to_string();
            if domain_val.is_empty() {
                return;
            }

            let new_domain = new_domain.clone();
            let error = error.clone();
            let submitting = submitting.clone();
            let refresh = refresh.clone();

            submitting.set(true);
            error.set(None);

            wasm_bindgen_futures::spawn_local(async move {
                let body = serde_json::json!({ "domain": domain_val });
                match Request::post(&format!("/api/vendors/{}/allowed-domains", vendor_id))
                    .header("Content-Type", "application/json")
                    .body(body.to_string())
                    .unwrap()
                    .send()
                    .await
                {
                    Ok(resp) if resp.ok() => {
                        new_domain.set(String::new());
                        refresh.set(*refresh + 1);
                    }
                    Ok(resp) => {
                        let msg = resp
                            .json::<shared::ApiError>()
                            .await
                            .map(|e| e.error.message)
                            .unwrap_or_else(|_| "Failed to add domain".to_string());
                        error.set(Some(msg));
                    }
                    Err(e) => error.set(Some(format!("Request error: {}", e))),
                }
                submitting.set(false);
            });
        })
    };

    html! {
        <div class="vendor-domains">
            <h4>{ "Portal access" }</h4>
            <p class="vendor-domains-hint">
                { "Anyone with an email at these exact domains can request a read-only \
                   link to this vendor's open items. Subdomains are not included." }
            </p>

            if let Some(err) = (*error).clone() {
                <p class="error">{ err }</p>
            }

            <form class="vendor-domains-form" onsubmit={on_add}>
                <input
                    type="text"
                    placeholder="acme.com"
                    value={(*new_domain).clone()}
                    oninput={on_input}
                />
                <button type="submit" class="btn btn-small btn-primary" disabled={*submitting}>
                    { if *submitting { "Adding..." } else { "Add" } }
                </button>
            </form>

            if *loading {
                <p>{ "Loading..." }</p>
            } else if domains.is_empty() {
                <p class="vendor-domains-empty">
                    { "No domains configured — no one can use the portal for this vendor." }
                </p>
            } else {
                <ul class="vendor-domains-list">
                    { for domains.iter().map(|d| {
                        let domain_id = d.id;
                        let vendor_id = props.vendor_id;
                        let refresh = refresh.clone();
                        let on_remove = Callback::from(move |_: MouseEvent| {
                            let refresh = refresh.clone();
                            wasm_bindgen_futures::spawn_local(async move {
                                let url = format!(
                                    "/api/vendors/{}/allowed-domains/{}",
                                    vendor_id, domain_id
                                );
                                let _ = Request::delete(&url).send().await;
                                refresh.set(*refresh + 1);
                            });
                        });
                        html! {
                            <li key={d.id}>
                                <span>{ &d.domain }</span>
                                <button
                                    type="button"
                                    class="btn btn-small btn-danger"
                                    onclick={on_remove}
                                >{ "Remove" }</button>
                            </li>
                        }
                    })}
                </ul>
            }
        </div>
    }
}
