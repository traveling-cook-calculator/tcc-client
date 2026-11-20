//! Links that the API puts into its mails, and the parts of them the app has to read.
//!
//! * Team link (invitation, route update): `{app}/project/{project}/team/{team}#token={token}`.
//!   The secret token travels in the URL **fragment**: browsers never send it to a server, so it
//!   stays out of proxy and access logs.
//! * Confirmation link (invitation mail only): the same, plus `&verify={secret}`. The secret is
//!   what proves that the mailbox was read: the team link the guest keeps does not have it.
//! * Organizer link (notification): `{app}/admin/project/{project}[/team/{team}]`.
//!
//! No browser code in here, so everything is covered by the host tests.

use uuid::Uuid;

/// The token of a team link, taken from a URL fragment (`#token=<uuid>`). Further `&key=value`
/// pairs are tolerated. Anything that is not a valid, non-nil UUID yields `None`.
pub fn token_from_fragment(fragment: &str) -> Option<Uuid> {
    let fragment = fragment.trim().trim_start_matches('#');
    fragment
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(key, _)| *key == "token")
        .and_then(|(_, value)| Uuid::parse_str(value.trim()).ok())
        .filter(|token| !token.is_nil())
}

/// The secret of a confirmation link (`#token=<uuid>&verify=<uuid>`). Same rules as the token.
pub fn verify_secret_from_fragment(fragment: &str) -> Option<Uuid> {
    let fragment = fragment.trim().trim_start_matches('#');
    fragment
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(key, _)| *key == "verify")
        .and_then(|(_, value)| Uuid::parse_str(value.trim()).ok())
        .filter(|secret| !secret.is_nil())
}

/// Personal link of a team, as the API mails it.
pub fn team_link(origin: &str, project: Uuid, team: Uuid, token: Uuid) -> String {
    format!("{}/project/{project}/team/{team}#token={token}", origin.trim_end_matches('/'))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "7b1c2f0e-4d3a-4c55-9a51-0d6a5f1e2c33";

    #[test]
    fn reads_the_token_from_the_fragment() {
        let expected = Uuid::parse_str(TOKEN).ok();
        assert_eq!(token_from_fragment(&format!("#token={TOKEN}")), expected);
        assert_eq!(token_from_fragment(&format!("token={TOKEN}")), expected);
        assert_eq!(token_from_fragment(&format!("#lang=de&token={TOKEN}")), expected);
    }

    #[test]
    fn reads_the_secret_of_a_confirmation_link() {
        let secret = Uuid::parse_str(TOKEN).ok();
        assert_eq!(verify_secret_from_fragment(&format!("#token={TOKEN}&verify={TOKEN}")), secret);
        assert_eq!(verify_secret_from_fragment(&format!("#verify={TOKEN}")), secret);
        // the plain team link has none
        assert_eq!(verify_secret_from_fragment(&format!("#token={TOKEN}")), None);
        assert_eq!(verify_secret_from_fragment("#token=x&verify=abc"), None);
        // the token is still found next to it
        assert_eq!(token_from_fragment(&format!("#token={TOKEN}&verify=whatever")), Uuid::parse_str(TOKEN).ok());
    }

    #[test]
    fn rejects_everything_that_is_no_usable_token() {
        for bad in [
            "",
            "#",
            "#token=",
            "#token=abc",
            "#tokens=7b1c2f0e-4d3a-4c55-9a51-0d6a5f1e2c33",
            "#token=00000000-0000-0000-0000-000000000000",
        ] {
            assert_eq!(token_from_fragment(bad), None, "'{bad}'");
        }
    }

    #[test]
    fn the_link_matches_what_the_api_builds() {
        let (project, team) = (Uuid::from_u128(1), Uuid::from_u128(2));
        let token = Uuid::parse_str(TOKEN).unwrap();
        let link = team_link("https://app.example.org/", project, team, token);
        assert_eq!(link, format!("https://app.example.org/project/{project}/team/{team}#token={TOKEN}"));
        let fragment = link.split_once('#').map(|(_, fragment)| fragment).unwrap_or_default();
        assert_eq!(token_from_fragment(fragment), Some(token));
    }
}
