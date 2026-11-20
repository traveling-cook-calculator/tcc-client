use base64::engine::general_purpose;
use base64::Engine;
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;
use web_sys::{console, window};

use crate::config::AuthConfig;

const SCOPE: &str = "openid";

#[derive(Debug, Clone)]
pub enum AuthState {
    Loading(AuthConfig, OidcDiscovery, ProcessData),
    LoggedOut(AuthConfig, OidcDiscovery),
    LoggedIn(AuthConfig, OidcDiscovery, SessionData),
    Error(AuthConfig, OidcDiscovery, String),
    NotAvailable(),
}

#[derive(Debug, Clone, Deserialize)]
pub struct OidcDiscovery {
    authorization_endpoint: String,
    token_endpoint: String,
    userinfo_endpoint: String,
    end_session_endpoint: String,
}

#[derive(Serialize)]
struct TokenRequest<'a> {
    grant_type: &'a str,
    client_id: &'a str,
    code_verifier: &'a str,
    code: &'a str,
    redirect_uri: &'a str,
}

#[derive(Serialize)]
struct RefreshRequest<'a> {
    grant_type: &'a str,
    client_id: &'a str,
    refresh_token: &'a str,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    id_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    expires_in: i64,
}

fn get_storage() -> Result<web_sys::Storage, String> {
    window()
        .ok_or_else(|| "No window object available".to_string())?
        .local_storage()
        .map_err(|_| "Could not access local storage".to_string())?
        .ok_or_else(|| "Local storage is not available".to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UserData {
    pub sub: String,
    pub preferred_username: Option<String>,
    pub email: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionData {
    pub access_token: String,
    pub id_token: String,
    pub refresh_token: Option<String>,
    pub valid_until: NaiveDateTime,
    pub user: UserData,
}

impl SessionData {
    fn load() -> Option<Self> {
        let data = get_storage().ok()?.get_item("session_data").ok()??;
        serde_json::from_str(&data).ok()
    }

    fn save(&self) -> Result<(), String> {
        get_storage()?
            .set_item("session_data", &serde_json::to_string(self).unwrap())
            .map_err(|_| "Could not write session_data to local storage".to_string())
    }

    fn clear() -> Result<(), String> {
        get_storage()?
            .remove_item("session_data")
            .map_err(|_| "Could not remove session_data from local storage".to_string())
    }

    pub fn is_valid_for(&self, margin_secs: i64) -> bool {
        let threshold = chrono::Local::now().naive_local() + chrono::Duration::seconds(margin_secs);
        self.valid_until > threshold
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProcessData {
    pub code_verifier: String,
    /// Random one-time value, compared with the `state` that comes back from Keycloak.
    pub state: String,
    /// Route (URL path) the user wants to land on after the login.
    #[serde(default)]
    pub return_to: String,
}

impl ProcessData {
    fn load() -> Option<Self> {
        let data = get_storage().ok()?.get_item("process_data").ok()??;
        serde_json::from_str(&data).ok()
    }

    fn save(&self) -> Result<(), String> {
        get_storage()?
            .set_item("process_data", &serde_json::to_string(self).unwrap())
            .map_err(|_| "Could not write process_data to local storage".to_string())
    }

    fn clear() -> Result<(), String> {
        get_storage()?
            .remove_item("process_data")
            .map_err(|_| "Could not remove process_data from local storage".to_string())
    }
}

impl AuthState {
    pub async fn new(config: AuthConfig) -> Self {
        console::debug_1(&"Initializing Keycloak authentication...".into());
        let oidc_discovery = match fetch_discovery(&config).await {
            Ok(oidc_discovery) => oidc_discovery,
            Err(e) => {
                console::error_1(&format!("Error fetching OIDC discovery: {}", e).into());
                return AuthState::NotAvailable();
            }
        };
        console::debug_1(&"OIDC discovery fetched successfully".into());
        console::debug_1(&"Loading session data...".into());
        if let Some(session) = SessionData::load() {
            let auth_state = AuthState::LoggedIn(config, oidc_discovery, session);
            console::debug_1(&"Session data loaded, checking validity...".into());
            return auth_state.refresh().await;
        }

        if let Some(process_data) = ProcessData::load() {
            if on_callback_route() {
                console::debug_1(&"Login response is being processed".into());
                return AuthState::Loading(config, oidc_discovery, process_data);
            }
            // A login was started but never finished (tab closed, back button, …). The stored data
            // is stale: keeping it made the app wait for a response that will never come.
            console::debug_1(&"Discarding an abandoned login attempt".into());
            let _ = ProcessData::clear();
        }

        console::debug_1(&"No ongoing auth process, user is logged out".into());
        AuthState::LoggedOut(config, oidc_discovery)
    }

    /// Starts the login. Returns the Keycloak URL to navigate to. The app state stays
    /// `LoggedOut`: the page is left right away, and `Loading` only makes sense on the callback
    /// page (a restored page – back button – would otherwise show a spinner forever).
    pub async fn login(self, return_to: String) -> (Self, Option<String>) {
        console::debug_1(&"Starting Keycloak login process...".into());

        let (config, oidc_discovery) = match self {
            AuthState::Loading(config, od, _) => (config, od),
            AuthState::LoggedOut(config, od) => (config, od),
            AuthState::LoggedIn(_, _, _) => {
                console::info_1(&"Already logged in".into());
                return (self, None);
            }
            AuthState::Error(config, od, _) => (config, od),
            AuthState::NotAvailable() => {
                console::warn_1(&"AuthState is not suitable for login".into());
                return (self, None);
            }
        };

        let code_verifier = generate_random_string(128);
        let code_challenge = generate_code_challenge(&code_verifier);

        let state = generate_random_string(32);
        let process_data = ProcessData {
            code_verifier,
            state: state.clone(),
            return_to,
        };

        if let Err(e) = process_data.save() {
            console::error_1(&format!("Error saving process data: {}", e).into());
            return (
                AuthState::Error(
                    config,
                    oidc_discovery,
                    "Error saving process data".to_string(),
                ),
                None,
            );
        }

        let auth_url = authorization_url(&config, &oidc_discovery, &state, &code_challenge, "");

        (AuthState::LoggedOut(config, oidc_discovery), Some(auth_url))
    }

    pub async fn callback(self, code: &str, state: &str) -> Self {
        // (the authorization code and the state are not logged)
        console::debug_1(&"Handling login callback...".into());

        let (config, discovery, process_data) = match self {
            AuthState::Loading(config, discovery, p) => (config, discovery, p),
            invalid => {
                console::warn_1(
                    &format!("Received callback in invalid state: {:?}", invalid).into(),
                );
                return invalid;
            }
        };

        let token_response =
            match exchange_code_for_token(&config, &discovery, &process_data, code, state).await {
                Ok(r) => r,
                Err(e) => {
                    let _ = ProcessData::clear();
                    return AuthState::Error(config, discovery, e);
                }
            };

        if let Err(e) = ProcessData::clear() {
            console::error_1(&format!("Error clearing process data: {}", e).into());
        }

        match get_user_info(&discovery, &token_response.access_token).await {
            Ok(user) => {
                let valid_until = chrono::Local::now().naive_local()
                    + chrono::Duration::seconds(token_response.expires_in);

                let session_data = SessionData {
                    access_token: token_response.access_token,
                    id_token: token_response.id_token,
                    refresh_token: token_response.refresh_token,
                    valid_until,
                    user,
                };
                if let Err(e) = session_data.save() {
                    console::error_1(&format!("Error saving session: {}", e).into());
                    return AuthState::Error(config, discovery, "Error saving session".to_string());
                }
                return AuthState::LoggedIn(config, discovery, session_data);
            }
            Err(e) => return AuthState::Error(config, discovery, e),
        }
    }

    /// Verlängert eine abgelaufene Session mittels Refresh-Token, ohne den User
    /// erneut zur Keycloak-Login-Seite zu schicken.
    ///
    /// # Typischer Aufruf
    /// ```rust
    /// // Im Component, bevor ein API-Call gemacht wird:
    /// if let AuthState::LoggedIn(s) = &*auth {
    ///     if !s.is_valid_for(60) {
    ///         *auth.write() = auth.read().refresh().await;
    ///     }
    /// }
    /// ```
    pub async fn refresh(self) -> Self {
        self.refresh_with_margin(300).await
    }

    /// Like [`AuthState::refresh`], but renews the token as soon as it is valid for less than
    /// `margin_secs` seconds.
    pub async fn refresh_with_margin(self, margin_secs: i64) -> Self {
        let (config, oidc_discovery, session_data) = match self {
            AuthState::LoggedIn(config, oidc_discovery, session_data) => {
                console::debug_1(&"User is logged in!".into());
                (config, oidc_discovery, session_data)
            }
            other => {
                console::warn_1(
                    &format!("Cannot refresh token when not logged in: {:?}", other).into(),
                );
                return other;
            }
        };

        console::debug_1(&"Checking if session is still valid...".into());
        if session_data.is_valid_for(margin_secs) {
            console::debug_1(&"Session is still valid, no need to refresh".into());
            return AuthState::LoggedIn(config, oidc_discovery, session_data);
        }

        console::debug_1(&"Session is expired or about to expire, attempting to refresh...".into());
        let refresh_token = match &session_data.refresh_token {
            Some(rt) => rt.clone(),
            None => {
                console::error_1(&"No refresh token available – full re-login required".into());
                let _ = SessionData::clear();
                return AuthState::LoggedOut(config, oidc_discovery);
            }
        };

        let refresh_request = RefreshRequest {
            grant_type: "refresh_token",
            client_id: &config.client_id,
            refresh_token: &refresh_token,
        };

        let client = reqwest::Client::new();
        let response = match client
            .post(&oidc_discovery.token_endpoint)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .form(&refresh_request)
            .send()
            .await
        {
            Ok(r) => {
                console::debug_1(&"Refresh request sent successfully".into());
                r
            }
            Err(e) => {
                console::error_1(&format!("Refresh request failed: {}", e).into());
                return AuthState::Error(
                    config,
                    oidc_discovery,
                    format!("Refresh request failed: {}", e),
                );
            }
        };

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            if !status.is_client_error() {
                // Keycloak is down or overloaded (5xx): the session itself is probably fine.
                // Keep it, so that the next attempt can still succeed.
                console::error_1(&format!("Refresh failed temporarily ({}): {}", status, body).into());
                return AuthState::Error(
                    config,
                    oidc_discovery,
                    format!("Refresh failed temporarily ({})", status),
                );
            }
            // Refresh Token ist abgelaufen oder widerrufen → User muss sich neu anmelden
            console::error_1(&format!("Refresh token rejected ({}): {}", status, body).into());
            let _ = SessionData::clear();
            return AuthState::LoggedOut(config, oidc_discovery);
        }

        let token_response: TokenResponse = match response.json().await {
            Ok(t) => t,
            Err(e) => {
                console::error_1(&format!("Error parsing refresh response: {}", e).into());
                return AuthState::Error(
                    config,
                    oidc_discovery,
                    format!("Error parsing refresh response: {}", e),
                );
            }
        };

        let valid_until = chrono::Local::now().naive_local()
            + chrono::Duration::seconds(token_response.expires_in);

        // The user does not change with a refresh: no extra userinfo request that could fail.
        let new_session = SessionData {
            access_token: token_response.access_token,
            id_token: token_response.id_token,
            // Keycloak may rotate the refresh token; if it sends none, the old one stays valid.
            refresh_token: token_response.refresh_token.or(Some(refresh_token)),
            valid_until,
            user: session_data.user.clone(),
        };

        if let Err(e) = new_session.save() {
            let _ = ProcessData::clear();
            console::error_1(&format!("Error saving refreshed session: {}", e).into());
            return AuthState::Error(
                config,
                oidc_discovery,
                format!("Error saving refreshed session: {}", e),
            );
        }

        console::debug_1(&"Session successfully refreshed".into());
        AuthState::LoggedIn(config, oidc_discovery, new_session)
    }

    /// The access token of a logged in user (used to see whether a session was replaced).
    /// The logged in user as the login server describes him.
    pub fn user(&self) -> Option<&UserData> {
        match self {
            AuthState::LoggedIn(_, _, session) => Some(&session.user),
            _ => None,
        }
    }

    /// Keycloak's own account page of the user: there he changes the data of the login account
    /// (name, email address, password, sessions). The app does not copy any of that.
    pub fn account_console_url(&self) -> Option<String> {
        match self {
            AuthState::LoggedIn(config, _, _) => Some(account_console_url(config)),
            _ => None,
        }
    }

    pub fn access_token(&self) -> Option<&str> {
        match self {
            AuthState::LoggedIn(_, _, session) => Some(session.access_token.as_str()),
            _ => None,
        }
    }

    /// Periodic check for a long-open page: renews the token before it expires.
    ///
    /// Returns the new state, or `None` if nothing has to change:
    /// * token still valid for `margin_secs` → `None` (purely local, no request),
    /// * renewed → `LoggedIn` with the new session,
    /// * refresh token rejected or the session ran out → `LoggedOut`,
    /// * temporary problem (offline, Keycloak down) → `None`; the old session stays and the next
    ///   check tries again. A background check must never log the user out because of a bad minute.
    pub async fn keep_alive(&self, margin_secs: i64) -> Option<Self> {
        let AuthState::LoggedIn(config, discovery, session) = self else {
            return None;
        };

        // Another tab of this app may have renewed the session already. Refresh tokens can be
        // single-use, so using our outdated one could end the session for both tabs: adopt the
        // newer session from the browser storage instead.
        let mut current = self.clone();
        let mut adopted = false;
        if let Some(stored) = SessionData::load()
            && stored.user == session.user
            && stored.valid_until > session.valid_until
        {
            current = AuthState::LoggedIn(config.clone(), discovery.clone(), stored);
            adopted = true;
        }
        let AuthState::LoggedIn(_, _, session) = &current else {
            return None;
        };

        if session.is_valid_for(margin_secs) {
            return adopted.then_some(current);
        }
        if session.refresh_token.is_none() {
            // Nothing to renew with: the user stays logged in until the token really ends.
            if session.is_valid_for(0) {
                return adopted.then_some(current);
            }
            let _ = SessionData::clear();
            return Some(AuthState::LoggedOut(config.clone(), discovery.clone()));
        }

        match current.refresh_with_margin(margin_secs).await {
            renewed @ (AuthState::LoggedIn(..) | AuthState::LoggedOut(..)) => Some(renewed),
            AuthState::Error(_, _, message) => {
                console::warn_1(&format!("Session refresh failed, will retry: {message}").into());
                None
            }
            _ => None,
        }
    }

    /// Starts Keycloak's "delete account" action for the logged in user and returns the URL to
    /// navigate to. Call it **after** the API data is gone. The local session is dropped right
    /// away; when Keycloak redirects back, [`AuthState::finish_account_deletion`] ends the flow.
    ///
    /// Needs the realm to offer the required action `delete_account` and the user to have the role
    /// `delete-account` of the client `account` (see the realm of the test environment).
    pub async fn request_account_deletion(self, return_to: String) -> (Self, Option<String>) {
        let (config, oidc_discovery) = match self {
            AuthState::LoggedIn(config, oidc_discovery, _) => (config, oidc_discovery),
            other => {
                console::warn_1(&"Cannot delete the account when not logged in".into());
                return (other, None);
            }
        };

        let code_verifier = generate_random_string(128);
        let code_challenge = generate_code_challenge(&code_verifier);
        let state = generate_random_string(32);
        let process_data = ProcessData { code_verifier, state: state.clone(), return_to };
        if let Err(e) = process_data.save() {
            console::error_1(&format!("Error saving process data: {}", e).into());
            return (AuthState::Error(config, oidc_discovery, "Error saving process data".to_string()), None);
        }
        if let Err(e) = SessionData::clear() {
            console::warn_1(&format!("Error clearing session data: {}", e).into());
        }

        let url = authorization_url(&config, &oidc_discovery, &state, &code_challenge, "&kc_action=delete_account");
        (AuthState::LoggedOut(config, oidc_discovery), Some(url))
    }

    /// Ends the account deletion when Keycloak sends the browser back: no code is exchanged (the
    /// account is gone or the user backed out), nothing of the session stays.
    pub fn finish_account_deletion(self) -> Self {
        let _ = ProcessData::clear();
        let _ = SessionData::clear();
        match self {
            AuthState::Loading(config, od, _)
            | AuthState::LoggedOut(config, od)
            | AuthState::LoggedIn(config, od, _)
            | AuthState::Error(config, od, _) => AuthState::LoggedOut(config, od),
            AuthState::NotAvailable() => AuthState::NotAvailable(),
        }
    }

    pub async fn logout(self) -> (Self, Option<String>) {
        console::debug_1(&"Starting Keycloak logout process...".into());

        let (config, oidc_discovery, session_data) = match self {
            AuthState::LoggedIn(config, oidc_discovery, session_data) => {
                (config, oidc_discovery, session_data)
            }
            auth_state => {
                console::warn_1(&"Cannot logout when not logged in".into());
                return (auth_state, None);
            }
        };

        let logout_url = format!(
            "{}?client_id={}&id_token_hint={}&post_logout_redirect_uri={}",
            oidc_discovery.end_session_endpoint,
            urlencoding::encode(&config.client_id),
            urlencoding::encode(&session_data.id_token),
            urlencoding::encode(&config.redirect),
        );

        if let Err(e) = SessionData::clear() {
            console::warn_1(&format!("Error clearing session data during logout: {}", e).into());
        }

        console::debug_1(&format!("Keycloak logout URL: {}", logout_url).into());

        (
            AuthState::LoggedOut(config, oidc_discovery),
            Some(logout_url),
        )
    }
}

/// Authorization request of the code flow with PKCE. `extra` is appended as is (already encoded),
/// e.g. `&kc_action=delete_account`.
fn authorization_url(
    config: &AuthConfig,
    discovery: &OidcDiscovery,
    state: &str,
    code_challenge: &str,
    extra: &str,
) -> String {
    format!(
        "{}?response_type=code&client_id={}&redirect_uri={}&scope={}&state={}&code_challenge={}&code_challenge_method=S256{}",
        discovery.authorization_endpoint,
        urlencoding::encode(&config.client_id),
        urlencoding::encode(&format!("{}/callback", &config.redirect)),
        urlencoding::encode(SCOPE),
        urlencoding::encode(state),
        code_challenge,
        extra,
    )
}

async fn get_user_info(discovery: &OidcDiscovery, access_token: &str) -> Result<UserData, String> {
    let client = reqwest::Client::new();
    let response = client
        .get(&discovery.userinfo_endpoint)
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| format!("UserInfo request error: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("UserInfo request failed ({}): {}", status, body));
    }

    response
        .json::<UserData>()
        .await
        .map_err(|e| format!("Error parsing UserInfo response: {}", e))
}

async fn exchange_code_for_token(
    config: &AuthConfig,
    discovery: &OidcDiscovery,
    process_data: &ProcessData,
    code: &str,
    state: &str,
) -> Result<TokenResponse, String> {
    if state != process_data.state {
        return Err("State mismatch – possible CSRF attack".to_string());
    }

    let token_request = TokenRequest {
        grant_type: "authorization_code",
        client_id: &config.client_id,
        code_verifier: &process_data.code_verifier,
        code,
        redirect_uri: &format!("{}/callback", &config.redirect),
    };

    let client = reqwest::Client::new();
    let response = client
        .post(&discovery.token_endpoint)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .form(&token_request)
        .send()
        .await
        .map_err(|e| format!("Token request error: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(format!("Token request failed ({}): {}", status, body));
    }

    response
        .json::<TokenResponse>()
        .await
        .map_err(|e| format!("Error parsing token response: {}", e))
}

/// Cryptographically secure random string (`[0-9a-f]`, allowed for PKCE verifiers).
/// Uses UUID v4, which draws from `crypto.getRandomValues` in the browser – the previous
/// `Math.random()` is predictable and must not be used for PKCE or `state`.
fn generate_random_string(length: usize) -> String {
    let mut out = String::with_capacity(length + 32);
    while out.len() < length {
        out.push_str(&Uuid::new_v4().simple().to_string());
    }
    out.truncate(length);
    out
}

/// The login response arrives on `/callback` (see `Route::Callback`).
fn is_callback_path(path: &str) -> bool {
    path.trim_end_matches('/').ends_with("/callback")
}

fn on_callback_route() -> bool {
    window()
        .and_then(|window| window.location().pathname().ok())
        .is_some_and(|path| is_callback_path(&path))
}

fn generate_code_challenge(code_verifier: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(code_verifier.as_bytes());
    general_purpose::URL_SAFE_NO_PAD.encode(hasher.finalize())
}

fn account_console_url(config: &AuthConfig) -> String {
    format!(
        "{}/realms/{}/account",
        config.domain.trim_end_matches('/'),
        urlencoding::encode(&config.realm),
    )
}

async fn fetch_discovery(config: &AuthConfig) -> Result<OidcDiscovery, String> {
    let url = format!(
        "{}/realms/{}/.well-known/openid-configuration",
        &config.domain,
        urlencoding::encode(&config.realm),
    );

    console::debug_1(&format!("Fetching OIDC discovery from {}", url).into());

    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("OIDC discovery request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!(
            "OIDC discovery request failed ({})",
            response.status()
        ));
    }

    let discovery: OidcDiscovery = response
        .json()
        .await
        .map_err(|e| format!("Error parsing OIDC discovery response: {}", e))?;

    Ok(discovery)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_challenge_matches_the_rfc_7636_example() {
        // RFC 7636, appendix B.
        assert_eq!(
            generate_code_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn random_strings_have_the_requested_length_and_allowed_characters() {
        for length in [1, 31, 32, 33, 43, 128] {
            let value = generate_random_string(length);
            assert_eq!(value.len(), length);
            assert!(value.chars().all(|c| c.is_ascii_hexdigit()), "{value}");
        }
    }

    #[test]
    fn random_strings_do_not_repeat() {
        let all: std::collections::HashSet<String> = (0..200).map(|_| generate_random_string(128)).collect();
        assert_eq!(all.len(), 200);
    }

    #[test]
    fn only_the_callback_path_counts_as_callback() {
        assert!(is_callback_path("/callback"));
        assert!(is_callback_path("/callback/"));
        assert!(is_callback_path("/app/callback"));
        assert!(!is_callback_path("/"));
        assert!(!is_callback_path("/cook-and-run/1/overview"));
        assert!(!is_callback_path("/callbacks"));
    }
}

#[cfg(test)]
mod account_url_tests {
    use super::*;

    #[test]
    fn the_account_console_is_the_realms_account_page() {
        let config = AuthConfig {
            domain: "https://auth.example.org/".into(),
            realm: "cook and run".into(),
            client_id: "tcc".into(),
            redirect: "https://app.example.org".into(),
            audience: "tcc".into(),
        };
        assert_eq!(account_console_url(&config), "https://auth.example.org/realms/cook%20and%20run/account");
    }
}
