use gloo_net::http::Request;
use wasm_bindgen::JsCast;
use web_sys::{HtmlInputElement, HtmlSelectElement};
use yew::prelude::*;

#[derive(Clone, PartialEq, serde::Deserialize)]
pub struct User {
    pub id: i32,
    pub name: String,
}

#[derive(Clone, PartialEq, serde::Deserialize)]
pub struct Vendor {
    pub id: i32,
    pub prefix: String,
    pub name: String,
}

#[derive(Clone, PartialEq, serde::Deserialize)]
pub struct Category {
    pub id: i32,
    pub vendor_id: i32,
    pub name: String,
}

#[derive(Properties, PartialEq)]
pub struct NewItemModalProps {
    pub vendors: Vec<Vendor>,
    pub users: Vec<User>,
    pub categories: Vec<Category>,
    pub on_close: Callback<()>,
    pub on_created: Callback<()>,
}

#[function_component(NewItemModal)]
pub fn new_item_modal(props: &NewItemModalProps) -> Html {
    let title = use_state(String::new);
    let due_date = use_state(String::new);
    let category_id = use_state(|| 0i32);
    let priority = use_state(|| "Medium".to_string());
    let vendor_id = use_state(|| props.vendors.first().map(|v| v.id).unwrap_or(0));
    let owner_id = use_state(|| props.users.first().map(|u| u.id).unwrap_or(0));
    let error = use_state(|| None::<String>);
    let submitting = use_state(|| false);
    let new_category_name = use_state(String::new);
    let adding_category = use_state(|| false);

    // Filter categories for current vendor
    let vendor_categories: Vec<&Category> = props
        .categories
        .iter()
        .filter(|c| c.vendor_id == *vendor_id)
        .collect();

    // Set default category when vendor changes
    {
        let category_id = category_id.clone();
        let vendor_id_val = *vendor_id;
        let categories = props.categories.clone();
        use_effect_with(vendor_id_val, move |vid| {
            let first_cat = categories
                .iter()
                .find(|c| c.vendor_id == *vid)
                .map(|c| c.id)
                .unwrap_or(0);
            category_id.set(first_cat);
            || ()
        });
    }

    let on_title_change = {
        let title = title.clone();
        Callback::from(move |e: InputEvent| {
            let input: HtmlInputElement = e.target().unwrap().dyn_into().unwrap();
            title.set(input.value());
        })
    };

    let on_due_date_change = {
        let due_date = due_date.clone();
        Callback::from(move |e: InputEvent| {
            let input: HtmlInputElement = e.target().unwrap().dyn_into().unwrap();
            due_date.set(input.value());
        })
    };

    let on_vendor_change = {
        let vendor_id = vendor_id.clone();
        Callback::from(move |e: Event| {
            let select: HtmlSelectElement = e.target().unwrap().dyn_into().unwrap();
            if let Ok(id) = select.value().parse() {
                vendor_id.set(id);
            }
        })
    };

    let on_category_change = {
        let category_id = category_id.clone();
        Callback::from(move |e: Event| {
            let select: HtmlSelectElement = e.target().unwrap().dyn_into().unwrap();
            if let Ok(id) = select.value().parse() {
                category_id.set(id);
            }
        })
    };

    let on_priority_change = {
        let priority = priority.clone();
        Callback::from(move |e: Event| {
            let select: HtmlSelectElement = e.target().unwrap().dyn_into().unwrap();
            priority.set(select.value());
        })
    };

    let on_owner_change = {
        let owner_id = owner_id.clone();
        Callback::from(move |e: Event| {
            let select: HtmlSelectElement = e.target().unwrap().dyn_into().unwrap();
            if let Ok(id) = select.value().parse() {
                owner_id.set(id);
            }
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

    let on_new_category_input = {
        let new_category_name = new_category_name.clone();
        Callback::from(move |e: InputEvent| {
            let input: HtmlInputElement = e.target().unwrap().dyn_into().unwrap();
            new_category_name.set(input.value());
        })
    };

    let on_show_add_category = {
        let adding_category = adding_category.clone();
        Callback::from(move |_| {
            adding_category.set(true);
        })
    };

    let on_cancel_add_category = {
        let adding_category = adding_category.clone();
        let new_category_name = new_category_name.clone();
        Callback::from(move |_| {
            adding_category.set(false);
            new_category_name.set(String::new());
        })
    };

    let on_add_category = {
        let new_category_name = new_category_name.clone();
        let adding_category = adding_category.clone();
        let category_id = category_id.clone();
        let vendor_id = vendor_id.clone();
        let on_created = props.on_created.clone();

        Callback::from(move |_| {
            let name = (*new_category_name).clone();
            if name.trim().is_empty() {
                return;
            }

            let new_category_name = new_category_name.clone();
            let adding_category = adding_category.clone();
            let category_id = category_id.clone();
            let vendor_id_val = *vendor_id;
            let on_created = on_created.clone();

            wasm_bindgen_futures::spawn_local(async move {
                let body = serde_json::json!({
                    "name": name,
                });

                match Request::post(&format!("/api/vendors/{}/categories", vendor_id_val))
                    .header("Content-Type", "application/json")
                    .body(body.to_string())
                    .unwrap()
                    .send()
                    .await
                {
                    Ok(resp) if resp.ok() => {
                        if let Ok(cat) = resp.json::<Category>().await {
                            category_id.set(cat.id);
                        }
                        adding_category.set(false);
                        new_category_name.set(String::new());
                        // Trigger a refresh to reload categories
                        on_created.emit(());
                    }
                    _ => {}
                }
            });
        })
    };

    let on_submit = {
        let title = title.clone();
        let due_date = due_date.clone();
        let category_id = category_id.clone();
        let priority = priority.clone();
        let vendor_id = vendor_id.clone();
        let owner_id = owner_id.clone();
        let error = error.clone();
        let submitting = submitting.clone();
        let on_created = props.on_created.clone();

        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();

            let title_val = (*title).clone();
            let due_date_val = (*due_date).clone();
            let category_id_val = *category_id;
            let priority_val = (*priority).clone();
            let vendor_id_val = *vendor_id;
            let owner_id_val = *owner_id;
            let error = error.clone();
            let submitting = submitting.clone();
            let on_created = on_created.clone();

            if vendor_id_val == 0 {
                error.set(Some("Please select a vendor".to_string()));
                return;
            }

            if category_id_val == 0 {
                error.set(Some("Please select a category".to_string()));
                return;
            }

            submitting.set(true);

            wasm_bindgen_futures::spawn_local(async move {
                let body = serde_json::json!({
                    "title": title_val,
                    "due_date": if due_date_val.is_empty() { None::<String> } else { Some(due_date_val) },
                    "category_id": category_id_val,
                    "priority": priority_val,
                    "owner_id": owner_id_val,
                });

                match Request::post(&format!("/api/vendors/{}/items", vendor_id_val))
                    .header("Content-Type", "application/json")
                    .body(body.to_string())
                    .unwrap()
                    .send()
                    .await
                {
                    Ok(resp) if resp.ok() => {
                        on_created.emit(());
                    }
                    Ok(resp) => {
                        let msg = resp.text().await.unwrap_or_else(|_| "Unknown error".into());
                        error.set(Some(msg));
                        submitting.set(false);
                    }
                    Err(e) => {
                        error.set(Some(format!("Request error: {}", e)));
                        submitting.set(false);
                    }
                }
            });
        })
    };

    let on_cancel = {
        let on_close = props.on_close.clone();
        Callback::from(move |_| {
            on_close.emit(());
        })
    };

    html! {
        <div class="modal-backdrop" onclick={on_backdrop_click}>
            <div class="modal" onclick={on_modal_click}>
                <div class="modal-header">
                    <h2>{ "New Action Item" }</h2>
                </div>

                if let Some(err) = (*error).clone() {
                    <p class="error">{ err }</p>
                }

                <form onsubmit={on_submit}>
                    <div class="form-group">
                        <label for="vendor">{ "Vendor" }</label>
                        <select id="vendor" onchange={on_vendor_change} required=true>
                            { for props.vendors.iter().map(|v| {
                                html! {
                                    <option value={v.id.to_string()} selected={*vendor_id == v.id}>
                                        { format!("{} - {}", v.prefix, v.name) }
                                    </option>
                                }
                            })}
                        </select>
                    </div>

                    <div class="form-group">
                        <label for="title">{ "Title" }</label>
                        <input
                            type="text"
                            id="title"
                            value={(*title).clone()}
                            oninput={on_title_change}
                            required=true
                        />
                    </div>

                    <div class="form-group">
                        <label for="due_date">{ "Due Date (optional)" }</label>
                        <input
                            type="date"
                            id="due_date"
                            value={(*due_date).clone()}
                            oninput={on_due_date_change}
                        />
                    </div>

                    <div class="form-group">
                        <label for="category">{ "Category" }</label>
                        if *adding_category {
                            <div class="inline-add-form">
                                <input
                                    type="text"
                                    placeholder="New category name"
                                    value={(*new_category_name).clone()}
                                    oninput={on_new_category_input}
                                />
                                <button type="button" class="btn btn-small btn-primary" onclick={on_add_category}>
                                    { "Add" }
                                </button>
                                <button type="button" class="btn btn-small" onclick={on_cancel_add_category}>
                                    { "Cancel" }
                                </button>
                            </div>
                        } else {
                            <div class="select-with-add">
                                <select id="category" onchange={on_category_change}>
                                    { for vendor_categories.iter().map(|c| {
                                        html! {
                                            <option value={c.id.to_string()} selected={*category_id == c.id}>
                                                { &c.name }
                                            </option>
                                        }
                                    })}
                                </select>
                                <button type="button" class="btn btn-small" onclick={on_show_add_category} title="Add new category">
                                    { "+" }
                                </button>
                            </div>
                        }
                    </div>

                    <div class="form-group">
                        <label for="priority">{ "Priority" }</label>
                        <select id="priority" onchange={on_priority_change}>
                            <option value="High" selected={*priority == "High"}>{ "High" }</option>
                            <option value="Medium" selected={*priority == "Medium"}>{ "Medium" }</option>
                            <option value="Low" selected={*priority == "Low"}>{ "Low" }</option>
                        </select>
                    </div>

                    <div class="form-group">
                        <label for="owner">{ "Owner" }</label>
                        <select id="owner" onchange={on_owner_change}>
                            { for props.users.iter().map(|u| {
                                html! {
                                    <option value={u.id.to_string()} selected={*owner_id == u.id}>
                                        { &u.name }
                                    </option>
                                }
                            })}
                        </select>
                    </div>

                    <div class="form-actions">
                        <button type="submit" class="btn btn-primary" disabled={*submitting}>
                            { if *submitting { "Creating..." } else { "Create Item" } }
                        </button>
                        <button type="button" class="btn" onclick={on_cancel}>
                            { "Cancel" }
                        </button>
                    </div>
                </form>
            </div>
        </div>
    }
}
