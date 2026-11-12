use std::cmp::Ordering;

use gloo_net::http::Request;
use shared::{RiskResponse, RiskStatus, RiskTrend, Vendor};
use yew::prelude::*;
use yew_router::prelude::*;

use crate::components::Header;
use crate::pages::risk_detail::RiskDetailModal;
use crate::pages::risk_form::NewRiskModal;
use crate::pages::risk_style::{event_value, fmt_num, level_class, status_class, trend_class};
use crate::Route;

#[derive(Clone, Copy, PartialEq)]
enum SortColumn {
    Id,
    Component,
    Category,
    Score,
    Status,
    Trend,
    Items,
    Updated,
}

#[derive(Clone, Copy, PartialEq)]
enum SortDirection {
    Asc,
    Desc,
}

fn reload_page() {
    if let Some(window) = web_sys::window() {
        let _ = window.location().reload();
    }
}

#[derive(Properties, PartialEq)]
pub struct RiskRegisterProps {
    #[prop_or_default]
    pub initial_risk_id: Option<String>,
}

#[function_component(RiskRegister)]
pub fn risk_register(props: &RiskRegisterProps) -> Html {
    let navigator = use_navigator().unwrap();
    let risks = use_state(Vec::<RiskResponse>::new);
    let vendors = use_state(Vec::<Vendor>::new);
    let loading = use_state(|| true);
    let error = use_state(|| None::<String>);
    let is_admin = use_state(|| false);
    let refresh_trigger = use_state(|| 0u32);
    let show_new_modal = use_state(|| false);
    let selected_risk_id = use_state(|| props.initial_risk_id.clone());

    let filter_vendor_id = use_state(|| None::<i32>);
    let filter_status = use_state(|| None::<RiskStatus>);
    let show_closed = use_state(|| false);
    // Highest score first is what a register is for.
    let sort_column = use_state(|| SortColumn::Score);
    let sort_direction = use_state(|| SortDirection::Desc);

    {
        let selected_risk_id = selected_risk_id.clone();
        let initial = props.initial_risk_id.clone();
        use_effect_with(initial.clone(), move |_| {
            selected_risk_id.set(initial);
            || ()
        });
    }

    {
        let is_admin = is_admin.clone();
        use_effect_with((), move |_| {
            wasm_bindgen_futures::spawn_local(async move {
                if let Ok(resp) = Request::get("/auth/me").send().await {
                    if let Ok(me) = resp.json::<shared::CurrentUserResponse>().await {
                        is_admin.set(me.is_admin);
                    }
                }
            });
            || ()
        });
    }

    {
        let risks = risks.clone();
        let vendors = vendors.clone();
        let loading = loading.clone();
        let error = error.clone();
        let refresh = *refresh_trigger;

        use_effect_with(refresh, move |_| {
            wasm_bindgen_futures::spawn_local(async move {
                match Request::get("/api/risks").send().await {
                    Ok(resp) => {
                        if resp.status() == 401 {
                            reload_page();
                            return;
                        }
                        if resp.ok() {
                            match resp.json::<Vec<RiskResponse>>().await {
                                Ok(data) => risks.set(data),
                                Err(e) => {
                                    error.set(Some(format!("Failed to parse response: {}", e)))
                                }
                            }
                        } else {
                            error.set(Some(format!("Request failed: {}", resp.status())));
                        }
                    }
                    Err(e) => error.set(Some(format!("Request error: {}", e))),
                }

                if let Ok(resp) = Request::get("/api/vendors").send().await {
                    if let Ok(data) = resp.json::<Vec<Vendor>>().await {
                        vendors.set(data);
                    }
                }

                loading.set(false);
            });
            || ()
        });
    }

    let on_new_click = {
        let show_new_modal = show_new_modal.clone();
        Callback::from(move |_| show_new_modal.set(true))
    };

    let on_new_close = {
        let show_new_modal = show_new_modal.clone();
        Callback::from(move |_| show_new_modal.set(false))
    };

    let on_created = {
        let show_new_modal = show_new_modal.clone();
        let refresh_trigger = refresh_trigger.clone();
        Callback::from(move |_| {
            show_new_modal.set(false);
            refresh_trigger.set(*refresh_trigger + 1);
        })
    };

    let on_detail_close = {
        let selected_risk_id = selected_risk_id.clone();
        let refresh_trigger = refresh_trigger.clone();
        let navigator = navigator.clone();
        Callback::from(move |_| {
            selected_risk_id.set(None);
            refresh_trigger.set(*refresh_trigger + 1);
            navigator.push(&Route::Risks);
        })
    };

    let on_vendor_filter_change = {
        let filter_vendor_id = filter_vendor_id.clone();
        Callback::from(move |e: Event| {
            let value = event_value(&e);
            filter_vendor_id.set(if value.is_empty() {
                None
            } else {
                value.parse().ok()
            });
        })
    };

    let on_status_filter_change = {
        let filter_status = filter_status.clone();
        Callback::from(move |e: Event| {
            filter_status.set(RiskStatus::from_label(&event_value(&e)));
        })
    };

    let on_sort = {
        let sort_column = sort_column.clone();
        let sort_direction = sort_direction.clone();
        Callback::from(move |col: SortColumn| {
            if *sort_column == col {
                sort_direction.set(match *sort_direction {
                    SortDirection::Asc => SortDirection::Desc,
                    SortDirection::Desc => SortDirection::Asc,
                });
            } else {
                sort_column.set(col);
                // Numbers read best biggest-first; text reads best A-Z.
                sort_direction.set(match col {
                    SortColumn::Score | SortColumn::Items | SortColumn::Updated => {
                        SortDirection::Desc
                    }
                    _ => SortDirection::Asc,
                });
            }
        })
    };

    let sort_indicator = |col: SortColumn| -> &'static str {
        if *sort_column == col {
            match *sort_direction {
                SortDirection::Asc => " ↑",
                SortDirection::Desc => " ↓",
            }
        } else {
            ""
        }
    };

    let mut filtered: Vec<&RiskResponse> = risks
        .iter()
        .filter(|r| {
            let vendor_match = filter_vendor_id.map(|v| r.vendor_id == v).unwrap_or(true);
            let status_match = filter_status.map(|s| r.status == s).unwrap_or(true);
            let closed_match = *show_closed || r.trend != RiskTrend::Closed;
            vendor_match && status_match && closed_match
        })
        .collect();

    let col = *sort_column;
    let dir = *sort_direction;
    filtered.sort_by(|a, b| {
        let ord = match col {
            SortColumn::Id => a.id.cmp(&b.id),
            SortColumn::Component => a.component.cmp(&b.component),
            SortColumn::Category => a.category.cmp(&b.category),
            SortColumn::Score => a.score.partial_cmp(&b.score).unwrap_or(Ordering::Equal),
            SortColumn::Status => a.status.as_str().cmp(b.status.as_str()),
            SortColumn::Trend => a.trend.as_str().cmp(b.trend.as_str()),
            SortColumn::Items => a.linked_item_count.cmp(&b.linked_item_count),
            SortColumn::Updated => a.updated_at.cmp(&b.updated_at),
        }
        .then_with(|| a.id.cmp(&b.id));
        match dir {
            SortDirection::Asc => ord,
            SortDirection::Desc => ord.reverse(),
        }
    });

    let header_cell = |label: &'static str, col: SortColumn, extra: &'static str| -> Html {
        let on_sort = on_sort.clone();
        html! {
            <th class={classes!("sortable-header", extra)} onclick={Callback::from(move |_| on_sort.emit(col))}>
                { label }{ sort_indicator(col) }
            </th>
        }
    };

    let total_live = risks
        .iter()
        .filter(|r| r.trend != RiskTrend::Closed)
        .count();

    html! {
        <>
            <Header />
            <main>
                <div class="page-header">
                    <h2>{ "Risk Register" }</h2>
                    <div class="header-actions">
                        <button type="button" class="btn btn-primary" onclick={on_new_click} disabled={vendors.is_empty()}>
                            { "+ New Risk" }
                        </button>
                    </div>
                </div>

                <div class="filters">
                    <div class="filter-group">
                        <label>{ "Vendor:" }</label>
                        <select onchange={on_vendor_filter_change}>
                            <option value="" selected={filter_vendor_id.is_none()}>{ "All Vendors" }</option>
                            { for vendors.iter().map(|v| {
                                let selected = *filter_vendor_id == Some(v.id);
                                html! { <option value={v.id.to_string()} selected={selected}>{ &v.name }</option> }
                            })}
                        </select>
                    </div>
                    <div class="filter-group">
                        <label>{ "Status:" }</label>
                        <select onchange={on_status_filter_change}>
                            <option value="" selected={filter_status.is_none()}>{ "All Statuses" }</option>
                            { for RiskStatus::all().iter().map(|s| {
                                let selected = *filter_status == Some(*s);
                                html! { <option value={s.as_str()} selected={selected}>{ s.as_str() }</option> }
                            })}
                        </select>
                    </div>
                    <div class="filter-group">
                        <label class="checkbox-label">
                            <input
                                type="checkbox"
                                checked={*show_closed}
                                onchange={{
                                    let show_closed = show_closed.clone();
                                    Callback::from(move |_: Event| show_closed.set(!*show_closed))
                                }}
                            />
                            { " Show Closed" }
                        </label>
                    </div>
                    <div class="filter-group risk-count">
                        { format!("{} shown of {} open", filtered.len(), total_live) }
                    </div>
                </div>

                if *show_new_modal {
                    <NewRiskModal
                        vendors={(*vendors).clone()}
                        existing={(*risks).clone()}
                        on_close={on_new_close}
                        on_created={on_created}
                    />
                }

                if let Some(risk_id) = (*selected_risk_id).clone() {
                    <RiskDetailModal
                        risk_id={risk_id}
                        is_admin={*is_admin}
                        on_close={on_detail_close}
                    />
                }

                if *loading {
                    <p>{ "Loading..." }</p>
                } else if let Some(err) = (*error).clone() {
                    <p class="error">{ err }</p>
                } else if vendors.is_empty() {
                    <p>{ "No vendors configured. Add one under Action Items → Manage Vendors first." }</p>
                } else if risks.is_empty() {
                    <p>{ "No risks yet. Click '+ New Risk' to open one." }</p>
                } else if filtered.is_empty() {
                    <p>{ "No risks match the current filters." }</p>
                } else {
                    <div class="risk-table-wrap">
                        <table class="table risk-table">
                            <thead>
                                <tr>
                                    { header_cell("ID", SortColumn::Id, "") }
                                    { header_cell("Item", SortColumn::Component, "") }
                                    { header_cell("Category", SortColumn::Category, "") }
                                    <th>{ "Risk" }</th>
                                    <th class="num" title="Severity, 0-5">{ "Sev" }</th>
                                    <th class="num" title="Likelihood, 0-5">{ "Lik" }</th>
                                    { header_cell("Score", SortColumn::Score, "num") }
                                    <th>{ "Level" }</th>
                                    { header_cell("Status", SortColumn::Status, "") }
                                    { header_cell("Trend", SortColumn::Trend, "") }
                                    { header_cell("Items", SortColumn::Items, "num") }
                                    { header_cell("Updated", SortColumn::Updated, "") }
                                </tr>
                            </thead>
                            <tbody>
                                { for filtered.iter().map(|r| {
                                    let risk_id = r.id.clone();
                                    let navigator = navigator.clone();
                                    let on_row_click = Callback::from(move |_| {
                                        navigator.push(&Route::Risk { id: risk_id.clone() });
                                    });
                                    html! {
                                        <tr class={classes!("clickable-row", (r.trend == RiskTrend::Closed).then_some("risk-closed"))} onclick={on_row_click}>
                                            <td><span class="item-id">{ &r.id }</span></td>
                                            <td>{ r.component.as_deref().unwrap_or("-") }</td>
                                            <td>{ &r.category }</td>
                                            <td class="risk-title" title={r.title.clone()}>{ &r.title }</td>
                                            <td class="num">{ fmt_num(r.severity) }</td>
                                            <td class="num">{ fmt_num(r.likelihood) }</td>
                                            <td class={classes!("num", "risk-score", level_class(r.level))}>{ fmt_num(r.score) }</td>
                                            <td><span class={classes!("risk-badge", level_class(r.level))}>{ r.level.as_str() }</span></td>
                                            <td class={status_class(r.status)}>{ r.status.as_str() }</td>
                                            <td class={trend_class(r.trend)}>{ r.trend.as_str() }</td>
                                            <td class="num">{ r.linked_item_count }</td>
                                            <td>{ r.updated_at.date_naive().to_string() }</td>
                                        </tr>
                                    }
                                })}
                            </tbody>
                        </table>
                    </div>
                }
            </main>
        </>
    }
}
