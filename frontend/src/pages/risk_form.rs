use gloo_net::http::Request;
use shared::{
    risk_level, risk_score, ActionItemResponse, CreateRisk, RiskResponse, RiskStatus, RiskTrend,
    Vendor,
};
use yew::prelude::*;

use crate::components::ItemPicker;
use crate::pages::risk_style::{blank_to_none, event_value, fmt_num, level_class, parse_scale};

#[derive(Properties, PartialEq)]
pub struct NewRiskModalProps {
    pub vendors: Vec<Vendor>,
    /// Used only to suggest components, categories and test types that are
    /// already in use, so the register stays consistent without a lookup table.
    pub existing: Vec<RiskResponse>,
    pub on_close: Callback<()>,
    pub on_created: Callback<()>,
}

/// Distinct, sorted values of one text field across the existing risks.
fn suggestions<'a>(
    risks: &'a [RiskResponse],
    pick: impl Fn(&'a RiskResponse) -> Option<&'a str>,
) -> Vec<String> {
    let mut v: Vec<String> = risks
        .iter()
        .filter_map(pick)
        .map(|s| s.to_string())
        .collect();
    v.sort();
    v.dedup();
    v
}

fn datalist(id: &'static str, options: &[String]) -> Html {
    html! {
        <datalist id={id}>
            { for options.iter().map(|o| html! { <option value={o.clone()} /> }) }
        </datalist>
    }
}

