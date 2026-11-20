//! Central UI kit. Pages import from here (`use crate::ui::*`) instead of
//! re-implementing cards, buttons, inputs, dialogs or loading/error handling.
//!
//! | module       | contents                                                        |
//! |--------------|-----------------------------------------------------------------|
//! | `button`     | `Button`, `DeleteButton`, `CloseButton`, `Spinner`, `FloatingButton`, `ToggleButton`, `ToolButton`, `TextButton`, `ChipButton` |
//! | `controls`   | `Slider`, `ColorInput`, `ColorField` (design tools)             |
//! | `busy`       | `use_busy` – spinner state for async actions                    |
//! | `form`       | `Field`, `TextInput`, `TextArea`, `Select`, `Checkbox`, `CheckboxCell`, `Eyebrow`, `Hint`, `FieldError`, `SavingIcon` |
//! | `layout`     | `PageShell`, `PageHeader`, `Card`, `AddCardButton`, `Dialog`    |
//! | `feedback`   | `LoadingView`, `ErrorView`, `Notice`, `render_resource`         |
//! | `flash`      | `use_flash` – "saved" highlight for `Card { flash }`             |
//! | `toast`      | `use_toasts`, `ToastHost`                                       |
//! | `widgets`    | `Pill`, `ActionMenu`, `SegmentedTabs`, `Tabs`, `SearchField`, `EmptyState`, `InfoTip`, `Disclosure` |
//! | `typography` | `Headline1..3`, `Text`                                          |
//! | `icons`      | SVG icons (colored via `currentColor`)                          |

// Not every component is used yet; removed in step 7 once all pages are migrated.
#![allow(dead_code)]

pub mod busy;
pub mod button;
pub mod controls;
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
    button::{
        button_class, Button, ButtonVariant, CardButton, ChipButton, CloseButton, DeleteButton, FloatingButton, HeaderButton,
        Spinner, TagButton, TextButton, TileButton, ToggleButton, ToolButton,
    },
    controls::{hex6, ColorField, ColorInput, Slider},
    feedback::{render_resource, ErrorView, LoadingView, Notice, NoticeTone},
    flash::{use_flash, Flash},
    form::{
        Checkbox, CheckboxCell, ChoiceSelect, Eyebrow, Field, FieldError, FileInput, Hint, InputKind, PickOption, PickSelect, SavingIcon, Select, Switch,
        TextArea, TextInput, EYEBROW,
    },
    icons::*,
    layout::{AddCardButton, Card, Centered, Dialog, PageHeader, PageShell, Tone, Panel},
    toast::{use_toasts, use_toasts_provider, ToastHost, ToastKind, ToastMessage, Toasts},
    typography::{Headline1, Headline2, Headline3, Text},
    widgets::{
        menu_item_class, ActionMenu, CopyField, Disclosure, DisclosureList, EmptyState, InfoTip, MenuDivider, MenuItem, Pill, PillTone, SearchField,
        SegmentedTabs, Skeleton, Tabs, Toolbar,
    },
};
