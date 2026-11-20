//! The configuration files of the repository and of the deployment examples have to be readable
//! by the app (a typo would only show when the page is opened).

use tcc::config::Config;

fn parse(text: &str) -> Config {
    serde_json::from_str(text).expect("config.json is readable")
}

#[test]
fn the_development_config_is_readable_and_names_the_api() {
    let config = parse(include_str!("../public/config.json"));
    assert_eq!(config.api_base_url(), "http://localhost:3000");
    assert_eq!(config.legal.retention_participant_data_days, Some(30));
}

#[test]
fn a_config_without_the_new_sections_still_works() {
    // As deployed before: no `api` block, no retention periods.
    let config = parse(
        r#"{"auth":{"domain":"https://auth.example.org","realm":"r","client_id":"c",
            "redirect":"https://app.example.org","audience":"https://api.example.org"},
            "legal":{"operator_name":"o","address_street":"s","address_zip":"z","address_city":"c",
            "address_country":"d","contact_email":"e","hosting_provider":"h","server_location":"l",
            "data_retention_logs_days":7,"cookie_consent_enabled":false,"consent_version":"1",
            "app_name":"a"}}"#,
    );
    assert_eq!(config.api_base_url(), "https://api.example.org");
    assert_eq!(config.legal.retention_project_days, None);
}
