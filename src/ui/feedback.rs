//! Loading / error views and the `render_resource` helper that replaces the
//! `match resource { None => LoadingPage, Some(Err) => ErrorPage, … }` block
//! copied into every page.

use super::button::{Button, ButtonVariant};
use super::layout::{Card, Tone};
use dioxus::prelude::*;

/// Centered spinner. Fills the available space of its flex parent instead of
/// using `h-screen` (which overflowed below the header).
#[component]
pub fn LoadingView(#[props(default, into)] label: Option<String>) -> Element {
    let label = label.unwrap_or_else(|| "Loading…".to_string());
    rsx! {
        div { class: "flex flex-1 items-center justify-center w-full min-h-[40vh]",
            div { class: "text-center",
                div { class: "inline-block",
                    div { class: "animate-spin rounded-full h-12 w-12 border-4 border-sand border-t-brand" }
                }
                p { class: "mt-4 text-ink-brown text-sm font-semibold tracking-wide", "{label}" }
            }
        }
    }
}

/// Error card with collapsible technical details and an optional retry button.
#[component]
pub fn ErrorView(
    #[props(into)] error_text: String,
    #[props(default, into)] error_details: String,
    #[props(default)] onretry: Option<EventHandler<()>>,
) -> Element {
    rsx! {
        div { class: "flex flex-1 items-center justify-center w-full min-h-[40vh] px-6",
            div { class: "w-full max-w-md",
                Card { title: "Something went wrong", tone: Tone::Danger,
                    p { class: "text-sm text-zinc-700", "{error_text}" }
                    if !error_details.is_empty() {
                        details { class: "group",
                            summary { class: "text-[11px] font-semibold tracking-[0.1em] uppercase \
                                              text-zinc-400 cursor-pointer \
                                              hover:text-zinc-600 transition-colors select-none",
                                "Details"
                            }
                            p { class: "mt-2 text-xs text-zinc-500 whitespace-pre-wrap \
                                        bg-zinc-50 rounded-xl border border-zinc-100 \
                                        px-3 py-2 leading-relaxed",
                                "{error_details}"
                            }
                        }
                    }
                    if let Some(onretry) = onretry {
                        Button {
                            label: "Try again",
                            variant: ButtonVariant::Secondary,
                            onclick: move |_| onretry.call(()),
                        }
                    }
                }
            }
        }
    }
}

/// Renders the three states of a `use_resource` that yields `Result<T, String>`.
///
/// ```ignore
/// let project = use_resource(move || async move { load(…).await });
/// render_resource(project, "Could not load project.", |p| rsx! { Content { p } })
/// ```
pub fn render_resource<T: Clone + 'static>(
    resource: Resource<Result<T, String>>,
    error_text: &str,
    on_ready: impl FnOnce(T) -> Element,
) -> Element {
    let state = resource.read_unchecked();
    match &*state {
        None => rsx! { LoadingView {} },
        Some(Err(details)) => {
            let mut resource = resource;
            rsx! {
                ErrorView {
                    error_text: error_text.to_string(),
                    error_details: details.clone(),
                    onretry: move |_| resource.restart(),
                }
            }
        }
        Some(Ok(value)) => on_ready(value.clone()),
    }
}