#[function_component(NewRiskModal)]
pub fn new_risk_modal(props: &NewRiskModalProps) -> Html {
    let vendor_id = use_state(|| props.vendors.first().map(|v| v.id).unwrap_or(0));
    let component = use_state(String::new);
    let category = use_state(String::new);
    let title = use_state(String::new);
    let cause = use_state(String::new);
    let effect = use_state(String::new);
    let severity = use_state(|| "3".to_string());
    let likelihood = use_state(|| "3".to_string());
    let mitigation = use_state(String::new);
    let test_type = use_state(String::new);
    let verification = use_state(String::new);
    let status = use_state(|| RiskStatus::Open);
    let trend = use_state(|| RiskTrend::New);
    let error = use_state(|| None::<String>);
    let submitting = use_state(|| false);

    // Action items the new risk should be linked to from the start.
    let all_items = use_state(Vec::<ActionItemResponse>::new);
    let selected_items = use_state(Vec::<String>::new);
    {
        let all_items = all_items.clone();
        use_effect_with((), move |_| {
            wasm_bindgen_futures::spawn_local(async move {
                if let Ok(resp) = Request::get("/api/items").send().await {
                    if let Ok(data) = resp.json::<Vec<ActionItemResponse>>().await {
                        all_items.set(data);
                    }
                }
            });
            || ()
        });
    }
    let on_items_change = {
        let selected_items = selected_items.clone();
        Callback::from(move |ids: Vec<String>| selected_items.set(ids))
    };

    let component_options = suggestions(&props.existing, |r| r.component.as_deref());
    let category_options = suggestions(&props.existing, |r| Some(r.category.as_str()));
    let test_type_options = suggestions(&props.existing, |r| r.test_type.as_deref());

    // One input handler per text field, all reading the same way.
    let text_handler = |state: UseStateHandle<String>| {
        Callback::from(move |e: InputEvent| state.set(event_value(&e)))
    };
    let on_component = text_handler(component.clone());
    let on_category = text_handler(category.clone());
    let on_title = text_handler(title.clone());
    let on_cause = text_handler(cause.clone());
    let on_effect = text_handler(effect.clone());
    let on_severity = text_handler(severity.clone());
    let on_likelihood = text_handler(likelihood.clone());
    let on_mitigation = text_handler(mitigation.clone());
    let on_test_type = text_handler(test_type.clone());
    let on_verification = text_handler(verification.clone());

    let on_vendor_change = {
        let vendor_id = vendor_id.clone();
        Callback::from(move |e: Event| {
            if let Ok(id) = event_value(&e).parse() {
                vendor_id.set(id);
            }
        })
    };

    let on_status_change = {
        let status = status.clone();
        Callback::from(move |e: Event| {
            if let Some(s) = RiskStatus::from_label(&event_value(&e)) {
                status.set(s);
            }
        })
    };

    let on_trend_change = {
        let trend = trend.clone();
        Callback::from(move |e: Event| {
            if let Some(t) = RiskTrend::from_label(&event_value(&e)) {
                trend.set(t);
            }
        })
    };

    let on_backdrop_click = {
        let on_close = props.on_close.clone();
        Callback::from(move |_| on_close.emit(()))
    };
    let on_modal_click = Callback::from(|e: MouseEvent| e.stop_propagation());
    let on_cancel = {
        let on_close = props.on_close.clone();
        Callback::from(move |_| on_close.emit(()))
    };

    // Live preview so the author sees the band change as they type.
    let preview = match (parse_scale(&severity), parse_scale(&likelihood)) {
        (Some(s), Some(l)) => {
            let score = risk_score(s, l);
            Some((score, risk_level(score)))
        }
        _ => None,
    };

    let on_submit = {
        let vendor_id = vendor_id.clone();
        let component = component.clone();
        let category = category.clone();
        let title = title.clone();
        let cause = cause.clone();
        let effect = effect.clone();
        let severity = severity.clone();
        let likelihood = likelihood.clone();
        let mitigation = mitigation.clone();
        let test_type = test_type.clone();
        let verification = verification.clone();
        let status = status.clone();
        let trend = trend.clone();
        let selected_items = selected_items.clone();
        let error = error.clone();
        let submitting = submitting.clone();
        let on_created = props.on_created.clone();

        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();

            let (Some(sev), Some(lik)) = (parse_scale(&severity), parse_scale(&likelihood)) else {
                error.set(Some(
                    "Severity and likelihood must be numbers from 0 to 5".to_string(),
                ));
                return;
            };
            if title.trim().is_empty() {
                error.set(Some("A risk statement is required".to_string()));
                return;
            }
            if category.trim().is_empty() {
                error.set(Some("A category is required".to_string()));
                return;
            }

            let body = CreateRisk {
                component: blank_to_none(&component),
                category: category.trim().to_string(),
                title: title.trim().to_string(),
                cause: blank_to_none(&cause),
                effect: blank_to_none(&effect),
                severity: sev,
                likelihood: lik,
                mitigation: blank_to_none(&mitigation),
                test_type: blank_to_none(&test_type),
                verification: blank_to_none(&verification),
                status: Some(*status),
                trend: Some(*trend),
                opened_on: None,
                item_ids: (*selected_items).clone(),
            };
            let vendor_id_val = *vendor_id;
            let error = error.clone();
            let submitting = submitting.clone();
            let on_created = on_created.clone();

            submitting.set(true);
            error.set(None);

            wasm_bindgen_futures::spawn_local(async move {
                match Request::post(&format!("/api/vendors/{}/risks", vendor_id_val))
                    .header("Content-Type", "application/json")
                    .body(serde_json::to_string(&body).unwrap())
                    .unwrap()
                    .send()
                    .await
                {
                    Ok(resp) if resp.ok() => on_created.emit(()),
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

    html! {
        <div class="modal-backdrop" onclick={on_backdrop_click}>
            <div class="modal modal-large" onclick={on_modal_click}>
                <div class="modal-header">
                    <h2>{ "New Risk" }</h2>
                </div>

                if let Some(err) = (*error).clone() {
                    <p class="error">{ err }</p>
                }

                <form class="risk-form" onsubmit={on_submit}>
                    { datalist("risk-component-options", &component_options) }
                    { datalist("risk-category-options", &category_options) }
                    { datalist("risk-test-type-options", &test_type_options) }

                    <div class="risk-form-grid">
                        <div class="form-group">
                            <label for="risk-vendor">{ "Vendor" }</label>
                            <select id="risk-vendor" onchange={on_vendor_change} required=true>
                                { for props.vendors.iter().map(|v| html! {
                                    <option value={v.id.to_string()} selected={*vendor_id == v.id}>
                                        { format!("{} - {}", v.prefix, v.name) }
                                    </option>
                                })}
                            </select>
                        </div>
                        <div class="form-group">
                            <label for="risk-component">{ "Item" }</label>
                            <input type="text" id="risk-component" list="risk-component-options"
                                value={(*component).clone()} oninput={on_component}
                                placeholder="Module or area, e.g. NSV-455, All modules, Program" />
                        </div>
                        <div class="form-group">
                            <label for="risk-category">{ "Category" }</label>
                            <input type="text" id="risk-category" list="risk-category-options"
                                value={(*category).clone()} oninput={on_category}
                                placeholder="e.g. Thermal, Firmware, Programmatic" required=true />
                        </div>
                        <div class="form-group">
                            <label for="risk-test-type">{ "Test type" }</label>
                            <input type="text" id="risk-test-type" list="risk-test-type-options"
                                value={(*test_type).clone()} oninput={on_test_type}
                                placeholder="e.g. Functional, Performance, Environmental, N/A" />
                        </div>
                    </div>

                    <div class="form-group">
                        <label for="risk-title">{ "Risk" }</label>
                        <input type="text" id="risk-title" value={(*title).clone()} oninput={on_title}
                            placeholder="What might go wrong" required=true />
                    </div>

                    <div class="risk-form-grid">
                        <div class="form-group">
                            <label for="risk-cause">{ "Cause" }</label>
                            <textarea id="risk-cause" rows="2" value={(*cause).clone()} oninput={on_cause}
                                placeholder="Why it might happen" />
                        </div>
                        <div class="form-group">
                            <label for="risk-effect">{ "Effect" }</label>
                            <textarea id="risk-effect" rows="2" value={(*effect).clone()} oninput={on_effect}
                                placeholder="What it would cost if it did" />
                        </div>
                    </div>

                    <div class="risk-form-grid risk-form-scores">
                        <div class="form-group">
                            <label for="risk-severity">{ "Severity (0-5)" }</label>
                            <input type="number" id="risk-severity" min="0" max="5" step="0.5"
                                value={(*severity).clone()} oninput={on_severity} required=true />
                        </div>
                        <div class="form-group">
                            <label for="risk-likelihood">{ "Likelihood (0-5)" }</label>
                            <input type="number" id="risk-likelihood" min="0" max="5" step="0.5"
                                value={(*likelihood).clone()} oninput={on_likelihood} required=true />
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
                            <label for="risk-status">{ "Status" }</label>
                            <select id="risk-status" onchange={on_status_change}>
                                { for RiskStatus::all().iter().map(|s| html! {
                                    <option value={s.as_str()} selected={*status == *s}>{ s.as_str() }</option>
                                })}
                            </select>
                        </div>
                        <div class="form-group">
                            <label for="risk-trend">{ "Trend" }</label>
                            <select id="risk-trend" onchange={on_trend_change}>
                                { for RiskTrend::all().iter().map(|t| html! {
                                    <option value={t.as_str()} selected={*trend == *t}>{ t.as_str() }</option>
                                })}
                            </select>
                        </div>
                    </div>

                    <div class="form-group">
                        <label for="risk-mitigation">{ "Mitigation" }</label>
                        <textarea id="risk-mitigation" rows="2" value={(*mitigation).clone()} oninput={on_mitigation}
                            placeholder="What is being done about it" />
                    </div>

                    <div class="form-group">
                        <label for="risk-verification">{ "Verification (test IDs)" }</label>
                        <input type="text" id="risk-verification" value={(*verification).clone()} oninput={on_verification}
                            placeholder="e.g. Thermal vacuum (D2)" />
                    </div>

                    <div class="form-group">
                        <label>{ "Action items addressing this risk" }</label>
                        <ItemPicker
                            items={(*all_items).clone()}
                            selected={(*selected_items).clone()}
                            on_change={on_items_change}
                            disabled={*submitting}
                        />
                    </div>

                    <div class="form-actions">
                        <button type="submit" class="btn btn-primary" disabled={*submitting}>
                            { if *submitting { "Creating..." } else { "Create Risk" } }
                        </button>
                        <button type="button" class="btn" onclick={on_cancel}>{ "Cancel" }</button>
                    </div>
                </form>
            </div>
        </div>
    }
}
