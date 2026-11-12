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
    let s = format!("{:.2}", v);
    s.trim_end_matches('0').trim_end_matches('.').to_string()
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
