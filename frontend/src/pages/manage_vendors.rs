use gloo_net::http::Request;
use wasm_bindgen::JsCast;
use web_sys::HtmlInputElement;
use yew::prelude::*;

#[derive(Clone, PartialEq, serde::Deserialize)]
pub struct VendorEntry {
    pub id: i32,
    pub prefix: String,
    pub name: String,
    pub description: Option<String>,
    pub archived: bool,
}

#[derive(Properties, PartialEq)]
pub struct ManageVendorsModalProps {
    pub on_close: Callback<()>,
}

#[function_component(ManageVendorsModal)]
pub fn manage_vendors_modal(props: &ManageVendorsModalProps) -> Html {
    let vendors = use_state(Vec::<VendorEntry>::new);
    let loading = use_state(|| true);
    let error = use_state(|| None::<String>);
    let refresh_trigger = use_state(|| 0u32);

    let new_prefix = use_state(String::new);
    let new_name = use_state(String::new);
    let new_description = use_state(String::new);
    let submitting = use_state(|| false);

    // Fetch vendors (include archived)
    {
        let vendors = vendors.clone();
        let loading = loading.clone();
        let error = error.clone();
        let refresh = *refresh_trigger;

        use_effect_with(refresh, move |_| {
            wasm_bindgen_futures::spawn_local(async move {
                match Request::get("/api/vendors?include_archived=true")
                    .send()
                    .await
                {
                    Ok(resp) if resp.ok() => {
                        if let Ok(data) = resp.json::<Vec<VendorEntry>>().await {
                            vendors.set(data);
                        }
                    }
                    Ok(resp) => {
                        error.set(Some(format!("Failed to fetch vendors: {}", resp.status())));
                    }
                    Err(e) => {
                        error.set(Some(format!("Request error: {}", e)));
                    }
                }
                loading.set(false);
            });
            || ()
        });
    }

    let on_prefix_input = {
        let new_prefix = new_prefix.clone();
        Callback::from(move |e: InputEvent| {
            let input: HtmlInputElement = e.target().unwrap().dyn_into().unwrap();
            new_prefix.set(input.value().to_uppercase());
        })
    };

    let on_name_input = {
        let new_name = new_name.clone();
        Callback::from(move |e: InputEvent| {
            let input: HtmlInputElement = e.target().unwrap().dyn_into().unwrap();
            new_name.set(input.value());
        })
    };

    let on_description_input = {
        let new_description = new_description.clone();
        Callback::from(move |e: InputEvent| {
            let input: HtmlInputElement = e.target().unwrap().dyn_into().unwrap();
            new_description.set(input.value());
        })
    };

    let on_add_vendor = {
        let new_prefix = new_prefix.clone();
        let new_name = new_name.clone();
        let new_description = new_description.clone();
        let error = error.clone();
        let submitting = submitting.clone();
        let refresh_trigger = refresh_trigger.clone();

        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();

            let prefix_val = (*new_prefix).clone();
            let name_val = (*new_name).clone();
            let desc_val = (*new_description).clone();
            let error = error.clone();
            let submitting = submitting.clone();
            let refresh_trigger = refresh_trigger.clone();
            let new_prefix = new_prefix.clone();
            let new_name = new_name.clone();
            let new_description = new_description.clone();

            if prefix_val.is_empty() || name_val.is_empty() {
                error.set(Some("Prefix and name are required".to_string()));
                return;
            }

            submitting.set(true);
            error.set(None);

            wasm_bindgen_futures::spawn_local(async move {
                let body = serde_json::json!({
                    "prefix": prefix_val,
                    "name": name_val,
                    "description": if desc_val.is_empty() { None::<String> } else { Some(desc_val) },
                });

                match Request::post("/api/vendors")
                    .header("Content-Type", "application/json")
                    .body(body.to_string())
                    .unwrap()
                    .send()
                    .await
                {
                    Ok(resp) if resp.ok() => {
                        new_prefix.set(String::new());
                        new_name.set(String::new());
                        new_description.set(String::new());
                        refresh_trigger.set(*refresh_trigger + 1);
                    }
                    Ok(resp) => {
                        let msg = resp
                            .text()
                            .await
                            .unwrap_or_else(|_| "Failed to create vendor".into());
                        error.set(Some(msg));
                    }
                    Err(e) => {
                        error.set(Some(format!("Request error: {}", e)));
                    }
                }
                submitting.set(false);
            });
        })
    };

    let on_backdrop_click = {
        let on_close = props.on_close.clone();
        Callback::from(move |_| {
            on_close.emit(());
        })
    };

    let on_modal_click = Callback::from(|e: MouseEvent| {
        e.stop_propagation();
    });

    let active_vendors: Vec<&VendorEntry> = vendors.iter().filter(|v| !v.archived).collect();
    let archived_vendors: Vec<&VendorEntry> = vendors.iter().filter(|v| v.archived).collect();

    html! {
        <div class="modal-backdrop" onclick={on_backdrop_click}>
            <div class="modal" onclick={on_modal_click}>
                <div class="modal-header">
                    <h2>{ "Manage Vendors" }</h2>
                    <button class="btn-close" onclick={
                        let on_close = props.on_close.clone();
                        Callback::from(move |_: MouseEvent| on_close.emit(()))
                    }>{ "\u{00d7}" }</button>
                </div>

                <div class="modal-body">
                    if let Some(err) = (*error).clone() {
                        <p class="error">{ err }</p>
                    }

                    <form class="add-vendor-form" onsubmit={on_add_vendor}>
                        <div class="form-group">
                            <label>{ "Prefix" }</label>
                            <input
                                type="text"
                                placeholder="XX"
                                value={(*new_prefix).clone()}
                                oninput={on_prefix_input}
                                maxlength="5"
                                style="width: 80px"
                            />
                        </div>
                        <div class="form-group">
                            <label>{ "Name" }</label>
                            <input
                                type="text"
                                placeholder="Vendor name"
                                value={(*new_name).clone()}
                                oninput={on_name_input}
                            />
                        </div>
                        <div class="form-group">
                            <label>{ "Description" }</label>
                            <input
                                type="text"
                                placeholder="Optional"
                                value={(*new_description).clone()}
                                oninput={on_description_input}
                            />
                        </div>
                        <button type="submit" class="btn btn-primary" disabled={*submitting}>
                            { if *submitting { "Adding..." } else { "Add Vendor" } }
                        </button>
                    </form>

                    if *loading {
                        <p>{ "Loading..." }</p>
                    } else {
                        <table class="table manage-vendors-list">
                            <thead>
                                <tr>
                                    <th>{ "Prefix" }</th>
                                    <th>{ "Name" }</th>
                                    <th>{ "Description" }</th>
                                    <th></th>
                                </tr>
                            </thead>
                            <tbody>
                                { for active_vendors.iter().map(|v| {
                                    let vendor_id = v.id;
                                    let refresh_trigger = refresh_trigger.clone();
                                    let on_archive = Callback::from(move |_: MouseEvent| {
                                        let refresh_trigger = refresh_trigger.clone();
                                        wasm_bindgen_futures::spawn_local(async move {
                                            let body = serde_json::json!({ "archived": true });
                                            let _ = Request::patch(&format!("/api/vendors/{}", vendor_id))
                                                .header("Content-Type", "application/json")
                                                .body(body.to_string())
                                                .unwrap()
                                                .send()
                                                .await;
                                            refresh_trigger.set(*refresh_trigger + 1);
                                        });
                                    });
                                    html! {
                                        <tr>
                                            <td>{ &v.prefix }</td>
                                            <td>{ &v.name }</td>
                                            <td>{ v.description.as_deref().unwrap_or("-") }</td>
                                            <td>
                                                <button type="button" class="btn btn-small btn-danger" onclick={on_archive}>
                                                    { "Archive" }
                                                </button>
                                            </td>
                                        </tr>
                                    }
                                })}
                                { for archived_vendors.iter().map(|v| {
                                    let vendor_id = v.id;
                                    let refresh_trigger = refresh_trigger.clone();
                                    let on_unarchive = Callback::from(move |_: MouseEvent| {
                                        let refresh_trigger = refresh_trigger.clone();
                                        wasm_bindgen_futures::spawn_local(async move {
                                            let body = serde_json::json!({ "archived": false });
                                            let _ = Request::patch(&format!("/api/vendors/{}", vendor_id))
                                                .header("Content-Type", "application/json")
                                                .body(body.to_string())
                                                .unwrap()
                                                .send()
                                                .await;
                                            refresh_trigger.set(*refresh_trigger + 1);
                                        });
                                    });
                                    html! {
                                        <tr class="vendor-archived">
                                            <td>{ &v.prefix }</td>
                                            <td>{ &v.name }</td>
                                            <td>{ v.description.as_deref().unwrap_or("-") }</td>
                                            <td>
                                                <button type="button" class="btn btn-small btn-success" onclick={on_unarchive}>
                                                    { "Unarchive" }
                                                </button>
                                            </td>
                                        </tr>
                                    }
                                })}
                            </tbody>
                        </table>
                    }
                </div>
            </div>
        </div>
    }
}
