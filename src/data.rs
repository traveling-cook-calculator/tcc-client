//! Data layer: the one and only handle to the `StorageManager`.
//!
//! # Why this exists
//! The old code kept the manager in a `Signal<StorageManager>` and
//!   * cloned it for reads (`storage.read().clone()` deep-copies *all* local
//!     projects, because `LocalStorage` owns a `Vec<CookAndRunData>`),
//!   * mutated clones (`storage.write().clone()` → the copy in the signal never
//!     sees the change, but `persist()` writes the copy's data to the browser
//!     storage → the next write from the original **overwrites it with stale data**),
//!   * held `write()` guards across `.await` (panics on concurrent access and
//!     re-renders every subscriber when the guard drops).
//!
//! # How it works now
//! * One `StorageManager` lives behind an async `RwLock` (readers run in
//!   parallel, writers are queued). It is never cloned.
//! * [`Storage`] is a small `Copy` handle: capture it in any closure/async block.
//! * Pages that show stored data subscribe to [`Storage::revision`] and call
//!   [`Storage::invalidate`] after *structural* changes (project created/deleted,
//!   cloud connected, …). Plain edits (autosaving a row) do not need to invalidate.
//!
//! ```ignore
//! let storage = use_storage();
//! let courses = use_resource(move || async move {
//!     let _ = storage.revision();                         // re-run on invalidate()
//!     storage.read().await.select_cook_and_run_course_list(id).await
//! });
//!
//! // write: keep the guard in its own scope and never `.await` other storage calls while holding it
//! let result = { storage.write().await.delete_course_of_cook_and_run(id, course_id).await };
//! storage.invalidate();
//! ```
//!
//! **Deadlock rule:** while you hold a guard, do not call `storage.read()/write()` again
//! in the same task. Drop the guard first (`{ … }` scope or `drop(guard)`).

use crate::keycloak::AuthState;
use crate::storage::StorageManager;
use async_std::sync::{Arc, RwLock};
use dioxus::prelude::*;
use std::ops::{Deref, DerefMut};

#[derive(Clone, Copy, PartialEq)]
pub struct Storage {
    inner: CopyValue<Arc<RwLock<StorageManager>>>,
    revision: Signal<u32>,
}

impl Storage {
    /// Must be called inside a component/hook (the handle is owned by the current scope).
    pub fn new(manager: StorageManager) -> Self {
        Self {
            inner: CopyValue::new(Arc::new(RwLock::new(manager))),
            revision: Signal::new(0),
        }
    }

    fn arc(&self) -> Arc<RwLock<StorageManager>> {
        self.inner.read().clone()
    }

    /// Shared access for `&self` methods (`select_*`, …). Many readers may hold it at once.
    pub async fn read(&self) -> impl Deref<Target = StorageManager> {
        let arc = self.arc();
        arc.read_arc().await
    }

    /// Exclusive access for `&mut self` methods (`create_*`, `update_*`, `delete_*`, …).
    pub async fn write(&self) -> impl DerefMut<Target = StorageManager> {
        let arc = self.arc();
        arc.write_arc().await
    }

    /// Reactive counter. Read it inside `use_resource`/`use_memo` to re-run when
    /// [`Storage::invalidate`] is called.
    pub fn revision(&self) -> u32 {
        *self.revision.read()
    }

    /// Tell all subscribers that stored data changed structurally.
    pub fn invalidate(&self) {
        let mut revision = self.revision;
        *revision.write() += 1;
    }

    /// Connects (or re-connects after a login/logout) to the cloud backend.
    /// Invalidates afterwards so pages re-fetch with the new connection.
    pub async fn connect_cloud(&self, auth: AuthState, base_url: String) -> Result<(), String> {
        let result = { self.write().await.load_cloud(auth, base_url).await };
        self.invalidate();
        result
    }

    /// Connects without a login (participant-only operations).
    pub async fn connect_cloud_anonymous(&self, base_url: String) -> Result<(), String> {
        let result = { self.write().await.load_cloud_anonymous(base_url).await };
        self.invalidate();
        result
    }

    /// Gives the open cloud connection a renewed token. Does not invalidate: nothing in the data changed.
    pub async fn update_session(&self, auth: AuthState) {
        self.write().await.update_cloud_session(auth);
    }

    pub async fn disconnect_cloud(&self) {
        self.write().await.disconnect_cloud();
        self.invalidate();
    }

    pub async fn is_cloud_connected(&self) -> bool {
        self.read().await.is_cloud_connected()
    }
}

/// Handle for components below `App`.
pub fn use_storage() -> Storage {
    use_context::<Storage>()
}
