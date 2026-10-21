//! SVG icons. Colors come from `currentColor`, so tone is set with Tailwind
//! text utilities (`text-brand`, `text-red-500`, …) instead of hex literals.

use dioxus::prelude::*;

#[component]
pub(crate) fn StartSVG() -> Element {
    rsx!(
        svg {
            class: "w-5 h-5 text-brand",
            xmlns: "http://www.w3.org/2000/svg",
            fill: "none",
            view_box: "0 0 24 24",
            stroke_width: "2",
            stroke: "currentColor",
            path {
                stroke_linecap: "round",
                stroke_linejoin: "round",
                d: "M12 2l3.09 6.26L22 9.27l-5 4.87L18.18 22 12 18.27 5.82 22 7 14.14l-5-4.87 6.91-1.01L12 2z",
            }
        }
    )
}

#[component]
pub(crate) fn EndSVG() -> Element {
    rsx!(
        svg {
            class: "w-5 h-5 text-brand",
            xmlns: "http://www.w3.org/2000/svg",
            fill: "none",
            view_box: "0 0 24 24",
            stroke_width: "2",
            stroke: "currentColor",
            path {
                stroke_linecap: "round",
                stroke_linejoin: "round",
                d: "M4 2v20m0-18h10l-2 4 2 4H4",
            }
            circle {
                cx: "4",
                cy: "20",
                r: "1",
                fill: "currentColor",
            }
        }
    )
}

#[component]
pub(crate) fn DownloadSVG() -> Element {
    rsx!(
        svg {
            class: "w-5 h-5",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            view_box: "0 0 24 24",
            xmlns: "http://www.w3.org/2000/svg",
            path { d: "M4 16v2a2 2 0 002 2h12a2 2 0 002-2v-2M7 10l5 5 5-5M12 15V3" }
        }
    )
}

#[component]
pub(crate) fn AddressSVG() -> Element {
    rsx!(
        svg {
            class: "w-4 h-4 shrink-0 text-brand",
            xmlns: "http://www.w3.org/2000/svg",
            fill: "currentColor",
            view_box: "0 0 24 24",
            path { d: "M12 2C8.134 2 5 5.134 5 9c0 4.418 7 13 7 13s7-8.582 7-13c0-3.866-3.134-7-7-7zm0 9.5a2.5 2.5 0 1 1 0-5 2.5 2.5 0 0 1 0 5z" }
        }
    )
}

#[component]
pub(crate) fn WarningSVG() -> Element {
    rsx!(
        svg {
            class: "w-5 h-5 text-brand",
            xmlns: "http://www.w3.org/2000/svg",
            fill: "none",
            view_box: "0 0 24 24",
            stroke: "currentColor",
            path {
                stroke_linecap: "round",
                stroke_linejoin: "round",
                stroke_width: "2",
                d: "M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0zM12 9v4m0 4h.01",
            }
        }
    )
}

#[component]
pub(crate) fn ErrorSVG() -> Element {
    rsx!(
        svg {
            class: "w-4 h-4 text-red-500 shrink-0",
            xmlns: "http://www.w3.org/2000/svg",
            fill: "none",
            view_box: "0 0 24 24",
            stroke_width: "2",
            stroke: "currentColor",
            circle { cx: "12", cy: "12", r: "10", stroke: "currentColor", stroke_width: "2" }
            line { x1: "12", y1: "8", x2: "12", y2: "12", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round" }
            line { x1: "12", y1: "16", x2: "12", y2: "16", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round" }
        }
    )
}

#[component]
pub(crate) fn InfoSVG() -> Element {
    rsx!(
        svg {
            class: "w-5 h-5 text-amber-500",
            xmlns: "http://www.w3.org/2000/svg",
            fill: "none",
            view_box: "0 0 24 24",
            stroke_width: "2",
            stroke: "currentColor",
            path {
                stroke_linecap: "round",
                stroke_linejoin: "round",
                d: "M13 16h-1v-4h-1m1-4h.01M12 2a10 10 0 100 20 10 10 0 000-20z",
            }
        }
    )
}

#[component]
pub(crate) fn CourseSVG() -> Element {
    rsx!(
        svg {
            class: "w-5 h-5 text-brand",
            xmlns: "http://www.w3.org/2000/svg",
            fill: "none",
            view_box: "0 0 24 24",
            stroke: "currentColor",
            path {
                stroke_linecap: "round",
                stroke_linejoin: "round",
                stroke_width: "2",
                d: "M4 6h1v12H4zM7 6h1v12H7zM21 12a9 9 0 11-18 0 9 9 0 0118 0zm-6 0a3 3 0 11-6 0 3 3 0 016 0z",
            }
        }
    )
}

#[component]
pub(crate) fn TimeSVG() -> Element {
    rsx!(
        svg {
            class: "w-5 h-5 text-brand",
            xmlns: "http://www.w3.org/2000/svg",
            fill: "none",
            stroke: "currentColor",
            path {
                stroke_linecap: "round",
                stroke_linejoin: "round",
                stroke_width: "2",
                d: "M12 8v4l3 3M12 2a10 10 0 100 20 10 10 0 000-20z",
            }
        }
    )
}

#[component]
pub(crate) fn PhoneSVG() -> Element {
    rsx!(
        svg {
            xmlns: "http://www.w3.org/2000/svg",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            class: "w-5 h-5 text-brand",
            path { d: "M22 16.92v3a2 2 0 0 1-2.18 2A19.86 19.86 0 0 1 3.1 5.18 2 2 0 0 1 5 3h3a2 2 0 0 1 2 1.72c.12.81.31 1.6.57 2.35a2 2 0 0 1-.45 2.11L9.03 10.91a16 16 0 0 0 6.06 6.06l1.73-1.09a2 2 0 0 1 2.11-.45c.75.26 1.54.45 2.35.57a2 2 0 0 1 1.72 2z" }
        }
    )
}

#[component]
pub(crate) fn GroupSVG() -> Element {
    rsx!(
        svg {
            class: "w-5 h-5 text-brand",
            xmlns: "http://www.w3.org/2000/svg",
            fill: "none",
            stroke: "currentColor",
            path {
                stroke_linecap: "round",
                stroke_linejoin: "round",
                stroke_width: "2",
                d: "M17 20h5v-2a4 4 0 00-5-4m-4-2a4 4 0 100-8 4 4 0 000 8zm6 6v2m-6-2v2m-6-2v2H2v-2a4 4 0 015-4m0 0a4 4 0 014 4",
            }
        }
    )
}

#[component]
pub(crate) fn PersonSVG() -> Element {
    rsx!(
        svg {
            class: "w-5 h-5 text-brand",
            xmlns: "http://www.w3.org/2000/svg",
            fill: "none",
            stroke: "currentColor",
            path {
                stroke_linecap: "round",
                stroke_linejoin: "round",
                stroke_width: "2",
                d: "M12 4.5a3.5 3.5 0 110 7 3.5 3.5 0 010-7zM12 14.5c-4.418 0-8 1.79-8 4v1h16v-1c0-2.21-3.582-4-8-4z",
            }
        }
    )
}

#[component]
pub(crate) fn CheckSVG() -> Element {
    rsx!(
        svg {
            class: "w-4 h-4 shrink-0 text-green-600",
            xmlns: "http://www.w3.org/2000/svg",
            fill: "none",
            view_box: "0 0 24 24",
            stroke_width: "2.5",
            stroke: "currentColor",
            path {
                stroke_linecap: "round",
                stroke_linejoin: "round",
                d: "M5 13l4 4L19 7",
            }
        }
    )
}
