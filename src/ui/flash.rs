//! `use_flash` – short-lived "saved" highlight for a `Card { flash: … }`.
//! Replaces the `save_success_signal` + `sleep` + inline `SAVE_GLOW_CSS`
//! block that was copy-pasted into every page.

use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;

const FLASH_MS: u32 = 2_000;

#[derive(Clone, Copy, PartialEq)]
pub struct Flash {
    active: Signal<bool>,
    generation: Signal<u32>,
}

pub fn use_flash() -> Flash {
    Flash { active: use_signal(|| false), generation: use_signal(|| 0) }
}

impl Flash {
    pub fn is_active(&self) -> bool {
        *self.active.read()
    }

    /// Starts (or restarts) the highlight. Safe to call repeatedly: only the
    /// latest call may switch it off again.
    pub fn trigger(&self) {
        let (mut active, mut generation) = (self.active, self.generation);
        let current = *generation.peek() + 1;
        generation.set(current);
        active.set(true);
        spawn(async move {
            TimeoutFuture::new(FLASH_MS).await;
            if *generation.peek() == current {
                active.set(false);
            }
        });
    }
}
