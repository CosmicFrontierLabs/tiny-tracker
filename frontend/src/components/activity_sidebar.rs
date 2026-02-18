use chrono::{DateTime, Utc};
use gloo_net::http::Request;
use gloo_storage::{LocalStorage, Storage};
use js_sys::{Date, Object, Reflect};
use shared::{ActivityEntry, ActivityEventType};
use wasm_bindgen::JsValue;
use yew::prelude::*;

const STORAGE_KEY: &str = "activity_last_viewed";

fn format_relative_time(dt: &DateTime<Utc>) -> String {
    let js_date = Date::new(&JsValue::from_f64(dt.timestamp_millis() as f64));
    let options = Object::new();
    let _ = Reflect::set(&options, &"month".into(), &"short".into());
    let _ = Reflect::set(&options, &"day".into(), &"numeric".into());
    let _ = Reflect::set(&options, &"hour".into(), &"2-digit".into());
    let _ = Reflect::set(&options, &"minute".into(), &"2-digit".into());

    js_date
        .to_locale_string("default", &options)
        .as_string()
        .unwrap_or_else(|| dt.format("%b %d %H:%M").to_string())
}

#[derive(Properties, PartialEq)]
pub struct ActivitySidebarProps {
    pub on_select_item: Callback<String>,
    pub refresh_trigger: u32,
}

#[function_component(ActivitySidebar)]
pub fn activity_sidebar(props: &ActivitySidebarProps) -> Html {
    let entries = use_state(Vec::<ActivityEntry>::new);
    let loading = use_state(|| true);

    {
        let entries = entries.clone();
        let loading = loading.clone();
        let refresh = props.refresh_trigger;

        use_effect_with(refresh, move |_| {
            wasm_bindgen_futures::spawn_local(async move {
                let since: String =
                    LocalStorage::get(STORAGE_KEY).unwrap_or_else(|_| String::new());

                let url = if since.is_empty() {
                    "/api/activity?limit=50".to_string()
                } else {
                    format!("/api/activity?since={}&limit=50", since)
                };

                match Request::get(&url).send().await {
                    Ok(resp) if resp.ok() => {
                        if let Ok(data) = resp.json::<Vec<ActivityEntry>>().await {
                            entries.set(data);
                        }
                    }
                    _ => {}
                }

                let now = Utc::now().to_rfc3339();
                let _ = LocalStorage::set(STORAGE_KEY, now);

                loading.set(false);
            });
            || ()
        });
    }

    let entry_count = entries.len();

    let activity_content = if *loading {
        html! { <p class="activity-empty">{ "Loading..." }</p> }
    } else if entries.is_empty() {
        html! { <p class="activity-empty">{ "No new activity." }</p> }
    } else {
        html! {
            <ul class="activity-list">
                { for entries.iter().map(|entry| {
                    let item_id = entry.item_id.clone();
                    let on_click = {
                        let on_select = props.on_select_item.clone();
                        let id = item_id.clone();
                        Callback::from(move |_: MouseEvent| {
                            on_select.emit(id.clone());
                        })
                    };

                    let type_class = match entry.event_type {
                        ActivityEventType::NoteAdded => "activity-type-note",
                        ActivityEventType::StatusChanged => "activity-type-status",
                    };

                    let type_label = match entry.event_type {
                        ActivityEventType::NoteAdded => "added a note",
                        ActivityEventType::StatusChanged => "changed status",
                    };

                    html! {
                        <li class={classes!("activity-entry", type_class)} onclick={on_click}>
                            <div class="activity-entry-header">
                                <span class="activity-item-id">{ &entry.item_id }</span>
                                <span class="activity-time">{ format_relative_time(&entry.timestamp) }</span>
                            </div>
                            <div class="activity-actor">
                                { &entry.actor_name }
                                { " " }
                                { type_label }
                            </div>
                            <div class="activity-detail">{ &entry.detail }</div>
                        </li>
                    }
                })}
            </ul>
        }
    };

    let count_badge = if entry_count > 0 {
        html! { <span class="activity-count">{ entry_count }</span> }
    } else {
        html! {}
    };

    html! {
        <div class="activity-sidebar">
            // Desktop: always visible
            <div class="activity-desktop">
                <h3>
                    { "Recent Activity" }
                    { count_badge.clone() }
                </h3>
                { activity_content.clone() }
            </div>
            // Mobile: collapsible
            <details class="activity-mobile">
                <summary>
                    { "Recent Activity " }
                    { count_badge }
                </summary>
                { activity_content }
            </details>
        </div>
    }
}
