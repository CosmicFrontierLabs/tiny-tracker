use chrono::{DateTime, Utc};
use gloo_net::http::Request;
use shared::{
    risk_level, risk_score, ActionItemResponse, CreateRiskNote, RiskHistoryField,
    RiskHistoryResponse, RiskNoteResponse, RiskResponse, RiskStatus, RiskTrend, UpdateRisk,
};
use yew::prelude::*;
use yew_router::prelude::*;

use crate::components::ItemPicker;
use crate::pages::risk_style::{
    blank_to_none, event_value, fmt_num, format_datetime, level_class, parse_scale, status_class,
    trend_class,
};
use crate::pages::status_style::status_class as item_status_class;
use crate::Route;

#[derive(Properties, PartialEq)]
pub struct RiskDetailModalProps {
    pub risk_id: String,
    /// Shows the delete control. The server rejects a non-admin delete
    /// regardless.
    #[prop_or_default]
    pub is_admin: bool,
    pub on_close: Callback<()>,
}

/// The editable fields, as strings, while the form is open.
#[derive(Clone, PartialEq, Default)]
struct EditFields {
    component: String,
    category: String,
    title: String,
    cause: String,
    effect: String,
    severity: String,
    likelihood: String,
    mitigation: String,
    test_type: String,
    verification: String,
    status: RiskStatusOrDefault,
    trend: RiskTrendOrDefault,
    opened_on: String,
    /// Why this edit is being made; recorded with any status or score change.
    comment: String,
}

/// One entry of the merged timeline: a note, or every field one edit moved.
#[derive(Clone, PartialEq)]
enum TimelineEntry {
    Note {
        timestamp: DateTime<Utc>,
        author: String,
        content: String,
    },
    Change {
        timestamp: DateTime<Utc>,
        changed_by: String,
        /// `(field, old, new)` for each field that moved in this edit.
        changes: Vec<(RiskHistoryField, Option<String>, Option<String>)>,
        comment: Option<String>,
    },
}

/// Rows from one edit share a `changed_at` and author; fold them into one
/// event so "sev 4 → 2.5, lik 3 → 1" reads as a single line.
fn group_history(mut rows: Vec<RiskHistoryResponse>) -> Vec<TimelineEntry> {
    rows.sort_by_key(|r| (r.changed_at, r.id));
    let mut out: Vec<TimelineEntry> = Vec::new();
    for r in rows {
        if let Some(TimelineEntry::Change {
            timestamp,
            changed_by,
            changes,
            ..
        }) = out.last_mut()
        {
            if *timestamp == r.changed_at && *changed_by == r.changed_by_name {
                changes.push((r.field, r.old_value, r.new_value));
                continue;
            }
        }
        out.push(TimelineEntry::Change {
            timestamp: r.changed_at,
            changed_by: r.changed_by_name,
            changes: vec![(r.field, r.old_value, r.new_value)],
            comment: r.comment,
        });
    }
    out
}

fn field_label(f: RiskHistoryField) -> &'static str {
    match f {
        RiskHistoryField::Created => "opened",
        RiskHistoryField::Status => "status",
        RiskHistoryField::Trend => "trend",
        RiskHistoryField::Severity => "severity",
        RiskHistoryField::Likelihood => "likelihood",
    }
}

// `RiskStatus` / `RiskTrend` have no `Default`; wrapping keeps `EditFields`
// derivable without putting an arbitrary default into the shared crate.
#[derive(Clone, Copy, PartialEq)]
struct RiskStatusOrDefault(RiskStatus);
impl Default for RiskStatusOrDefault {
    fn default() -> Self {
        Self(RiskStatus::Open)
    }
}
#[derive(Clone, Copy, PartialEq)]
struct RiskTrendOrDefault(RiskTrend);
impl Default for RiskTrendOrDefault {
    fn default() -> Self {
        Self(RiskTrend::New)
    }
}

