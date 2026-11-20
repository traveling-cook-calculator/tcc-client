//! Pages for people who are **not** project admins and need no login:
//!
//! | file / folder | contents                                                          |
//! |---------------|-------------------------------------------------------------------|
//! | `register.rs` | `ShareRegisterPage` – team registration via the share link        |
//! | `team/`       | `TeamPage` – a team views/edits/cancels itself via its token link |
//! | `form.rs`     | form model + validation shared by both (pure, tested)             |
//! | `lang.rs`     | German/English texts                                              |
//! | `views.rs`    | inactive / closed / success states                                |
//!
//! Participants use their **own** `Storage` instance with an anonymous cloud connection
//! ([`use_participant`]). The app-wide connection requires a login (`CloudStorage::new` checks the
//! session), and sharing one connection would let the login state of the app overwrite the
//! anonymous one.

mod fields;
mod form;
mod lang;
mod register;
mod team;
mod views;

use dioxus::prelude::*;
use uuid::Uuid;

use crate::config::Config;
use crate::data::Storage;
use crate::storage::StorageManager;
use crate::web::origin;

pub use register::ShareRegisterPage;
pub use team::{TeamDeepLink, TeamPage, TeamVerifyPage};

/// The participant's own storage. Call [`use_participant`] in the page component and
/// [`Participant::connect`] at the start of every resource that talks to the backend.
#[derive(Clone)]
pub struct Participant {
    storage: Result<Storage, String>,
    config: Signal<Option<Config>>,
}

pub fn use_participant() -> Participant {
    let config = use_context::<Signal<Option<Config>>>();
    // Created once, in the scope of the page (a child scope would trigger Dioxus' "hoisted" warning).
    let storage = use_hook(|| StorageManager::new().map(Storage::new));
    Participant { storage, config }
}

impl Participant {
    /// `None` if the browser storage could not be opened (then `connect` reports the reason).
    pub fn storage(&self) -> Option<Storage> {
        self.storage.clone().ok()
    }

    /// Connects anonymously (once; later calls return immediately).
    pub async fn connect(&self) -> Result<Storage, String> {
        let storage = self.storage.clone()?;
        if storage.is_cloud_connected().await {
            return Ok(storage);
        }
        let base_url = self
            .config
            .peek()
            .as_ref()
            .map(|config| config.api_base_url())
            .ok_or_else(|| "The configuration is not loaded.".to_string())?;
        storage.connect_cloud_anonymous(base_url).await?;
        Ok(storage)
    }
}

/// Personal link of a team, in the shape the API mails it (`…/project/{id}/team/{team}#token=…`).
/// The token is the team's only credential: anybody with the link can edit, cancel or delete the
/// team. Older links of the form `/cook-and-run/{id}/team/{team}/{token}` keep working.
pub fn team_url(cook_and_run_id: Uuid, team_id: Uuid, access_token: Uuid) -> String {
    crate::deeplink::team_link(&origin().unwrap_or_default(), cook_and_run_id, team_id, access_token)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Route;

    #[test]
    fn the_new_link_opens_the_team_page_and_keeps_the_token_out_of_the_path() {
        let (project, team) = (Uuid::from_u128(1), Uuid::from_u128(2));
        let url = Route::TeamDeepLink { cook_and_run_id: project, team_id: team }.to_string();
        assert_eq!(url, format!("/project/{project}/team/{team}"));
        assert!(url.parse::<Route>().ok() == Some(Route::TeamDeepLink { cook_and_run_id: project, team_id: team }));
    }

    #[test]
    fn the_old_path_links_still_resolve() {
        let (project, team, token) = (Uuid::from_u128(1), Uuid::from_u128(2), Uuid::from_u128(3));
        let url = Route::TeamPage { cook_and_run_id: project, team_id: team, access_token: token }.to_string();
        assert_eq!(url, format!("/cook-and-run/{project}/team/{team}/{token}"));
        // And the route can be parsed back.
        assert!(
            url.parse::<Route>().ok()
                == Some(Route::TeamPage { cook_and_run_id: project, team_id: team, access_token: token })
        );
    }

    #[test]
    fn the_verification_link_is_its_own_route() {
        let (project, team, token) = (Uuid::from_u128(1), Uuid::from_u128(2), Uuid::from_u128(3));
        let url = format!("/cook-and-run/{project}/team/{team}/{token}/verify");
        assert!(
            url.parse::<Route>().ok()
                == Some(Route::TeamVerifyPage { cook_and_run_id: project, team_id: team, access_token: token })
        );
    }
}
