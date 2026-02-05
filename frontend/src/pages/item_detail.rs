use chrono::{DateTime, NaiveDate, Utc};
use gloo_net::http::Request;
use js_sys::{Date, Object, Reflect};
use regex::Regex;
use shared::{ActionItemResponse, NoteResponse, StatusHistoryResponse};
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{HtmlInputElement, HtmlSelectElement, HtmlTextAreaElement};
use yew::prelude::*;

// (display_name, api_value)
const STATUSES: &[(&str, &str)] = &[
    ("New", "new"),
    ("Not Started", "not_started"),
    ("In Progress", "in_progress"),
    ("TBC", "tbc"),
    ("Complete", "complete"),
    ("Blocked", "blocked"),
];

fn display_to_api(display: &str) -> &'static str {
    STATUSES
        .iter()
        .find(|(d, _)| *d == display)
        .map(|(_, a)| *a)
        .unwrap_or("new")
}

#[derive(Clone, PartialEq)]
enum HistoryEntry {
    Note {
        timestamp: DateTime<Utc>,
        author: String,
        content: String,
    },
    StatusChange {
        timestamp: DateTime<Utc>,
        changed_by: String,
        from_status: Option<String>,
        to_status: String,
        comment: Option<String>,
    },
}

#[derive(Properties, PartialEq)]
pub struct ItemDetailModalProps {
    pub item_id: String,
    pub on_close: Callback<()>,
}

fn linkify_text(text: &str) -> Html {
    let url_regex = Regex::new(r"(https?://[^\s<>\[\]()]+)").unwrap();
    let mut result = Vec::new();
    let mut last_end = 0;

    for cap in url_regex.captures_iter(text) {
        let m = cap.get(0).unwrap();
        if m.start() > last_end {
            result.push(html! { <>{&text[last_end..m.start()]}</> });
        }
        let url = m.as_str();
        result.push(html! {
            <a href={url.to_string()} target="_blank" rel="noopener noreferrer" class="auto-link">{ url }</a>
        });
        last_end = m.end();
    }
    if last_end < text.len() {
        result.push(html! { <>{&text[last_end..]}</> });
    }
    html! { <>{ for result }</> }
}

fn format_datetime(dt: &DateTime<Utc>) -> String {
    // Use JS Date.toLocaleString for locale-aware formatting in user's timezone
    let js_date = Date::new(&JsValue::from_f64(dt.timestamp_millis() as f64));

    // Create options for datetime formatting
    let options = Object::new();
    let _ = Reflect::set(&options, &"year".into(), &"numeric".into());
    let _ = Reflect::set(&options, &"month".into(), &"short".into());
    let _ = Reflect::set(&options, &"day".into(), &"numeric".into());
    let _ = Reflect::set(&options, &"hour".into(), &"2-digit".into());
    let _ = Reflect::set(&options, &"minute".into(), &"2-digit".into());
    let _ = Reflect::set(&options, &"second".into(), &"2-digit".into());

    js_date
        .to_locale_string("default", &options)
        .as_string()
        .unwrap_or_else(|| dt.format("%b %d, %Y %H:%M:%S").to_string())
}

fn format_naive_date(date: &NaiveDate) -> String {
    // Use JS Date.toLocaleDateString for locale-aware date formatting
    // NaiveDate doesn't have timezone, treat as midnight UTC
    let js_date = Date::new(&JsValue::from_str(&format!("{}T00:00:00Z", date)));

    let options = Object::new();
    let _ = Reflect::set(&options, &"year".into(), &"numeric".into());
    let _ = Reflect::set(&options, &"month".into(), &"short".into());
    let _ = Reflect::set(&options, &"day".into(), &"numeric".into());

    js_date
        .to_locale_date_string("default", &options)
        .as_string()
        .unwrap_or_else(|| date.format("%b %d, %Y").to_string())
}

