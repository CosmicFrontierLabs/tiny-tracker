//! The "Risks" section of the action item modal: which register entries this
//! item is addressing, with link and unlink controls.

use gloo_net::http::Request;
use shared::{LinkRisk, RiskResponse, RiskTrend};
use yew::prelude::*;
use yew_router::prelude::*;

use crate::pages::risk_style::{event_value, fmt_num, level_class};
use crate::Route;

#[derive(Properties, PartialEq)]
pub struct ItemRisksProps {
    pub item_id: String,
}

#[function_component(ItemRisks)]
pub fn item_risks(props: &ItemRisksProps) -> Html {
    let navigator = use_navigator().unwrap();
    let linked = use_state(Vec::<RiskResponse>::new);
    let all_risks = use_state(Vec::<RiskResponse>::new);
    let refresh_trigger = use_state(|| 0u32);
    let choice = use_state(String::new);
    let busy = use_state(|| false);
    let error = use_state(|| None::<String>);

    {
        let linked = linked.clone();
        let all_risks = all_risks.clone();
        let item_id = props.item_id.clone();
        let refresh = *refresh_trigger;
        use_effect_with((item_id, refresh), move |(iid, _)| {
            let iid = iid.clone();
            wasm_bindgen_futures::spawn_local(async move {
                if let Ok(resp) = Request::get(&format!("/api/items/{}/risks", iid))
                    .send()
                    .await
                {
                    if let Ok(data) = resp.json::<Vec<RiskResponse>>().await {
                        linked.set(data);
                    }
                }
                if let Ok(resp) = Request::get("/api/risks").send().await {
                    if let Ok(data) = resp.json::<Vec<RiskResponse>>().await {
                        all_risks.set(data);
                    }
                }
            });
            || ()
        });
    }

    let on_choice = {
        let choice = choice.clone();
        Callback::from(move |e: Event| choice.set(event_value(&e)))
    };

    let on_link = {
        let choice = choice.clone();
        let busy = busy.clone();
        let error = error.clone();
        let refresh_trigger = refresh_trigger.clone();
        let item_id = props.item_id.clone();
        Callback::from(move |_: MouseEvent| {
            let risk_id = (*choice).clone();
            if risk_id.is_empty() {
                return;
            }
            let choice = choice.clone();
            let busy = busy.clone();
            let error = error.clone();
            let refresh_trigger = refresh_trigger.clone();
            let item_id = item_id.clone();
            busy.set(true);
            error.set(None);
            wasm_bindgen_futures::spawn_local(async move {
                let body = serde_json::to_string(&LinkRisk { risk_id }).unwrap();
                match Request::post(&format!("/api/items/{}/risks", item_id))
                    .header("Content-Type", "application/json")
                    .body(body)
                    .unwrap()
                    .send()
                    .await
                {
                    Ok(resp) if resp.ok() => {
                        choice.set(String::new());
                        refresh_trigger.set(*refresh_trigger + 1);
                    }
                    Ok(resp) => error.set(Some(format!("Failed to link: {}", resp.status()))),
                    Err(e) => error.set(Some(format!("Request error: {}", e))),
                }
                busy.set(false);
            });
        })
    };

    let unlink = {
        let refresh_trigger = refresh_trigger.clone();
        let error = error.clone();
        let item_id = props.item_id.clone();
        move |risk_id: String| {
            let refresh_trigger = refresh_trigger.clone();
            let error = error.clone();
            let url = format!("/api/items/{}/risks/{}", item_id, risk_id);
            Callback::from(move |e: MouseEvent| {
                // The chip navigates to the risk; the × must not.
                e.stop_propagation();
                let refresh_trigger = refresh_trigger.clone();
                let error = error.clone();
                let url = url.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    match Request::delete(&url).send().await {
                        Ok(resp) if resp.ok() => refresh_trigger.set(*refresh_trigger + 1),
                        Ok(resp) => error.set(Some(format!("Failed to unlink: {}", resp.status()))),
                        Err(e) => error.set(Some(format!("Request error: {}", e))),
                    }
                });
            })
        }
    };

    let linked_ids: Vec<&str> = linked.iter().map(|r| r.id.as_str()).collect();
    // Closed risks stay linkable but sink to the bottom of the picker.
    let mut candidates: Vec<&RiskResponse> = all_risks
        .iter()
        .filter(|r| !linked_ids.contains(&r.id.as_str()))
        .collect();
    candidates.sort_by(|a, b| {
        (a.trend == RiskTrend::Closed)
            .cmp(&(b.trend == RiskTrend::Closed))
            .then_with(|| {
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| a.id.cmp(&b.id))
    });

    html! {
        <div class="item-risks-section">
            <h3>{ "Risks" }</h3>
            if let Some(err) = (*error).clone() {
                <p class="error">{ err }</p>
            }
            <div class="item-risks-content">
                if linked.is_empty() {
                    <span class="placeholder">{ "Not linked to any risk." }</span>
                } else {
                    <ul class="risk-chip-list">
                        { for linked.iter().map(|r| {
                            let id = r.id.clone();
                            let navigator = navigator.clone();
                            let on_click = Callback::from(move |_: MouseEvent| {
                                navigator.push(&Route::Risk { id: id.clone() });
                            });
                            html! {
                                <li class="risk-chip" onclick={on_click} title={format!("{} · score {} · {}", r.id, fmt_num(r.score), r.status.as_str())}>
                                    <span class={classes!("risk-badge", level_class(r.level))}>{ &r.id }</span>
                                    <span class="risk-chip-title">{ &r.title }</span>
                                    <button type="button" class="risk-chip-remove" title="Unlink" onclick={unlink(r.id.clone())}>{ "×" }</button>
                                </li>
                            }
                        })}
                    </ul>
                }
                <div class="risk-link-picker">
                    <select onchange={on_choice} disabled={*busy || candidates.is_empty()}>
                        <option value="" selected={choice.is_empty()}>
                            { if candidates.is_empty() { "No risks to link" } else { "Link a risk..." } }
                        </option>
                        { for candidates.iter().map(|r| html! {
                            <option value={r.id.clone()} selected={*choice == r.id}>
                                { format!("{} [{}] {}{}", r.id, fmt_num(r.score), r.title,
                                    if r.trend == RiskTrend::Closed { " (closed)" } else { "" }) }
                            </option>
                        })}
                    </select>
                    <button type="button" class="btn btn-small btn-primary" onclick={on_link} disabled={*busy || choice.is_empty()}>
                        { if *busy { "Linking..." } else { "Link" } }
                    </button>
                </div>
            </div>
        </div>
    }
}
