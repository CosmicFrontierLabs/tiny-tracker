/// CSS class for a priority label.
pub fn priority_class(priority: &str) -> &'static str {
    match priority {
        "High" => "priority-high",
        "Medium" => "priority-medium",
        "Low" => "priority-low",
        _ => "",
    }
}

/// CSS class for a status label.
pub fn status_class(status: &str) -> &'static str {
    match status {
        "New" => "status-new",
        "Not Started" => "status-not-started",
        "In Progress" => "status-in-progress",
        "TBC" => "status-tbc",
        "Complete" => "status-complete",
        "Blocked" => "status-blocked",
        _ => "",
    }
}
