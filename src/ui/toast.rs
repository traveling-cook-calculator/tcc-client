//! Toast notifications.
//!
//! * `use_toasts_provider()` once in `App`, `<ToastHost {}>` once in the layout.
//! * Anywhere else: `let toasts = use_toasts(); toasts.error("Title", "Details");`
//!   (auto-dismiss is handled by the toast card itself)
//!
//! The context type stays `Signal<Vec<ToastMessage>>`, so existing
//! `use_context::<Signal<Vec<ToastMessage>>>()` call sites keep working.

use super::button::CloseButton;
use super::icons::{CheckSVG, ErrorSVG, InfoSVG};
use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;
use uuid::Uuid;

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum ToastKind {
    #[default]
    Error,
    Success,
    Info,
}

impl ToastKind {
    fn ttl_ms(self) -> u32 {
        match self {
            Self::Error => 15_000,
            Self::Success => 4_000,
            Self::Info => 8_000,
        }
    }
    fn border(self) -> &'static str {
        match self {
            Self::Error => "border-l-red-500",
            Self::Success => "border-l-green-500",
            Self::Info => "border-l-amber-400",
        }
    }
}

#[derive(Clone, PartialEq)]
pub struct ToastMessage {
    pub id: Uuid,
    pub kind: ToastKind,
    pub headline: String,
    pub message: String,
}

/// Copyable handle to the global toast list.
#[derive(Clone, Copy)]
pub struct Toasts {
    list: Signal<Vec<ToastMessage>>,
}

/// Call once in the root component.
pub fn use_toasts_provider() -> Toasts {
    let list = use_signal(Vec::<ToastMessage>::new);
    use_context_provider(|| list);
    Toasts { list }
}

pub fn use_toasts() -> Toasts {
    Toasts { list: use_context::<Signal<Vec<ToastMessage>>>() }
}

impl Toasts {
    pub fn error(&self, headline: impl Into<String>, message: impl Into<String>) {
        self.push(ToastKind::Error, headline.into(), message.into());
    }
    pub fn success(&self, headline: impl Into<String>, message: impl Into<String>) {
        self.push(ToastKind::Success, headline.into(), message.into());
    }
    pub fn info(&self, headline: impl Into<String>, message: impl Into<String>) {
        self.push(ToastKind::Info, headline.into(), message.into());
    }

    pub fn dismiss(&self, id: Uuid) {
        let mut list = self.list;
        list.write().retain(|t| t.id != id);
    }

    fn push(&self, kind: ToastKind, headline: String, message: String) {
        let mut list = self.list;
        list.write().push(ToastMessage { id: Uuid::new_v4(), kind, headline, message });
    }
}

/// Backwards-compatible helper for existing call sites.
pub fn trigger_error_toast(list: Signal<Vec<ToastMessage>>, headline: &str, message: &str) {
    Toasts { list }.error(headline, message);
}

/// Renders the stack of toasts (bottom-right). Place once in the layout.
#[component]
pub fn ToastHost() -> Element {
    let toasts = use_toasts();
    let list = toasts.list;

    rsx! {
        div { class: "fixed bottom-4 right-4 z-[100] flex flex-col gap-3 w-[calc(100%-2rem)] max-w-sm pointer-events-none",
            for toast in list.read().iter().cloned() {
                ToastCard { key: "{toast.id}", toast }
            }
        }
    }
}

#[component]
fn ToastCard(toast: ToastMessage) -> Element {
    let toasts = use_toasts();
    let id = toast.id;
    let ttl = toast.kind.ttl_ms();

    // The timer belongs to the card: it is a descendant of the scope that owns the list, and it
    // ends with the card (dismissed by hand → no stray task). It used to be a `spawn_forever`
    // in `push`, which runs in the *root* scope and touched a signal owned by a child scope –
    // Dioxus warns about that on every toast ("A Copy Value created in … was used in …").
    use_future(move || async move {
        TimeoutFuture::new(ttl).await;
        toasts.dismiss(id);
    });

    let role = if toast.kind == ToastKind::Error { "alert" } else { "status" };

    rsx! {
        div {
            role,
            class: "pointer-events-auto relative flex items-start gap-3 bg-white rounded-2xl border \
                    border-amber-100 border-l-4 {toast.kind.border()} shadow-lg \
                    pl-4 pr-10 py-3 animate-fade-in-up",
            div { class: "mt-0.5",
                match toast.kind {
                    ToastKind::Error => rsx! { ErrorSVG {} },
                    ToastKind::Success => rsx! { CheckSVG {} },
                    ToastKind::Info => rsx! { InfoSVG {} },
                }
            }
            div { class: "min-w-0",
                p { class: "text-sm font-semibold text-zinc-900", "{toast.headline}" }
                if !toast.message.is_empty() {
                    p { class: "mt-0.5 text-xs text-zinc-600 break-words", "{toast.message}" }
                }
            }
            CloseButton { onclick: move |_| toasts.dismiss(id) }
        }
    }
}
