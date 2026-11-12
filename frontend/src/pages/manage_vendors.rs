use gloo_net::http::Request;
use wasm_bindgen::JsCast;
use web_sys::HtmlInputElement;
use yew::prelude::*;

use crate::components::VendorDomains;

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

    // Which vendor's portal-domain editor is open, if any.
    let expanded_vendor = use_state(|| None::<i32>);

    // Inline prefix rename. Rare, and it rewrites every item ID for the
    // vendor, so it is opt-in per row and shows what it is about to do.
    let renaming_vendor = use_state(|| None::<i32>);
    let rename_value = use_state(String::new);
    let rename_error = use_state(|| None::<String>);
    let renaming = use_state(|| false);

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
            <div class="modal modal-large" onclick={on_modal_click}>
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
                                    let on_archive = {
                                        let refresh_trigger = refresh_trigger.clone();
                                        Callback::from(move |_: MouseEvent| {
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
                                        })
                                    };
                                    let is_expanded = *expanded_vendor == Some(vendor_id);
                                    let on_toggle_domains = {
                                        let expanded_vendor = expanded_vendor.clone();
                                        Callback::from(move |_: MouseEvent| {
                                            expanded_vendor.set(if is_expanded { None } else { Some(vendor_id) });
                                        })
                                    };
                                    let is_renaming = *renaming_vendor == Some(vendor_id);
                                    let on_rename_start = {
                                        let renaming_vendor = renaming_vendor.clone();
                                        let rename_value = rename_value.clone();
                                        let rename_error = rename_error.clone();
                                        let prefix = v.prefix.clone();
                                        Callback::from(move |_: MouseEvent| {
                                            rename_value.set(prefix.clone());
                                            rename_error.set(None);
                                            renaming_vendor.set(Some(vendor_id));
                                        })
                                    };
                                    let on_rename_cancel = {
                                        let renaming_vendor = renaming_vendor.clone();
                                        Callback::from(move |_: MouseEvent| renaming_vendor.set(None))
                                    };
                                    let on_rename_input = {
                                        let rename_value = rename_value.clone();
                                        Callback::from(move |e: InputEvent| {
                                            let input: HtmlInputElement = e.target().unwrap().dyn_into().unwrap();
                                            rename_value.set(input.value().to_uppercase());
                                        })
                                    };
                                    let on_rename_save = {
                                        let renaming_vendor = renaming_vendor.clone();
                                        let rename_value = rename_value.clone();
                                        let rename_error = rename_error.clone();
                                        let renaming = renaming.clone();
                                        let refresh_trigger = refresh_trigger.clone();
                                        let old_prefix = v.prefix.clone();
                                        Callback::from(move |_: MouseEvent| {
                                            let new_prefix = rename_value.trim().to_string();
                                            if new_prefix == old_prefix {
                                                renaming_vendor.set(None);
                                                return;
                                            }
                                            let renaming_vendor = renaming_vendor.clone();
                                            let rename_error = rename_error.clone();
                                            let renaming = renaming.clone();
                                            let refresh_trigger = refresh_trigger.clone();
                                            renaming.set(true);
                                            rename_error.set(None);
                                            wasm_bindgen_futures::spawn_local(async move {
                                                let body = serde_json::json!({ "prefix": new_prefix });
                                                match Request::patch(&format!("/api/vendors/{}", vendor_id))
                                                    .header("Content-Type", "application/json")
                                                    .body(body.to_string())
                                                    .unwrap()
                                                    .send()
                                                    .await
                                                {
                                                    Ok(resp) if resp.ok() => {
                                                        renaming_vendor.set(None);
                                                        refresh_trigger.set(*refresh_trigger + 1);
                                                    }
                                                    Ok(resp) => {
                                                        let msg = resp.text().await.unwrap_or_else(|_| "Rename failed".into());
                                                        rename_error.set(Some(msg));
                                                    }
                                                    Err(e) => rename_error.set(Some(format!("Request error: {}", e))),
                                                }
                                                renaming.set(false);
                                            });
                                        })
                                    };
                                    html! {
                                        <>
                                        <tr>
                                            <td>
                                                if is_renaming {
                                                    <input
                                                        type="text"
                                                        class="prefix-rename-input"
                                                        value={(*rename_value).clone()}
                                                        oninput={on_rename_input}
                                                        maxlength="5"
                                                        disabled={*renaming}
                                                        autofocus=true
                                                    />
                                                } else {
                                                    <span class="prefix-cell">
                                                        { &v.prefix }
                                                        <button type="button" class="prefix-rename-btn" title="Rename prefix" onclick={on_rename_start}>{ "✎" }</button>
                                                    </span>
                                                }
                                            </td>
                                            <td>{ &v.name }</td>
                                            <td>{ v.description.as_deref().unwrap_or("-") }</td>
                                            <td>
                                                <div class="vendor-row-actions">
                                                    <button type="button" class="btn btn-small btn-secondary" onclick={on_toggle_domains}>
                                                        { if is_expanded { "Hide portal" } else { "Portal" } }
                                                    </button>
                                                    <button type="button" class="btn btn-small btn-danger" onclick={on_archive}>
                                                        { "Archive" }
                                                    </button>
                                                </div>
                                            </td>
                                        </tr>
                                        if is_renaming {
                                            <tr class="vendor-rename-row">
                                                <td colspan="4">
                                                    <div class="vendor-rename">
                                                        <p class="vendor-rename-hint">
                                                            { format!(
                                                                "Renaming rewrites every item ID for {} ({}-001 becomes {}-001), including notes, history and risk links. Old links to the previous IDs will stop working.",
                                                                v.name, v.prefix,
                                                                if rename_value.is_empty() { "NEW".to_string() } else { (*rename_value).clone() }
                                                            ) }
                                                        </p>
                                                        if let Some(err) = (*rename_error).clone() {
                                                            <p class="error">{ err }</p>
                                                        }
                                                        <div class="vendor-row-actions">
                                                            <button type="button" class="btn btn-small btn-danger" onclick={on_rename_save}
                                                                disabled={*renaming || rename_value.trim().len() < 2}>
                                                                { if *renaming { "Renaming..." } else { "Rename and rewrite IDs" } }
                                                            </button>
                                                            <button type="button" class="btn btn-small" onclick={on_rename_cancel} disabled={*renaming}>{ "Cancel" }</button>
                                                        </div>
                                                    </div>
                                                </td>
                                            </tr>
                                        }
                                        if is_expanded {
                                            <tr class="vendor-domains-row">
                                                <td colspan="4">
                                                    <VendorDomains vendor_id={vendor_id} />
                                                </td>
                                            </tr>
                                        }
                                        </>
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
                                                <div class="vendor-row-actions">
                                                    <button type="button" class="btn btn-small btn-success" onclick={on_unarchive}>
                                                        { "Unarchive" }
                                                    </button>
                                                </div>
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
