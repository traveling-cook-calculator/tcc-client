//! Targets of the links in the organizer's notification mails:
//! `/admin/project/{project}` and `/admin/project/{project}/team/{team}`. They only forward to the
//! right page (the login, if needed, is handled there).

use dioxus::prelude::*;
use uuid::Uuid;

use crate::ui::LoadingView;
use crate::Route;

#[component]
pub fn AdminProjectLink(cook_and_run_id: ReadSignal<Uuid>) -> Element {
    let navigator = use_navigator();
    use_effect(move || {
        navigator.replace(Route::Overview { cook_and_run_id: cook_and_run_id() });
    });
    rsx! { LoadingView {} }
}

#[component]
pub fn AdminTeamLink(cook_and_run_id: ReadSignal<Uuid>, team_id: ReadSignal<Uuid>) -> Element {
    let navigator = use_navigator();
    use_effect(move || {
        navigator.replace(Route::Teams { cook_and_run_id: cook_and_run_id(), team: team_id().to_string() });
    });
    rsx! { LoadingView {} }
}
