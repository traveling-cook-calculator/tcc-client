//! Central UI kit. Pages import from here (`use crate::ui::*`) instead of
//! re-implementing cards, buttons, inputs, dialogs or loading/error handling.
//!
//! | module       | contents                                                        |
//! |--------------|-----------------------------------------------------------------|
//! | `button`     | `Button`, `DeleteButton`, `CloseButton`, `Spinner`              |
//! | `busy`       | `use_busy` – spinner state for async actions                    |
//! | `form`       | `Field`, `TextInput`, `TextArea`, `FieldError`, `SavingIcon`    |
//! | `layout`     | `PageShell`, `PageHeader`, `Card`, `AddCardButton`, `Dialog`    |
//! | `feedback`   | `LoadingView`, `ErrorView`, `render_resource`                   |
//! | `flash`      | `use_flash` – "saved" highlight for `Card { flash }`             |
//! | `toast`      | `use_toasts`, `ToastHost`                                       |
//! | `widgets`    | `Pill`, `SegmentedTabs`, `Tabs`, `SearchField`, `EmptyState`    |
//! | `typography` | `Headline1..3`, `Text`                                          |
//! | `icons`      | SVG icons (colored via `currentColor`)                          |

// Not every component is used yet; removed in step 7 once all pages are migrated.
#![allow(dead_code)]

pub mod busy;
pub mod button;
pub mod feedback;
pub mod flash;
pub mod form;
pub mod icons;
pub mod layout;
pub mod toast;
pub mod typography;
pub mod widgets;

#[allow(unused_imports)]
pub use {
    busy::{use_busy, Busy},
    button::{button_class, Button, ButtonVariant, CloseButton, DeleteButton, Spinner},
    feedback::{render_resource, ErrorView, LoadingView},
    flash::{use_flash, Flash},
    form::{Checkbox, Field, FieldError, InputKind, SavingIcon, Select, TextArea, TextInput},
    icons::*,
    layout::{AddCardButton, Card, Centered, Dialog, PageHeader, PageShell, Tone},
    toast::{use_toasts, use_toasts_provider, ToastHost, ToastKind, ToastMessage, Toasts},
    typography::{Headline1, Headline2, Headline3, Text},
    widgets::{CopyField, EmptyState, Pill, PillTone, SearchField, SegmentedTabs, Skeleton, Tabs, Toolbar},
};
