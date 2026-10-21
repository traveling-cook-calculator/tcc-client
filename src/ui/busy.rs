//! `use_busy` – the one place that knows how to run an async action while a
//! button shows a spinner.
//!
//! ```ignore
//! let save = use_busy();
//! Button {
//!     label: "Save",
//!     loading: save.is_busy(),
//!     onclick: move |_| save.run(async move { /* … */ }),
//! }
//! ```
//!
//! It replaces `Arc<dyn Fn>` closure props: the parent owns the task, the
//! button only receives plain, comparable values (`loading: bool`).

use dioxus::prelude::*;
use std::future::Future;

#[derive(Clone, Copy, PartialEq)]
pub struct Busy {
    state: Signal<bool>,
}

pub fn use_busy() -> Busy {
    Busy { state: use_signal(|| false) }
}

impl Busy {
    /// Reactive: components reading this re-render when the task starts/ends.
    pub fn is_busy(&self) -> bool {
        *self.state.read()
    }

    /// Runs `task` unless one is already running (double-click safe).
    /// Call from event handlers; the busy flag is set synchronously.
    pub fn run<F>(&self, task: F)
    where
        F: Future<Output = ()> + 'static,
    {
        let mut state = self.state;
        if *state.peek() {
            return;
        }
        state.set(true);
        spawn(async move {
            task.await;
            state.set(false);
        });
    }
}
