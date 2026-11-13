//! Presentation helpers shared by the risk register, the risk modals and the
//! item-side risk links.

use shared::{RiskLevel, RiskStatus, RiskTrend};
use web_sys::Event;

pub fn level_class(level: RiskLevel) -> &'static str {
    match level {
        RiskLevel::High => "risk-level-high",
        RiskLevel::Medium => "risk-level-medium",
        RiskLevel::Low => "risk-level-low",
    }
}

pub fn status_class(status: RiskStatus) -> &'static str {
    match status {
        RiskStatus::Open => "risk-status-open",
        RiskStatus::InWork => "risk-status-in-work",
        RiskStatus::Mitigated => "risk-status-mitigated",
        RiskStatus::Accepted => "risk-status-accepted",
    }
}

pub fn trend_class(trend: RiskTrend) -> &'static str {
    match trend {
        RiskTrend::New => "risk-trend-new",
        RiskTrend::Improving => "risk-trend-improving",
        RiskTrend::Stable => "risk-trend-stable",
        RiskTrend::Worsening => "risk-trend-worsening",
        RiskTrend::Closed => "risk-trend-closed",
    }
}

/// Format a score or scale value without trailing zeros: `16`, `7.5`, `3.75`.
pub fn fmt_num(v: f64) -> String {
    shared::format_scale(v)
}

/// Locale-aware date and time in the viewer's timezone.
pub fn format_datetime(dt: &chrono::DateTime<chrono::Utc>) -> String {
    let js_date = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(
        dt.timestamp_millis() as f64
    ));
    let options = js_sys::Object::new();
    for (k, v) in [
        ("year", "numeric"),
        ("month", "short"),
        ("day", "numeric"),
        ("hour", "2-digit"),
        ("minute", "2-digit"),
    ] {
        let _ = js_sys::Reflect::set(&options, &k.into(), &v.into());
    }
    js_date
        .to_locale_string("default", &options)
        .as_string()
        .unwrap_or_else(|| dt.format("%b %d, %Y %H:%M").to_string())
}

/// Read `.value` off whatever element raised the event: input, textarea or
/// select. Saves a cast per field on forms with a dozen fields.
pub fn event_value(e: &Event) -> String {
    e.target()
        .and_then(|t| js_sys::Reflect::get(&t, &"value".into()).ok())
        .and_then(|v| v.as_string())
        .unwrap_or_default()
}

/// Parse a 0-5 scale value typed into a number input.
pub fn parse_scale(s: &str) -> Option<f64> {
    let v: f64 = s.trim().parse().ok()?;
    (v.is_finite() && (0.0..=shared::RISK_SCALE_MAX).contains(&v)).then_some(v)
}

/// Fold an optional text field to `None` when blank.
pub fn blank_to_none(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}
