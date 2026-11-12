//! A filterable checkbox list for choosing several action items at once.
//!
//! Used by the new-risk form (link at creation) and the risk detail modal
//! (link more later). The parent owns the selection; this only renders it
//! and reports changes.

use shared::ActionItemResponse;
use yew::prelude::*;

use crate::pages::risk_style::event_value;
use crate::pages::status_style::status_class;

#[derive(Properties, PartialEq)]
pub struct ItemPickerProps {
    pub items: Vec<ActionItemResponse>,
    pub selected: Vec<String>,
    pub on_change: Callback<Vec<String>>,
    /// Items to leave out entirely, e.g. ones already linked.
    #[prop_or_default]
    pub exclude: Vec<String>,
    #[prop_or_default]
    pub disabled: bool,
}

#[function_component(ItemPicker)]
pub fn item_picker(props: &ItemPickerProps) -> Html {
    let filter = use_state(String::new);

    let on_filter = {
        let filter = filter.clone();
        Callback::from(move |e: InputEvent| filter.set(event_value(&e)))
    };

    let needle = filter.to_lowercase();
    let mut visible: Vec<&ActionItemResponse> = props
        .items
        .iter()
        .filter(|i| !props.exclude.contains(&i.id))
        .filter(|i| {
            needle.is_empty()
                || i.id.to_lowercase().contains(&needle)
                || i.title.to_lowercase().contains(&needle)
        })
        .collect();
    // Open work first; completed items are still linkable but sink.
    visible.sort_by_key(|i| (i.status == "Complete", i.id.clone()));

    let toggle = |id: String| {
        let selected = props.selected.clone();
        let on_change = props.on_change.clone();
        Callback::from(move |_: Event| {
            let mut next = selected.clone();
            if let Some(pos) = next.iter().position(|s| *s == id) {
                next.remove(pos);
            } else {
                next.push(id.clone());
            }
            on_change.emit(next);
        })
    };

    let total = props
        .items
        .iter()
        .filter(|i| !props.exclude.contains(&i.id))
        .count();

    html! {
        <div class="item-picker">
            <div class="item-picker-filter">
                <input
                    type="text"
                    placeholder="Filter by ID or title..."
                    value={(*filter).clone()}
                    oninput={on_filter}
                    disabled={props.disabled}
                />
                <span class="item-picker-count">
                    { format!("{} selected of {}", props.selected.len(), total) }
                </span>
            </div>
            if visible.is_empty() {
                <div class="item-picker-empty">
                    { if total == 0 { "No action items available." } else { "No items match the filter." } }
                </div>
            } else {
                <ul class="item-picker-list">
                    { for visible.iter().map(|i| {
                        let checked = props.selected.contains(&i.id);
                        html! {
                            <li class={classes!((i.status == "Complete").then_some("item-complete"))}>
                                <label>
                                    <input
                                        type="checkbox"
                                        checked={checked}
                                        onchange={toggle(i.id.clone())}
                                        disabled={props.disabled}
                                    />
                                    <span class="item-id">{ &i.id }</span>
                                    <span class="item-picker-title" title={i.title.clone()}>{ &i.title }</span>
                                    <span class={classes!("status-badge", status_class(&i.status))}>{ &i.status }</span>
                                </label>
                            </li>
                        }
                    })}
                </ul>
            }
        </div>
    }
}
