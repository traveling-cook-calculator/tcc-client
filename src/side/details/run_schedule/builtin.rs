//! Where the built-in graphics of the schedule designer are. The list of graphics (keys, labels)
//! is `schedule_design::BUILTIN_GRAPHICS`; here every key gets its file. A test makes sure that
//! the two lists agree.

use dioxus::prelude::*;

use crate::schedule_design::builtin_key;

macro_rules! graphics {
    ($($key:literal => $path:literal),* $(,)?) => {
        fn asset_for(key: &str) -> Option<Asset> {
            match key {
                $($key => Some(asset!($path)),)*
                _ => None,
            }
        }
    };
}

graphics! {
    "pot" => "/assets/pot.png",
    "spatula" => "/assets/spatula.png",
    "carrot" => "/assets/carrot.png",
    "cake" => "/assets/cake.png",
    "leaf" => "/assets/leaf_1.png",
    "herb" => "/assets/leaf_2.png",
    "cocktail" => "/assets/laufzettel/cocktail.svg",
    "pizza" => "/assets/laufzettel/pizza.svg",
    "wine_glass" => "/assets/laufzettel/wine_glass.svg",
    "spaghetti" => "/assets/laufzettel/spaghetti_lineart.svg",
    "chef_hat" => "/assets/laufzettel/chef_hat.svg",
    "fork_spoon" => "/assets/laufzettel/fork_spoon.svg",
    "sparkle" => "/assets/laufzettel/sparkle.svg",
    "burst_left" => "/assets/laufzettel/burst_left.svg",
    "burst_right" => "/assets/laufzettel/burst_right.svg",
    "squiggle_arrow" => "/assets/laufzettel/squiggle_arrow.svg",
    "icon_person" => "/assets/laufzettel/icon_person.svg",
    "icon_mail" => "/assets/laufzettel/icon_mail.svg",
    "icon_phone" => "/assets/laufzettel/icon_phone.svg",
    "icon_pin" => "/assets/laufzettel/icon_pin.svg",
    "icon_warning" => "/assets/laufzettel/icon_warning.svg",
    "icon_person_dark" => "/assets/laufzettel/icon_person_dark.svg",
    "icon_mail_dark" => "/assets/laufzettel/icon_mail_dark.svg",
    "icon_phone_dark" => "/assets/laufzettel/icon_phone_dark.svg",
    "icon_pin_dark" => "/assets/laufzettel/icon_pin_dark.svg",
    "icon_warning_dark" => "/assets/laufzettel/icon_warning_dark.svg",
    "icon_person_light" => "/assets/laufzettel/icon_person_light.svg",
    "icon_mail_light" => "/assets/laufzettel/icon_mail_light.svg",
    "icon_phone_light" => "/assets/laufzettel/icon_phone_light.svg",
    "icon_pin_light" => "/assets/laufzettel/icon_pin_light.svg",
    "icon_warning_light" => "/assets/laufzettel/icon_warning_light.svg",
}

/// What to put into `<img src>` for a picture of a design: a built-in graphic becomes the URL of
/// its file, an uploaded picture is already a data URL.
pub fn image_src(data_url: &str) -> String {
    match builtin_key(data_url).and_then(asset_for) {
        Some(asset) => asset.to_string(),
        None => data_url.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schedule_design::BUILTIN_GRAPHICS;

    #[test]
    fn every_graphic_has_a_file_reference() {
        for graphic in BUILTIN_GRAPHICS {
            assert!(asset_for(graphic.key).is_some(), "{} is not listed in builtin.rs", graphic.key);
        }
    }

    #[test]
    fn an_uploaded_picture_stays_as_it_is() {
        assert_eq!(image_src("data:image/png;base64,AA=="), "data:image/png;base64,AA==");
        assert_eq!(image_src("builtin:does_not_exist"), "builtin:does_not_exist");
    }
}
