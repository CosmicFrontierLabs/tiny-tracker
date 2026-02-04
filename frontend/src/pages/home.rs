use gloo_net::http::Request;
use wasm_bindgen::JsCast;
use web_sys::HtmlSelectElement;
use yew::prelude::*;

use crate::components::Header;
use crate::pages::item_detail::ItemDetailModal;
use crate::pages::item_form::{Category, NewItemModal, User, Vendor};

#[derive(Clone, PartialEq, serde::Deserialize)]
struct ActionItemWithStatus {
    id: String,
    vendor_id: i32,
    number: i32,
    title: String,
    create_date: String,
    due_date: Option<String>,
    category: String,
    owner_id: i32,
    priority: String,
    status: String,
    created_by_name: String,
    created_by_initials: Option<String>,
    owner_name: String,
    owner_initials: Option<String>,
}

fn name_to_color(name: &str) -> String {
    let hash: u32 = name
        .bytes()
        .fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
    let hue = hash % 360;
    format!("hsl({}, 65%, 45%)", hue)
}

fn get_initials(name: &str, fallback_initials: Option<&str>) -> String {
    if let Some(initials) = fallback_initials {
        return initials.to_string();
    }
    name.split_whitespace()
        .filter_map(|word| word.chars().next())
        .take(2)
        .collect::<String>()
        .to_uppercase()
}

