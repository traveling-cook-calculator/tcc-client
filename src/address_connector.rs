//! Address lookup via Nominatim (OpenStreetMap).

use serde::Deserialize;

/// Error text of [`get_address`] when the service found nothing.
pub const NOT_FOUND: &str = "Address not found";

/// Best match for a free-text address. Only called on an explicit user action
/// (never while typing), as the Nominatim usage policy requires.
pub async fn get_address(address: &str) -> Result<Feature, String> {
    let url = format!(
        "https://nominatim.openstreetmap.org/search?q={}&format=jsonv2&addressdetails=1&limit=1",
        urlencoding::encode(address)
    );

    let features: Vec<Feature> = reqwest::get(&url)
        .await
        .map_err(|e| format!("Request failed: {e}"))?
        .error_for_status()
        .map_err(|e| format!("The address service answered with an error: {e}"))?
        .json()
        .await
        .map_err(|e| format!("Response parsing failed: {e}"))?;

    features.into_iter().next().ok_or_else(|| NOT_FOUND.to_string())
}

// --- Structs for deserialization ---

#[derive(Debug, Deserialize, Clone)]
pub struct Feature {
    #[serde(deserialize_with = "deserialize_f64")]
    pub lat: f64,
    #[serde(deserialize_with = "deserialize_f64")]
    pub lon: f64,
    pub address: Address,
}

impl Feature {
    /// `Street 12, 12345 City` – only the parts that exist (the old code printed `-` for gaps).
    pub fn formatted(&self) -> String {
        let join = |parts: &[&str]| {
            parts.iter().copied().filter(|part| !part.is_empty()).collect::<Vec<_>>().join(" ")
        };
        let address = &self.address;
        let street = join(&[
            address.road.as_deref().unwrap_or_default(),
            address.house_number.as_deref().unwrap_or_default(),
        ]);
        let city = join(&[address.postcode.as_deref().unwrap_or_default(), &address.get_city()]);
        [street, city]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct Address {
    pub house_number: Option<String>,
    pub road: Option<String>,
    pub postcode: Option<String>,
    village: Option<String>,
    town: Option<String>,
    city: Option<String>,
}

impl Address {
    pub fn get_city(&self) -> String {
        self.city
            .as_ref()
            .or(self.town.as_ref())
            .or(self.village.as_ref())
            .cloned()
            .unwrap_or_default()
    }
}

fn deserialize_f64<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: String = serde::Deserialize::deserialize(deserializer)?;
    s.parse::<f64>().map_err(serde::de::Error::custom)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn feature(address: serde_json::Value) -> Feature {
        serde_json::from_value(json!({ "lat": "50.1", "lon": "8.6", "address": address })).unwrap()
    }

    #[test]
    fn test_geojson_response_parsing() {
        let json_data = json!([
          {
            "place_id": 126313729,
            "lat": "50.1127197",
            "lon": "8.6830441",
            "name": "Kleinmarkthalle",
            "address": {
              "tourism": "Kleinmarkthalle",
              "house_number": "5-7",
              "road": "Hasengasse",
              "suburb": "Altstadt",
              "city": "Frankfurt",
              "postcode": "60311",
              "country_code": "de"
            }
          }
        ]);

        let features: Vec<Feature> = serde_json::from_value(json_data).unwrap();
        assert_eq!(features.len(), 1);
        let feature = &features[0];
        assert_eq!(feature.lat, 50.1127197);
        assert_eq!(feature.lon, 8.6830441);
        assert_eq!(feature.address.house_number.as_deref(), Some("5-7"));
        assert_eq!(feature.formatted(), "Hasengasse 5-7, 60311 Frankfurt");
    }

    #[test]
    fn city_falls_back_to_town_then_village() {
        assert_eq!(feature(json!({ "town": "Kleinstadt", "village": "Dorf" })).address.get_city(), "Kleinstadt");
        assert_eq!(feature(json!({ "village": "Dorf" })).address.get_city(), "Dorf");
        assert_eq!(feature(json!({})).address.get_city(), "");
    }

    #[test]
    fn formatting_skips_missing_parts_instead_of_printing_dashes() {
        assert_eq!(feature(json!({ "road": "Hasengasse", "city": "Frankfurt" })).formatted(), "Hasengasse, Frankfurt");
        assert_eq!(feature(json!({ "postcode": "60311" })).formatted(), "60311");
        assert_eq!(feature(json!({})).formatted(), "");
    }

    #[test]
    fn invalid_coordinates_fail_to_parse() {
        let parsed: Result<Feature, _> =
            serde_json::from_value(json!({ "lat": "north", "lon": "8.6", "address": {} }));
        assert!(parsed.is_err());
    }
}
