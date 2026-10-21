//! Text building blocks. Heading colors use the `ink-brown` design token.

use dioxus::prelude::*;

#[component]
pub fn Headline1(#[props(into)] headline: String) -> Element {
    rsx! {
        div { class: "text-zinc-900 font-sans leading-tight",
            h1 { class: "text-3xl font-bold mb-2", "{headline}" }
        }
    }
}

#[component]
pub fn Headline2(#[props(into)] headline: String) -> Element {
    rsx! {
        div { class: "font-sans leading-tight",
            h2 { class: "text-xl font-semibold text-ink-brown mb-1", "{headline}" }
        }
    }
}

#[component]
pub fn Headline3(#[props(into)] headline: String) -> Element {
    rsx! {
        label { class: "block text-sm font-semibold text-ink-brown ml-2", "{headline}" }
    }
}

#[component]
pub fn Text(#[props(into)] text: String) -> Element {
    rsx! {
        div { class: "text-zinc-700 font-sans leading-relaxed",
            p { class: "text-sm mb-3", "{text}" }
        }
    }
}