#[function_component(Home)]
pub fn home() -> Html {
    let items = use_state(Vec::<ActionItemWithStatus>::new);
    let vendors = use_state(Vec::<Vendor>::new);
    let users = use_state(Vec::<User>::new);
    let categories = use_state(Vec::<Category>::new);
    let loading = use_state(|| true);
    let error = use_state(|| None::<String>);
    let show_new_item_modal = use_state(|| false);
    let selected_item_id = use_state(|| None::<String>);
    let refresh_trigger = use_state(|| 0u32);
    let filter_vendor_id = use_state(|| None::<i32>);
    let filter_owner_id = use_state(|| None::<i32>);

    {
        let items = items.clone();
        let vendors = vendors.clone();
        let users = users.clone();
        let categories = categories.clone();
        let loading = loading.clone();
        let error = error.clone();
        let refresh = *refresh_trigger;

        use_effect_with(refresh, move |_| {
            wasm_bindgen_futures::spawn_local(async move {
                // Fetch items
                match Request::get("/api/items").send().await {
                    Ok(resp) => {
                        if resp.ok() {
                            match resp.json::<Vec<ActionItemWithStatus>>().await {
                                Ok(data) => {
                                    items.set(data);
                                }
                                Err(e) => {
                                    error.set(Some(format!("Failed to parse response: {}", e)));
                                }
                            }
                        } else {
                            error.set(Some(format!("Request failed: {}", resp.status())));
                        }
                    }
                    Err(e) => {
                        error.set(Some(format!("Request error: {}", e)));
                    }
                }

                // Fetch vendors for the dropdown
                if let Ok(resp) = Request::get("/api/vendors").send().await {
                    if let Ok(data) = resp.json::<Vec<Vendor>>().await {
                        vendors.set(data);
                    }
                }

                // Fetch users for the dropdown
                if let Ok(resp) = Request::get("/api/users").send().await {
                    if let Ok(data) = resp.json::<Vec<User>>().await {
                        users.set(data);
                    }
                }

                // Fetch categories for the dropdown
                if let Ok(resp) = Request::get("/api/categories").send().await {
                    if let Ok(data) = resp.json::<Vec<Category>>().await {
                        categories.set(data);
                    }
                }

                loading.set(false);
            });
            || ()
        });
    }

    let on_new_item_click = {
        let show_new_item_modal = show_new_item_modal.clone();
        Callback::from(move |_| {
            show_new_item_modal.set(true);
        })
    };

    let on_new_item_modal_close = {
        let show_new_item_modal = show_new_item_modal.clone();
        Callback::from(move |_| {
            show_new_item_modal.set(false);
        })
    };

    let on_item_created = {
        let show_new_item_modal = show_new_item_modal.clone();
        let refresh_trigger = refresh_trigger.clone();
        Callback::from(move |_| {
            show_new_item_modal.set(false);
            refresh_trigger.set(*refresh_trigger + 1);
        })
    };

    let on_item_detail_close = {
        let selected_item_id = selected_item_id.clone();
        let refresh_trigger = refresh_trigger.clone();
        Callback::from(move |_| {
            selected_item_id.set(None);
            refresh_trigger.set(*refresh_trigger + 1);
        })
    };

    let priority_class = |priority: &str| -> &'static str {
        match priority {
            "High" => "priority-high",
            "Medium" => "priority-medium",
            "Low" => "priority-low",
            _ => "",
        }
    };

    let status_class = |status: &str| -> &'static str {
        match status {
            "New" => "status-new",
            "Not Started" => "status-not-started",
            "In Progress" => "status-in-progress",
            "TBC" => "status-tbc",
            "Complete" => "status-complete",
            "Blocked" => "status-blocked",
            _ => "",
        }
    };

    let on_vendor_filter_change = {
        let filter_vendor_id = filter_vendor_id.clone();
        Callback::from(move |e: Event| {
            let select: HtmlSelectElement = e.target().unwrap().dyn_into().unwrap();
            let value = select.value();
            if value.is_empty() {
                filter_vendor_id.set(None);
            } else {
                filter_vendor_id.set(value.parse().ok());
            }
        })
    };

    let on_owner_filter_change = {
        let filter_owner_id = filter_owner_id.clone();
        Callback::from(move |e: Event| {
            let select: HtmlSelectElement = e.target().unwrap().dyn_into().unwrap();
            let value = select.value();
            if value.is_empty() {
                filter_owner_id.set(None);
            } else {
                filter_owner_id.set(value.parse().ok());
            }
        })
    };

    // Apply filters to items
    let filtered_items: Vec<_> = items
        .iter()
        .filter(|item| {
            let vendor_match = filter_vendor_id
                .as_ref()
                .map(|v| item.vendor_id == *v)
                .unwrap_or(true);
            let owner_match = filter_owner_id
                .as_ref()
                .map(|o| item.owner_id == *o)
                .unwrap_or(true);
            vendor_match && owner_match
        })
        .collect();

    html! {
        <>
            <Header />
            <main>
                <div class="page-header">
                    <h2>{ "Action Items" }</h2>
                    <div class="header-actions">
                        <button type="button" class="btn btn-primary" onclick={on_new_item_click} disabled={vendors.is_empty()}>
                            { "+ New Item" }
                        </button>
                    </div>
                </div>

                <div class="filters">
                    <div class="filter-group">
                        <label>{ "Vendor:" }</label>
                        <select onchange={on_vendor_filter_change}>
                            <option value="" selected={filter_vendor_id.is_none()}>{ "All Vendors" }</option>
                            { for vendors.iter().map(|v| {
                                let selected = filter_vendor_id.as_ref().map(|id| *id == v.id).unwrap_or(false);
                                html! {
                                    <option value={v.id.to_string()} selected={selected}>{ &v.name }</option>
                                }
                            })}
                        </select>
                    </div>
                    <div class="filter-group">
                        <label>{ "Owner:" }</label>
                        <select onchange={on_owner_filter_change}>
                            <option value="" selected={filter_owner_id.is_none()}>{ "All Owners" }</option>
                            { for users.iter().map(|u| {
                                let selected = filter_owner_id.as_ref().map(|id| *id == u.id).unwrap_or(false);
                                html! {
                                    <option value={u.id.to_string()} selected={selected}>{ &u.name }</option>
                                }
                            })}
                        </select>
                    </div>
                </div>

                if *show_new_item_modal {
                    <NewItemModal
                        vendors={(*vendors).clone()}
                        users={(*users).clone()}
                        categories={(*categories).clone()}
                        on_close={on_new_item_modal_close}
                        on_created={on_item_created}
                    />
                }

                if let Some(item_id) = (*selected_item_id).clone() {
                    <ItemDetailModal
                        item_id={item_id}
                        on_close={on_item_detail_close}
                    />
                }

                if *loading {
                    <p>{ "Loading..." }</p>
                } else if let Some(err) = (*error).clone() {
                    <p class="error">{ err }</p>
                } else if vendors.is_empty() {
                    <p>{ "No vendors configured. Use the CLI to add vendors first." }</p>
                } else if items.is_empty() {
                    <p>{ "No action items yet. Click '+ New Item' to create one." }</p>
                } else if filtered_items.is_empty() {
                    <p>{ "No items match the current filters." }</p>
                } else {
                    <table class="table items-table">
                        <thead>
                            <tr>
                                <th>{ "ID" }</th>
                                <th>{ "Title" }</th>
                                <th>{ "Category" }</th>
                                <th>{ "Creator" }</th>
                                <th>{ "Owner" }</th>
                                <th>{ "Priority" }</th>
                                <th>{ "Status" }</th>
                                <th>{ "Created" }</th>
                                <th>{ "Due Date" }</th>
                            </tr>
                        </thead>
                        <tbody>
                            { for filtered_items.iter().map(|item| {
                                let item_id = item.id.clone();
                                let selected_item_id = selected_item_id.clone();
                                let on_row_click = {
                                    let item_id = item_id.clone();
                                    Callback::from(move |_| {
                                        selected_item_id.set(Some(item_id.clone()));
                                    })
                                };
                                let creator_initials = get_initials(&item.created_by_name, item.created_by_initials.as_deref());
                                let creator_color = name_to_color(&item.created_by_name);
                                let owner_initials = get_initials(&item.owner_name, item.owner_initials.as_deref());
                                let owner_color = name_to_color(&item.owner_name);
                                html! {
                                    <tr class="clickable-row" onclick={on_row_click}>
                                        <td>
                                            <span class="item-id">{ &item.id }</span>
                                        </td>
                                        <td class="item-title">{ &item.title }</td>
                                        <td>{ &item.category }</td>
                                        <td>
                                            <span class="user-avatar" style={format!("background-color: {}", creator_color)} title={item.created_by_name.clone()}>
                                                { creator_initials }
                                            </span>
                                        </td>
                                        <td>
                                            <span class="user-avatar" style={format!("background-color: {}", owner_color)} title={item.owner_name.clone()}>
                                                { owner_initials }
                                            </span>
                                        </td>
                                        <td class={priority_class(&item.priority)}>
                                            { &item.priority }
                                        </td>
                                        <td class={status_class(&item.status)}>
                                            { &item.status }
                                        </td>
                                        <td>{ &item.create_date }</td>
                                        <td>
                                            { item.due_date.as_deref().unwrap_or("-") }
                                        </td>
                                    </tr>
                                }
                            })}
                        </tbody>
                    </table>
                }
            </main>
        </>
    }
}