impl EditFields {
    fn from_risk(r: &RiskResponse) -> Self {
        Self {
            component: r.component.clone().unwrap_or_default(),
            category: r.category.clone(),
            title: r.title.clone(),
            cause: r.cause.clone().unwrap_or_default(),
            effect: r.effect.clone().unwrap_or_default(),
            severity: fmt_num(r.severity),
            likelihood: fmt_num(r.likelihood),
            mitigation: r.mitigation.clone().unwrap_or_default(),
            test_type: r.test_type.clone().unwrap_or_default(),
            verification: r.verification.clone().unwrap_or_default(),
            status: RiskStatusOrDefault(r.status),
            trend: RiskTrendOrDefault(r.trend),
            opened_on: r.opened_on.to_string(),
            comment: String::new(),
        }
    }

    /// Build the PATCH body. Every field is sent, so the form is the whole
    /// truth and a cleared textarea clears the column (`Some(None)` → `null`).
    fn to_update(&self) -> Result<UpdateRisk, String> {
        let severity =
            parse_scale(&self.severity).ok_or("Severity must be a number from 0 to 5")?;
        let likelihood =
            parse_scale(&self.likelihood).ok_or("Likelihood must be a number from 0 to 5")?;
        if self.title.trim().is_empty() {
            return Err("A risk statement is required".into());
        }
        if self.category.trim().is_empty() {
            return Err("A category is required".into());
        }
        let opened_on = self
            .opened_on
            .trim()
            .parse()
            .map_err(|_| "Opened date is not a valid date")?;
        Ok(UpdateRisk {
            component: Some(blank_to_none(&self.component)),
            category: Some(self.category.trim().to_string()),
            title: Some(self.title.trim().to_string()),
            cause: Some(blank_to_none(&self.cause)),
            effect: Some(blank_to_none(&self.effect)),
            severity: Some(severity),
            likelihood: Some(likelihood),
            mitigation: Some(blank_to_none(&self.mitigation)),
            test_type: Some(blank_to_none(&self.test_type)),
            verification: Some(blank_to_none(&self.verification)),
            status: Some(self.status.0),
            trend: Some(self.trend.0),
            opened_on: Some(opened_on),
            comment: blank_to_none(&self.comment),
        })
    }
}

fn text_block(label: &'static str, value: Option<&str>) -> Html {
    html! {
        <div class="risk-field">
            <h4>{ label }</h4>
            <div class="risk-field-value">
                if let Some(v) = value.filter(|v| !v.trim().is_empty()) {
                    { v }
                } else {
                    <span class="placeholder">{ "-" }</span>
                }
            </div>
        </div>
    }
}

