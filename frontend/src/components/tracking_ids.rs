//! Rendering for the free-form "Vendor Tracking IDs" field.
//!
//! The field is stored as typed. Here it is split into chips on commas,
//! semicolons and newlines, and any chip that is a URL becomes a link. Keys
//! like `VND-4521` stay plain text: there is no per-vendor URL template to
//! expand them with, and guessing one would send people to the wrong place.

use yew::prelude::*;

/// Split a stored tracking-ID string into its individual references.
pub fn split_tracking_ids(raw: &str) -> Vec<&str> {
    raw.split([',', ';', '\n'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect()
}

fn is_url(s: &str) -> bool {
    s.starts_with("https://") || s.starts_with("http://")
}

/// Render the references as a row of chips. Returns an empty fragment for an
/// empty or whitespace-only string, so callers can decide on a placeholder.
pub fn tracking_id_chips(raw: &str) -> Html {
    let chips = split_tracking_ids(raw);
    html! {
        <span class="tracking-id-chips">
            { for chips.into_iter().map(|c| {
                if is_url(c) {
                    html! {
                        <a
                            class="tracking-id-chip tracking-id-link"
                            href={c.to_string()}
                            target="_blank"
                            rel="noopener noreferrer"
                            title={c.to_string()}
                            // A chip is inside a click-to-edit region in the
                            // detail modal; following the link must not also
                            // open the editor.
                            onclick={Callback::from(|e: MouseEvent| e.stop_propagation())}
                        >
                            { c }
                        </a>
                    }
                } else {
                    html! { <span class="tracking-id-chip">{ c }</span> }
                }
            })}
        </span>
    }
}