#[function_component(ItemDetailModal)]
pub fn item_detail_modal(props: &ItemDetailModalProps) -> Html {
    let item = use_state(|| None::<ActionItemResponse>);
    let history = use_state(Vec::<HistoryEntry>::new);
    let loading = use_state(|| true);
    let error = use_state(|| None::<String>);
    let new_update_content = use_state(String::new);
    let submitting = use_state(|| false);
    let refresh_trigger = use_state(|| 0u32);
    let changing_status = use_state(|| false);

    // Editing states
    let editing_title = use_state(|| false);
    let editing_description = use_state(|| false);
    let edit_title_value = use_state(String::new);
    let edit_description_value = use_state(String::new);
    let saving = use_state(|| false);

    let item_id = props.item_id.clone();

    {
        let item = item.clone();
        let history = history.clone();
        let loading = loading.clone();
        let error = error.clone();
        let item_id = item_id.clone();
        let refresh = *refresh_trigger;

        use_effect_with((item_id.clone(), refresh), move |(iid, _)| {
            let iid = iid.clone();
            wasm_bindgen_futures::spawn_local(async move {
                // Fetch item
                match Request::get(&format!("/api/items/{}", iid)).send().await {
                    Ok(resp) if resp.ok() => {
                        if let Ok(data) = resp.json::<ActionItemResponse>().await {
                            item.set(Some(data));
                        }
                    }
                    Ok(resp) => {
                        error.set(Some(format!("Failed to load item: {}", resp.status())));
                    }
                    Err(e) => {
                        error.set(Some(format!("Request error: {}", e)));
                    }
                }

                // Fetch notes and status history, then merge them
                let mut entries: Vec<(DateTime<Utc>, HistoryEntry)> = Vec::new();

                // Fetch notes
                if let Ok(resp) = Request::get(&format!("/api/items/{}/notes", iid))
                    .send()
                    .await
                {
                    if let Ok(notes) = resp.json::<Vec<NoteResponse>>().await {
                        for note in notes {
                            entries.push((
                                note.created_at,
                                HistoryEntry::Note {
                                    timestamp: note.created_at,
                                    author: note.author_name,
                                    content: note.content,
                                },
                            ));
                        }
                    }
                }

                // Fetch status history
                if let Ok(resp) = Request::get(&format!("/api/items/{}/history", iid))
                    .send()
                    .await
                {
                    if let Ok(status_changes) = resp.json::<Vec<StatusHistoryResponse>>().await {
                        let mut prev_status: Option<String> = None;
                        // Status history comes in desc order, reverse to get chronological
                        let mut changes: Vec<_> = status_changes.into_iter().collect();
                        changes.reverse();
                        for change in changes {
                            entries.push((
                                change.changed_at,
                                HistoryEntry::StatusChange {
                                    timestamp: change.changed_at,
                                    changed_by: change.changed_by_name,
                                    from_status: prev_status.clone(),
                                    to_status: change.status.clone(),
                                    comment: change.comment,
                                },
                            ));
                            prev_status = Some(change.status);
                        }
                    }
                }

                // Sort by timestamp descending (newest first)
                entries.sort_by(|a, b| b.0.cmp(&a.0));
                let sorted_history: Vec<HistoryEntry> =
                    entries.into_iter().map(|(_, e)| e).collect();
                history.set(sorted_history);

                loading.set(false);
            });
            || ()
        });
    }

    let on_update_change = {
        let new_update_content = new_update_content.clone();
        Callback::from(move |e: InputEvent| {
            let textarea: HtmlTextAreaElement = e.target().unwrap().dyn_into().unwrap();
            new_update_content.set(textarea.value());
        })
    };

    let on_add_update = {
        let new_update_content = new_update_content.clone();
        let submitting = submitting.clone();
        let refresh_trigger = refresh_trigger.clone();
        let item_id = item_id.clone();

        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();

            let content = (*new_update_content).clone();
            if content.trim().is_empty() {
                return;
            }

            let new_update_content = new_update_content.clone();
            let submitting = submitting.clone();
            let refresh_trigger = refresh_trigger.clone();
            let item_id = item_id.clone();

            submitting.set(true);

            wasm_bindgen_futures::spawn_local(async move {
                let body = serde_json::json!({
                    "content": content,
                });

                match Request::post(&format!("/api/items/{}/notes", item_id))
                    .header("Content-Type", "application/json")
                    .body(body.to_string())
                    .unwrap()
                    .send()
                    .await
                {
                    Ok(resp) if resp.ok() => {
                        new_update_content.set(String::new());
                        refresh_trigger.set(*refresh_trigger + 1);
                    }
                    _ => {}
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

    let on_close_btn = {
        let on_close = props.on_close.clone();
        Callback::from(move |_| {
            on_close.emit(());
        })
    };

    // Title editing handlers
    let on_title_click = {
        let editing_title = editing_title.clone();
        let edit_title_value = edit_title_value.clone();
        let item = item.clone();
        Callback::from(move |_| {
            if let Some(ref i) = *item {
                edit_title_value.set(i.title.clone());
                editing_title.set(true);
            }
        })
    };

    let on_title_input = {
        let edit_title_value = edit_title_value.clone();
        Callback::from(move |e: InputEvent| {
            let input: HtmlInputElement = e.target().unwrap().dyn_into().unwrap();
            edit_title_value.set(input.value());
        })
    };

    let on_title_blur = {
        let editing_title = editing_title.clone();
        let edit_title_value = edit_title_value.clone();
        let item = item.clone();
        let saving = saving.clone();
        let refresh_trigger = refresh_trigger.clone();
        let item_id = item_id.clone();
        Callback::from(move |_| {
            let new_title = (*edit_title_value).clone();
            let current_title = (*item)
                .as_ref()
                .map(|i| i.title.clone())
                .unwrap_or_default();

            if new_title.trim().is_empty() || new_title == current_title {
                editing_title.set(false);
                return;
            }

            let editing_title = editing_title.clone();
            let saving = saving.clone();
            let refresh_trigger = refresh_trigger.clone();
            let item_id = item_id.clone();

            saving.set(true);

            wasm_bindgen_futures::spawn_local(async move {
                let body = serde_json::json!({
                    "title": new_title,
                });

                match Request::patch(&format!("/api/items/{}", item_id))
                    .header("Content-Type", "application/json")
                    .body(body.to_string())
                    .unwrap()
                    .send()
                    .await
                {
                    Ok(resp) if resp.ok() => {
                        refresh_trigger.set(*refresh_trigger + 1);
                    }
                    _ => {}
                }
                saving.set(false);
                editing_title.set(false);
            });
        })
    };

    let on_title_keydown = {
        let on_title_blur = on_title_blur.clone();
        let editing_title = editing_title.clone();
        Callback::from(move |e: KeyboardEvent| {
            if e.key() == "Enter" {
                e.prevent_default();
                on_title_blur.emit(FocusEvent::new("blur").unwrap());
            } else if e.key() == "Escape" {
                editing_title.set(false);
            }
        })
    };

    // Description editing handlers
    let on_description_click = {
        let editing_description = editing_description.clone();
        let edit_description_value = edit_description_value.clone();
        let item = item.clone();
        Callback::from(move |_| {
            if let Some(ref i) = *item {
                edit_description_value.set(i.description.clone().unwrap_or_default());
                editing_description.set(true);
            }
        })
    };

    let on_description_input = {
        let edit_description_value = edit_description_value.clone();
        Callback::from(move |e: InputEvent| {
            let textarea: HtmlTextAreaElement = e.target().unwrap().dyn_into().unwrap();
            edit_description_value.set(textarea.value());
        })
    };

    let on_description_blur = {
        let editing_description = editing_description.clone();
        let edit_description_value = edit_description_value.clone();
        let item = item.clone();
        let saving = saving.clone();
        let refresh_trigger = refresh_trigger.clone();
        let item_id = item_id.clone();
        Callback::from(move |_| {
            let new_desc = (*edit_description_value).clone();
            let current_desc = (*item)
                .as_ref()
                .and_then(|i| i.description.clone())
                .unwrap_or_default();

            if new_desc == current_desc {
                editing_description.set(false);
                return;
            }

            let editing_description = editing_description.clone();
            let saving = saving.clone();
            let refresh_trigger = refresh_trigger.clone();
            let item_id = item_id.clone();

            saving.set(true);

            wasm_bindgen_futures::spawn_local(async move {
                let body = if new_desc.is_empty() {
                    serde_json::json!({ "description": null })
                } else {
                    serde_json::json!({ "description": new_desc })
                };

                match Request::patch(&format!("/api/items/{}", item_id))
                    .header("Content-Type", "application/json")
                    .body(body.to_string())
                    .unwrap()
                    .send()
                    .await
                {
                    Ok(resp) if resp.ok() => {
                        refresh_trigger.set(*refresh_trigger + 1);
                    }
                    _ => {}
                }
                saving.set(false);
                editing_description.set(false);
            });
        })
    };

    let on_description_keydown = {
        let editing_description = editing_description.clone();
        Callback::from(move |e: KeyboardEvent| {
            if e.key() == "Escape" {
                editing_description.set(false);
            }
        })
    };

    // Status change handler
    let on_status_change = {
        let item = item.clone();
        let changing_status = changing_status.clone();
        let refresh_trigger = refresh_trigger.clone();
        let item_id = item_id.clone();
        Callback::from(move |e: Event| {
            let select: HtmlSelectElement = e.target().unwrap().dyn_into().unwrap();
            let new_status = select.value();
            let current_status = (*item)
                .as_ref()
                .map(|i| i.status.clone())
                .unwrap_or_default();

            if new_status == current_status {
                return;
            }

            let changing_status = changing_status.clone();
            let refresh_trigger = refresh_trigger.clone();
            let item_id = item_id.clone();

            changing_status.set(true);

            wasm_bindgen_futures::spawn_local(async move {
                let api_status = display_to_api(&new_status);
                let body = serde_json::json!({
                    "status": api_status,
                });

                match Request::post(&format!("/api/items/{}/status", item_id))
                    .header("Content-Type", "application/json")
                    .body(body.to_string())
                    .unwrap()
                    .send()
                    .await
                {
                    Ok(resp) if resp.ok() => {
                        refresh_trigger.set(*refresh_trigger + 1);
                    }
                    _ => {}
                }
                changing_status.set(false);
            });
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

    html! {
        <div class="modal-backdrop" onclick={on_backdrop_click}>
            <div class="modal modal-large" onclick={on_modal_click}>
                if *loading {
                    <div class="modal-header">
                        <h2>{ "Loading..." }</h2>
                    </div>
                } else if let Some(err) = (*error).clone() {
                    <div class="modal-header">
                        <h2>{ "Error" }</h2>
                        <button type="button" class="modal-close" onclick={on_close_btn.clone()}>{ "×" }</button>
                    </div>
                    <div class="modal-body">
                        <p class="error">{ err }</p>
                    </div>
                } else if let Some(i) = (*item).clone() {
                    <div class="modal-header">
                        <div class="title-container">
                            <span class="item-id-badge">{ &i.id }</span>
                            if *editing_title {
                                <input
                                    type="text"
                                    class="title-edit-input"
                                    value={(*edit_title_value).clone()}
                                    oninput={on_title_input}
                                    onblur={on_title_blur}
                                    onkeydown={on_title_keydown}
                                    autofocus=true
                                />
                            } else {
                                <h2 class="editable-title" onclick={on_title_click} title="Click to edit">
                                    { &i.title }
                                    if *saving { <span class="saving-indicator">{ " (saving...)" }</span> }
                                </h2>
                            }
                        </div>
                        <button type="button" class="modal-close" onclick={on_close_btn}>{ "×" }</button>
                    </div>
                    <div class="modal-body">
                        <div class="item-meta">
                            <span class="meta-item">
                                <strong>{ "Created: " }</strong>{ format_naive_date(&i.create_date) }
                            </span>
                            <span class="meta-item">
                                <strong>{ "Due: " }</strong>{ i.due_date.as_ref().map(format_naive_date).unwrap_or_else(|| "TBD".to_string()) }
                            </span>
                            <span class="meta-item">
                                <strong>{ "Category: " }</strong>{ &i.category }
                            </span>
                            <span class={classes!("meta-item", priority_class(&i.priority))}>
                                <strong>{ "Priority: " }</strong>{ &i.priority }
                            </span>
                            <span class="meta-item">
                                <strong>{ "Status: " }</strong>
                                <select
                                    class={classes!("status-select", status_class(&i.status))}
                                    onchange={on_status_change}
                                    disabled={*changing_status}
                                    value={i.status.clone()}
                                >
                                    { for STATUSES.iter().map(|(display, _)| {
                                        html! {
                                            <option value={*display} selected={*display == i.status}>{ display }</option>
                                        }
                                    })}
                                </select>
                                if *changing_status {
                                    <span class="saving-indicator">{ " (saving...)" }</span>
                                }
                            </span>
                        </div>

                        <div class="description-section">
                            <h3>{ "Description" }</h3>
                            if *editing_description {
                                <textarea
                                    class="description-edit-textarea"
                                    value={(*edit_description_value).clone()}
                                    oninput={on_description_input}
                                    onblur={on_description_blur}
                                    onkeydown={on_description_keydown}
                                    rows="4"
                                    placeholder="Add a description..."
                                    autofocus=true
                                />
                                <p class="edit-hint">{ "Press Escape to cancel, click outside to save" }</p>
                            } else {
                                <div class="description-content editable" onclick={on_description_click} title="Click to edit">
                                    if let Some(desc) = &i.description {
                                        if !desc.is_empty() {
                                            { linkify_text(desc) }
                                        } else {
                                            <span class="placeholder">{ "Click to add description..." }</span>
                                        }
                                    } else {
                                        <span class="placeholder">{ "Click to add description..." }</span>
                                    }
                                </div>
                            }
                        </div>

                        <h3>{ "Activity" }</h3>

                        <form class="add-update-form" onsubmit={on_add_update}>
                            <textarea
                                placeholder="Add a note..."
                                value={(*new_update_content).clone()}
                                oninput={on_update_change}
                                rows="3"
                            />
                            <button type="submit" class="btn btn-primary" disabled={*submitting || new_update_content.trim().is_empty()}>
                                { if *submitting { "Adding..." } else { "Add Note" } }
                            </button>
                        </form>

                        <div class="history-scroll">
                            if history.is_empty() {
                                <p class="no-updates">{ "No activity yet." }</p>
                            } else {
                                <ul class="updates-list">
                                    { for history.iter().map(|entry| {
                                        match entry {
                                            HistoryEntry::Note { timestamp, author, content } => {
                                                html! {
                                                    <li class="update-item">
                                                        <div class="update-header">
                                                            <span class="update-author">{ author }</span>
                                                            <span class="update-date">{ format_datetime(timestamp) }</span>
                                                        </div>
                                                        <div class="update-content">{ linkify_text(content) }</div>
                                                    </li>
                                                }
                                            }
                                            HistoryEntry::StatusChange { timestamp, changed_by, from_status, to_status, comment } => {
                                                html! {
                                                    <li class="update-item status-change-item">
                                                        <div class="update-header">
                                                            <span class="status-change-label">{ format!("Status changed by {}", changed_by) }</span>
                                                            <span class="update-date">{ format_datetime(timestamp) }</span>
                                                        </div>
                                                        <div class="status-change-content">
                                                            if let Some(from) = from_status {
                                                                <span class={classes!("status-badge", status_class(from))}>{ from }</span>
                                                                <span class="arrow">{ " → " }</span>
                                                            }
                                                            <span class={classes!("status-badge", status_class(to_status))}>{ to_status }</span>
                                                            if let Some(c) = comment {
                                                                if !c.is_empty() {
                                                                    <div class="status-comment">{ c }</div>
                                                                }
                                                            }
                                                        </div>
                                                    </li>
                                                }
                                            }
                                        }
                                    })}
                                </ul>
                            }
                        </div>
                    </div>
                } else {
                    <div class="modal-header">
                        <h2>{ "Item not found" }</h2>
                        <button type="button" class="modal-close" onclick={on_close_btn}>{ "×" }</button>
                    </div>
                }
            </div>
        </div>
    }
}