#[function_component(RiskDetailModal)]
pub fn risk_detail_modal(props: &RiskDetailModalProps) -> Html {
    let navigator = use_navigator().unwrap();
    let risk = use_state(|| None::<RiskResponse>);
    let linked_items = use_state(Vec::<ActionItemResponse>::new);
    let all_items = use_state(Vec::<ActionItemResponse>::new);
    let loading = use_state(|| true);
    let error = use_state(|| None::<String>);
    let refresh_trigger = use_state(|| 0u32);

    let editing = use_state(|| false);
    let fields = use_state(EditFields::default);
    let saving = use_state(|| false);
    let save_error = use_state(|| None::<String>);

    let link_selection = use_state(Vec::<String>::new);
    let linking = use_state(|| false);
    let link_error = use_state(|| None::<String>);

    let confirming_delete = use_state(|| false);
    let deleting = use_state(|| false);
    let delete_error = use_state(|| None::<String>);

    let timeline = use_state(Vec::<TimelineEntry>::new);
    let new_note = use_state(String::new);
    let adding_note = use_state(|| false);

    let risk_id = props.risk_id.clone();

    {
        let risk = risk.clone();
        let linked_items = linked_items.clone();
        let all_items = all_items.clone();
        let timeline = timeline.clone();
        let loading = loading.clone();
        let error = error.clone();
        let refresh = *refresh_trigger;
        let risk_id = risk_id.clone();

        use_effect_with((risk_id, refresh), move |(rid, _)| {
            let rid = rid.clone();
            wasm_bindgen_futures::spawn_local(async move {
                match Request::get(&format!("/api/risks/{}", rid)).send().await {
                    Ok(resp) if resp.ok() => {
                        if let Ok(data) = resp.json::<RiskResponse>().await {
                            risk.set(Some(data));
                        }
                    }
                    Ok(resp) => error.set(Some(format!("Failed to load risk: {}", resp.status()))),
                    Err(e) => error.set(Some(format!("Request error: {}", e))),
                }

                if let Ok(resp) = Request::get(&format!("/api/risks/{}/items", rid))
                    .send()
                    .await
                {
                    if let Ok(data) = resp.json::<Vec<ActionItemResponse>>().await {
                        linked_items.set(data);
                    }
                }

                if let Ok(resp) = Request::get("/api/items").send().await {
                    if let Ok(data) = resp.json::<Vec<ActionItemResponse>>().await {
                        all_items.set(data);
                    }
                }

                // Notes and change history, merged newest-first.
                let mut entries: Vec<TimelineEntry> = Vec::new();
                if let Ok(resp) = Request::get(&format!("/api/risks/{}/notes", rid))
                    .send()
                    .await
                {
                    if let Ok(notes) = resp.json::<Vec<RiskNoteResponse>>().await {
                        entries.extend(notes.into_iter().map(|n| TimelineEntry::Note {
                            timestamp: n.created_at,
                            author: n.author_name,
                            content: n.content,
                        }));
                    }
                }
                if let Ok(resp) = Request::get(&format!("/api/risks/{}/history", rid))
                    .send()
                    .await
                {
                    if let Ok(rows) = resp.json::<Vec<RiskHistoryResponse>>().await {
                        entries.extend(group_history(rows));
                    }
                }
                entries.sort_by_key(|e| {
                    std::cmp::Reverse(match e {
                        TimelineEntry::Note { timestamp, .. } => *timestamp,
                        TimelineEntry::Change { timestamp, .. } => *timestamp,
                    })
                });
                timeline.set(entries);

                loading.set(false);
            });
            || ()
        });
    }

    let on_backdrop_click = {
        let on_close = props.on_close.clone();
        Callback::from(move |_| on_close.emit(()))
    };
    let on_modal_click = Callback::from(|e: MouseEvent| e.stop_propagation());
    let on_close_btn = {
        let on_close = props.on_close.clone();
        Callback::from(move |_| on_close.emit(()))
    };

    // --- editing ---------------------------------------------------------

    let on_edit = {
        let editing = editing.clone();
        let fields = fields.clone();
        let risk = risk.clone();
        let save_error = save_error.clone();
        Callback::from(move |_| {
            if let Some(r) = &*risk {
                fields.set(EditFields::from_risk(r));
                save_error.set(None);
                editing.set(true);
            }
        })
    };

    let on_cancel_edit = {
        let editing = editing.clone();
        Callback::from(move |_| editing.set(false))
    };

    // One handler factory for every text-ish field: copy the struct, poke one
    // field, set it back.
    let field_handler = |apply: fn(&mut EditFields, String)| {
        let fields = fields.clone();
        Callback::from(move |e: InputEvent| {
            let mut f = (*fields).clone();
            apply(&mut f, event_value(&e));
            fields.set(f);
        })
    };
    let select_handler = |apply: fn(&mut EditFields, String)| {
        let fields = fields.clone();
        Callback::from(move |e: Event| {
            let mut f = (*fields).clone();
            apply(&mut f, event_value(&e));
            fields.set(f);
        })
    };

    let on_save = {
        let editing = editing.clone();
        let fields = fields.clone();
        let saving = saving.clone();
        let save_error = save_error.clone();
        let refresh_trigger = refresh_trigger.clone();
        let risk_id = risk_id.clone();
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            let body = match fields.to_update() {
                Ok(b) => b,
                Err(msg) => {
                    save_error.set(Some(msg));
                    return;
                }
            };
            let editing = editing.clone();
            let saving = saving.clone();
            let save_error = save_error.clone();
            let refresh_trigger = refresh_trigger.clone();
            let risk_id = risk_id.clone();

            saving.set(true);
            save_error.set(None);

            wasm_bindgen_futures::spawn_local(async move {
                match Request::patch(&format!("/api/risks/{}", risk_id))
                    .header("Content-Type", "application/json")
                    .body(serde_json::to_string(&body).unwrap())
                    .unwrap()
                    .send()
                    .await
                {
                    Ok(resp) if resp.ok() => {
                        editing.set(false);
                        refresh_trigger.set(*refresh_trigger + 1);
                    }
                    Ok(resp) => {
                        let msg = resp
                            .text()
                            .await
                            .unwrap_or_else(|_| "Failed to save".into());
                        save_error.set(Some(msg));
                    }
                    Err(e) => save_error.set(Some(format!("Request error: {}", e))),
                }
                saving.set(false);
            });
        })
    };

    // --- linking ---------------------------------------------------------

    let on_link_selection = {
        let link_selection = link_selection.clone();
        Callback::from(move |ids: Vec<String>| link_selection.set(ids))
    };

    let on_link = {
        let link_selection = link_selection.clone();
        let linking = linking.clone();
        let link_error = link_error.clone();
        let refresh_trigger = refresh_trigger.clone();
        let risk_id = risk_id.clone();
        Callback::from(move |_: MouseEvent| {
            let item_ids = (*link_selection).clone();
            if item_ids.is_empty() {
                return;
            }
            let link_selection = link_selection.clone();
            let linking = linking.clone();
            let link_error = link_error.clone();
            let refresh_trigger = refresh_trigger.clone();
            let risk_id = risk_id.clone();

            linking.set(true);
            link_error.set(None);

            wasm_bindgen_futures::spawn_local(async move {
                let body = serde_json::to_string(&shared::LinkItems { item_ids }).unwrap();
                match Request::post(&format!("/api/risks/{}/items", risk_id))
                    .header("Content-Type", "application/json")
                    .body(body)
                    .unwrap()
                    .send()
                    .await
                {
                    Ok(resp) if resp.ok() => {
                        link_selection.set(Vec::new());
                        refresh_trigger.set(*refresh_trigger + 1);
                    }
                    Ok(resp) => {
                        let msg = resp
                            .text()
                            .await
                            .unwrap_or_else(|_| format!("Failed to link: {}", resp.status()));
                        link_error.set(Some(msg));
                    }
                    Err(e) => link_error.set(Some(format!("Request error: {}", e))),
                }
                linking.set(false);
            });
        })
    };

    let unlink = {
        let refresh_trigger = refresh_trigger.clone();
        let link_error = link_error.clone();
        let risk_id = risk_id.clone();
        move |item_id: String| {
            let refresh_trigger = refresh_trigger.clone();
            let link_error = link_error.clone();
            let risk_id = risk_id.clone();
            Callback::from(move |e: MouseEvent| {
                // The row itself navigates to the item; this button must not.
                e.stop_propagation();
                let refresh_trigger = refresh_trigger.clone();
                let link_error = link_error.clone();
                let url = format!("/api/items/{}/risks/{}", item_id, risk_id);
                wasm_bindgen_futures::spawn_local(async move {
                    match Request::delete(&url).send().await {
                        Ok(resp) if resp.ok() => refresh_trigger.set(*refresh_trigger + 1),
                        Ok(resp) => {
                            link_error.set(Some(format!("Failed to unlink: {}", resp.status())))
                        }
                        Err(e) => link_error.set(Some(format!("Request error: {}", e))),
                    }
                });
            })
        }
    };

    // --- deleting --------------------------------------------------------

    let on_delete_click = {
        let confirming_delete = confirming_delete.clone();
        let delete_error = delete_error.clone();
        Callback::from(move |_| {
            delete_error.set(None);
            confirming_delete.set(true);
        })
    };
    let on_delete_cancel = {
        let confirming_delete = confirming_delete.clone();
        Callback::from(move |_| confirming_delete.set(false))
    };
    let on_delete_confirm = {
        let risk_id = risk_id.clone();
        let deleting = deleting.clone();
        let delete_error = delete_error.clone();
        let confirming_delete = confirming_delete.clone();
        let on_close = props.on_close.clone();
        Callback::from(move |_| {
            let risk_id = risk_id.clone();
            let deleting = deleting.clone();
            let delete_error = delete_error.clone();
            let confirming_delete = confirming_delete.clone();
            let on_close = on_close.clone();
            deleting.set(true);
            wasm_bindgen_futures::spawn_local(async move {
                match Request::delete(&format!("/api/risks/{}", risk_id))
                    .send()
                    .await
                {
                    Ok(resp) if resp.ok() => on_close.emit(()),
                    Ok(resp) if resp.status() == 403 => {
                        delete_error.set(Some("Only an administrator can delete risks.".into()));
                        confirming_delete.set(false);
                        deleting.set(false);
                    }
                    Ok(resp) => {
                        delete_error.set(Some(format!("Failed to delete: {}", resp.status())));
                        confirming_delete.set(false);
                        deleting.set(false);
                    }
                    Err(e) => {
                        delete_error.set(Some(format!("Request error: {}", e)));
                        confirming_delete.set(false);
                        deleting.set(false);
                    }
                }
            });
        })
    };

    // --- notes -----------------------------------------------------------

    let on_note_input = {
        let new_note = new_note.clone();
        Callback::from(move |e: InputEvent| new_note.set(event_value(&e)))
    };

    let on_add_note = {
        let new_note = new_note.clone();
        let adding_note = adding_note.clone();
        let refresh_trigger = refresh_trigger.clone();
        let risk_id = risk_id.clone();
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            let content = new_note.trim().to_string();
            if content.is_empty() {
                return;
            }
            let new_note = new_note.clone();
            let adding_note = adding_note.clone();
            let refresh_trigger = refresh_trigger.clone();
            let risk_id = risk_id.clone();
            adding_note.set(true);
            wasm_bindgen_futures::spawn_local(async move {
                let body = CreateRiskNote {
                    note_date: None,
                    content,
                };
                if let Ok(resp) = Request::post(&format!("/api/risks/{}/notes", risk_id))
                    .header("Content-Type", "application/json")
                    .body(serde_json::to_string(&body).unwrap())
                    .unwrap()
                    .send()
                    .await
                {
                    if resp.ok() {
                        new_note.set(String::new());
                        refresh_trigger.set(*refresh_trigger + 1);
                    }
                }
                adding_note.set(false);
            });
        })
    };

    // Already-linked items are hidden from the picker rather than disabled.
    let linked_ids: Vec<String> = linked_items.iter().map(|i| i.id.clone()).collect();
    let can_link_more = all_items.iter().any(|i| !linked_ids.contains(&i.id));

    let preview = match (
        parse_scale(&fields.severity),
        parse_scale(&fields.likelihood),
    ) {
        (Some(s), Some(l)) => {
            let score = risk_score(s, l);
            Some((score, risk_level(score)))
        }
        _ => None,
    };

    html! {
        <div class="modal-backdrop" onclick={on_backdrop_click}>
            <div class="modal modal-large" onclick={on_modal_click}>
                if *loading {
                    <div class="modal-header"><h2>{ "Loading..." }</h2></div>
                } else if let Some(err) = (*error).clone() {
                    <div class="modal-header">
                        <h2>{ "Error" }</h2>
                        <button type="button" class="modal-close" onclick={on_close_btn.clone()}>{ "×" }</button>
                    </div>
                    <div class="modal-body"><p class="error">{ err }</p></div>
                } else if let Some(r) = (*risk).clone() {
                    <div class="modal-header">
                        <div class="title-container">
                            <span class="item-id-badge risk-id-badge">{ &r.id }</span>
                            <h2 class="risk-detail-title">{ &r.title }</h2>
                        </div>
                        <button type="button" class="modal-close" onclick={on_close_btn}>{ "×" }</button>
                    </div>
                    <div class="modal-body">
                        if *editing {
                            <form class="risk-form risk-edit-form" onsubmit={on_save}>
                                if let Some(err) = (*save_error).clone() {
                                    <p class="error">{ err }</p>
                                }
                                <div class="form-group">
                                    <label>{ "Risk" }</label>
                                    <input type="text" value={fields.title.clone()} required=true
                                        oninput={field_handler(|f, v| f.title = v)} />
                                </div>
                                <div class="risk-form-grid">
                                    <div class="form-group">
                                        <label>{ "Item" }</label>
                                        <input type="text" value={fields.component.clone()}
                                            oninput={field_handler(|f, v| f.component = v)} />
                                    </div>
                                    <div class="form-group">
                                        <label>{ "Category" }</label>
                                        <input type="text" value={fields.category.clone()} required=true
                                            oninput={field_handler(|f, v| f.category = v)} />
                                    </div>
                                    <div class="form-group">
                                        <label>{ "Cause" }</label>
                                        <textarea rows="3" value={fields.cause.clone()}
                                            oninput={field_handler(|f, v| f.cause = v)} />
                                    </div>
                                    <div class="form-group">
                                        <label>{ "Effect" }</label>
                                        <textarea rows="3" value={fields.effect.clone()}
                                            oninput={field_handler(|f, v| f.effect = v)} />
                                    </div>
                                </div>
                                <div class="risk-form-grid risk-form-scores">
                                    <div class="form-group">
                                        <label>{ "Severity (0-5)" }</label>
                                        <input type="number" min="0" max="5" step="0.5" required=true
                                            value={fields.severity.clone()}
                                            oninput={field_handler(|f, v| f.severity = v)} />
                                    </div>
                                    <div class="form-group">
                                        <label>{ "Likelihood (0-5)" }</label>
                                        <input type="number" min="0" max="5" step="0.5" required=true
                                            value={fields.likelihood.clone()}
                                            oninput={field_handler(|f, v| f.likelihood = v)} />
                                    </div>
                                    <div class="form-group">
                                        <label>{ "Score" }</label>
                                        <div class="risk-score-preview">
                                            if let Some((score, level)) = preview {
                                                <span class={classes!("risk-score", level_class(level))}>{ fmt_num(score) }</span>
                                                <span class={classes!("risk-badge", level_class(level))}>{ level.as_str() }</span>
                                            } else {
                                                <span class="placeholder">{ "-" }</span>
                                            }
                                        </div>
                                    </div>
                                    <div class="form-group">
                                        <label>{ "Status" }</label>
                                        <select onchange={select_handler(|f, v| {
                                            if let Some(s) = RiskStatus::from_label(&v) { f.status = RiskStatusOrDefault(s); }
                                        })}>
                                            { for RiskStatus::all().iter().map(|s| html! {
                                                <option value={s.as_str()} selected={fields.status.0 == *s}>{ s.as_str() }</option>
                                            })}
                                        </select>
                                    </div>
                                    <div class="form-group">
                                        <label>{ "Trend" }</label>
                                        <select onchange={select_handler(|f, v| {
                                            if let Some(t) = RiskTrend::from_label(&v) { f.trend = RiskTrendOrDefault(t); }
                                        })}>
                                            { for RiskTrend::all().iter().map(|t| html! {
                                                <option value={t.as_str()} selected={fields.trend.0 == *t}>{ t.as_str() }</option>
                                            })}
                                        </select>
                                    </div>
                                </div>
                                <div class="form-group">
                                    <label>{ "Mitigation" }</label>
                                    <textarea rows="3" value={fields.mitigation.clone()}
                                        oninput={field_handler(|f, v| f.mitigation = v)} />
                                </div>
                                <div class="risk-form-grid">
                                    <div class="form-group">
                                        <label>{ "Test type" }</label>
                                        <input type="text" value={fields.test_type.clone()}
                                            oninput={field_handler(|f, v| f.test_type = v)} />
                                    </div>
                                    <div class="form-group">
                                        <label>{ "Verification (test IDs)" }</label>
                                        <input type="text" value={fields.verification.clone()}
                                            oninput={field_handler(|f, v| f.verification = v)} />
                                    </div>
                                    <div class="form-group">
                                        <label>{ "Opened" }</label>
                                        <input type="date" value={fields.opened_on.clone()} required=true
                                            oninput={field_handler(|f, v| f.opened_on = v)} />
                                    </div>
                                </div>
                                <div class="form-group">
                                    <label>{ "Why? (recorded with any status, trend or score change)" }</label>
                                    <input type="text" value={fields.comment.clone()}
                                        placeholder="e.g. TVAC data in, conductance within model"
                                        oninput={field_handler(|f, v| f.comment = v)} />
                                </div>
                                <div class="form-actions">
                                    <button type="submit" class="btn btn-primary" disabled={*saving}>
                                        { if *saving { "Saving..." } else { "Save" } }
                                    </button>
                                    <button type="button" class="btn" onclick={on_cancel_edit} disabled={*saving}>{ "Cancel" }</button>
                                </div>
                            </form>
                        } else {
                            <div class="item-meta risk-meta">
                                <span class="meta-item">
                                    <strong>{ "Score: " }</strong>
                                    <span class={classes!("risk-score", level_class(r.level))}>{ fmt_num(r.score) }</span>
                                    <span class={classes!("risk-badge", level_class(r.level))}>{ r.level.as_str() }</span>
                                    <span class="risk-score-parts">{ format!("(sev {} × lik {})", fmt_num(r.severity), fmt_num(r.likelihood)) }</span>
                                </span>
                                <span class="meta-item">
                                    <strong>{ "Status: " }</strong><span class={status_class(r.status)}>{ r.status.as_str() }</span>
                                </span>
                                <span class="meta-item">
                                    <strong>{ "Trend: " }</strong><span class={trend_class(r.trend)}>{ r.trend.as_str() }</span>
                                </span>
                                <span class="meta-item">
                                    <strong>{ "Item: " }</strong>{ r.component.as_deref().unwrap_or("-") }
                                </span>
                                <span class="meta-item">
                                    <strong>{ "Category: " }</strong>{ &r.category }
                                </span>
                                <span class="meta-item">
                                    <strong>{ "Opened: " }</strong>{ r.opened_on.to_string() }
                                </span>
                                <span class="meta-item">
                                    <strong>{ "Updated: " }</strong>{ r.updated_at.date_naive().to_string() }
                                </span>
                                <button type="button" class="btn btn-small btn-secondary risk-edit-btn" onclick={on_edit}>
                                    { "Edit" }
                                </button>
                            </div>

                            <div class="risk-fields">
                                { text_block("Cause", r.cause.as_deref()) }
                                { text_block("Effect", r.effect.as_deref()) }
                                { text_block("Mitigation", r.mitigation.as_deref()) }
                                <div class="risk-field">
                                    <h4>{ "Verification" }</h4>
                                    <div class="risk-field-value">
                                        if let Some(t) = r.test_type.as_deref() {
                                            <span class="tracking-id-chip">{ t }</span>{ " " }
                                        }
                                        { r.verification.as_deref().unwrap_or("-") }
                                    </div>
                                </div>
                            </div>
                        }

                        <div class="risk-links-section">
                            <h3>{ format!("Action Items ({})", linked_items.len()) }</h3>
                            if let Some(err) = (*link_error).clone() {
                                <p class="error">{ err }</p>
                            }
                            if linked_items.is_empty() {
                                <p class="no-updates">{ "No action items are addressing this risk yet." }</p>
                            } else {
                                <ul class="risk-linked-list">
                                    { for linked_items.iter().map(|i| {
                                        let id = i.id.clone();
                                        let navigator = navigator.clone();
                                        let on_row = Callback::from(move |_: MouseEvent| {
                                            navigator.push(&Route::Item { id: id.clone() });
                                        });
                                        html! {
                                            <li class="risk-linked-row" onclick={on_row}>
                                                <span class="item-id">{ &i.id }</span>
                                                <span class="risk-linked-title">{ &i.title }</span>
                                                <span class={classes!("status-badge", item_status_class(&i.status))}>{ &i.status }</span>
                                                <button type="button" class="btn btn-small" title="Unlink from this risk" onclick={unlink(i.id.clone())}>
                                                    { "Unlink" }
                                                </button>
                                            </li>
                                        }
                                    })}
                                </ul>
                            }
                            if can_link_more {
                                <ItemPicker
                                    items={(*all_items).clone()}
                                    selected={(*link_selection).clone()}
                                    exclude={linked_ids.clone()}
                                    on_change={on_link_selection}
                                    disabled={*linking}
                                />
                                <div class="risk-link-actions">
                                    <button type="button" class="btn btn-small btn-primary" onclick={on_link}
                                        disabled={*linking || link_selection.is_empty()}>
                                        { if *linking {
                                            "Linking...".to_string()
                                        } else if link_selection.len() == 1 {
                                            "Link 1 item".to_string()
                                        } else {
                                            format!("Link {} items", link_selection.len())
                                        } }
                                    </button>
                                </div>
                            } else {
                                <p class="no-updates">{ "Every action item is already linked." }</p>
                            }
                        </div>

                        <h3>{ "Activity" }</h3>

                        <form class="add-update-form" onsubmit={on_add_note}>
                            <textarea
                                placeholder="Add a note..."
                                value={(*new_note).clone()}
                                oninput={on_note_input}
                                rows="3"
                            />
                            <button type="submit" class="btn btn-primary" disabled={*adding_note || new_note.trim().is_empty()}>
                                { if *adding_note { "Adding..." } else { "Add Note" } }
                            </button>
                        </form>

                        <div class="history-scroll">
                            if timeline.is_empty() {
                                <p class="no-updates">{ "No activity yet." }</p>
                            } else {
                                <ul class="updates-list">
                                    { for timeline.iter().map(|entry| match entry {
                                        TimelineEntry::Note { timestamp, author, content } => html! {
                                            <li class="update-item">
                                                <div class="update-header">
                                                    <span class="update-author">{ author }</span>
                                                    <span class="update-date">{ format_datetime(timestamp) }</span>
                                                </div>
                                                <div class="update-content">{ content }</div>
                                            </li>
                                        },
                                        TimelineEntry::Change { timestamp, changed_by, changes, comment } => html! {
                                            <li class="update-item risk-change-item">
                                                <div class="update-header">
                                                    <span class="risk-change-label">{ format!("Changed by {}", changed_by) }</span>
                                                    <span class="update-date">{ format_datetime(timestamp) }</span>
                                                </div>
                                                <div class="risk-change-content">
                                                    { for changes.iter().map(|(field, old, new)| html! {
                                                        <span class="risk-change-pill">
                                                            <strong>{ field_label(*field) }</strong>
                                                            if let Some(n) = new {
                                                                { " " }
                                                                if let Some(o) = old {
                                                                    <span class="risk-change-old">{ o }</span>
                                                                    <span class="arrow">{ " → " }</span>
                                                                }
                                                                <span class="risk-change-new">{ n }</span>
                                                            }
                                                        </span>
                                                    })}
                                                    if let Some(c) = comment.as_deref().filter(|c| !c.is_empty()) {
                                                        <div class="status-comment">{ c }</div>
                                                    }
                                                </div>
                                            </li>
                                        },
                                    })}
                                </ul>
                            }
                        </div>

                        if props.is_admin && !*editing {
                            <div class="danger-zone">
                                if let Some(err) = (*delete_error).clone() {
                                    <p class="error">{ err }</p>
                                }
                                if *confirming_delete {
                                    <span class="delete-confirm-prompt">{ "Delete this risk?" }</span>
                                    <button type="button" class="btn btn-small btn-danger" onclick={on_delete_confirm} disabled={*deleting}>
                                        { if *deleting { "Deleting..." } else { "Yes, delete" } }
                                    </button>
                                    <button type="button" class="btn btn-small" onclick={on_delete_cancel} disabled={*deleting}>{ "Cancel" }</button>
                                } else {
                                    <button type="button" class="btn btn-small btn-danger" onclick={on_delete_click}>{ "Delete risk" }</button>
                                }
                            </div>
                        }
                    </div>
                } else {
                    <div class="modal-header">
                        <h2>{ "Risk not found" }</h2>
                        <button type="button" class="modal-close" onclick={on_close_btn}>{ "×" }</button>
                    </div>
                }
            </div>
        </div>
    }
}
