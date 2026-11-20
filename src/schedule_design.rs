//! Data and logic of the freely designable schedule sheet ("Laufzettel"). No UI in here, so that
//! it can be tested and later be stored in the cloud as it is (everything is `serde`).
//!
//! A [`ScheduleDesign`] is a canvas with a background and a list of [`Element`]s (text, image,
//! shape) at percent positions. Texts contain `{placeholders}` that are replaced per team, see
//! [`fill`] and [`placeholder_catalog`].

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::schedule_text::{GUESTS_CLOSE, GUESTS_OPEN};
use uuid::Uuid;

pub const DESIGN_VERSION: u32 = 1;
/// Width in pixels of the exported image. The height follows from the format.
pub const EXPORT_WIDTH: u32 = 1080;
pub const MAX_ELEMENTS: usize = 60;
/// Pictures a user uploads (built-in graphics do not count: they are files of the project).
pub const MAX_IMAGES: usize = 12;
/// Everything in `ScheduleDesign::images`, uploads and built-in graphics together.
pub const MAX_ALL_IMAGES: usize = 60;
pub const MAX_DESIGNS: usize = 8;
pub const MAX_TEXT_LEN: usize = 2_000;
/// Longest name of a design, an element or a picture.
pub const MAX_NAME_LEN: usize = 60;
/// Longest replacement text of `Missing::Text`.
pub const MAX_MISSING_LEN: usize = 40;
/// The title that the presets use (and that replaces `{event_title}` in older designs).
pub const DEFAULT_TITLE: &str = "Cook & Run";
pub const MAX_LIST_ROWS: usize = 12;
pub const MAX_ROW_LEN: usize = 200;

/// `(label, height / width)`
pub const FORMATS: [(&str, f32); 5] = [
    ("Portrait 4:5 (Instagram)", 1.25),
    ("A4 / A5 portrait", 1.414),
    ("Square 1:1", 1.0),
    ("Story 9:16", 1.778),
    ("Landscape 3:2", 0.667),
];

// ─────────────────────────────────────────────
//  Built-in graphics
// ─────────────────────────────────────────────

/// Marks a `DesignImage::data_url` that is not a picture but the key of a graphic that is part of
/// the project (`assets/`). Such designs stay tiny and need no browser storage for the picture.
pub const BUILTIN_PREFIX: &str = "builtin:";

pub struct BuiltinGraphic {
    pub key: &'static str,
    pub label: &'static str,
    pub group: &'static str,
    /// Path below `assets/`.
    pub file: &'static str,
}

const fn g(
    key: &'static str,
    label: &'static str,
    group: &'static str,
    file: &'static str,
) -> BuiltinGraphic {
    BuiltinGraphic {
        key,
        label,
        group,
        file,
    }
}

pub const BUILTIN_GRAPHICS: &[BuiltinGraphic] = &[
    g("pot", "Pot", "Kitchen", "pot.png"),
    g("spatula", "Spatula", "Kitchen", "spatula.png"),
    g("carrot", "Carrot", "Kitchen", "carrot.png"),
    g("cake", "Cake slice", "Kitchen", "cake.png"),
    g("leaf", "Leaf", "Kitchen", "leaf_1.png"),
    g("herb", "Herb", "Kitchen", "leaf_2.png"),
    g("cocktail", "Cocktail", "Party", "laufzettel/cocktail.svg"),
    g("pizza", "Pizza slice", "Party", "laufzettel/pizza.svg"),
    g(
        "wine_glass",
        "Wine glass",
        "Party",
        "laufzettel/wine_glass.svg",
    ),
    g(
        "spaghetti",
        "Spaghetti (line art)",
        "Party",
        "laufzettel/spaghetti_lineart.svg",
    ),
    g("chef_hat", "Chef hat", "Party", "laufzettel/chef_hat.svg"),
    g(
        "fork_spoon",
        "Fork and spoon",
        "Party",
        "laufzettel/fork_spoon.svg",
    ),
    g("sparkle", "Sparkle", "Decoration", "laufzettel/sparkle.svg"),
    g(
        "burst_left",
        "Rays (left)",
        "Decoration",
        "laufzettel/burst_left.svg",
    ),
    g(
        "burst_right",
        "Rays (right)",
        "Decoration",
        "laufzettel/burst_right.svg",
    ),
    g(
        "squiggle_arrow",
        "Dashed arrow",
        "Decoration",
        "laufzettel/squiggle_arrow.svg",
    ),
    g(
        "icon_person",
        "Person",
        "Icons (orange)",
        "laufzettel/icon_person.svg",
    ),
    g(
        "icon_mail",
        "Mail",
        "Icons (orange)",
        "laufzettel/icon_mail.svg",
    ),
    g(
        "icon_phone",
        "Phone",
        "Icons (orange)",
        "laufzettel/icon_phone.svg",
    ),
    g(
        "icon_pin",
        "Address",
        "Icons (orange)",
        "laufzettel/icon_pin.svg",
    ),
    g(
        "icon_warning",
        "Diets",
        "Icons (orange)",
        "laufzettel/icon_warning.svg",
    ),
    g(
        "icon_person_dark",
        "Person",
        "Icons (dark)",
        "laufzettel/icon_person_dark.svg",
    ),
    g(
        "icon_mail_dark",
        "Mail",
        "Icons (dark)",
        "laufzettel/icon_mail_dark.svg",
    ),
    g(
        "icon_phone_dark",
        "Phone",
        "Icons (dark)",
        "laufzettel/icon_phone_dark.svg",
    ),
    g(
        "icon_pin_dark",
        "Address",
        "Icons (dark)",
        "laufzettel/icon_pin_dark.svg",
    ),
    g(
        "icon_warning_dark",
        "Diets",
        "Icons (dark)",
        "laufzettel/icon_warning_dark.svg",
    ),
    g(
        "icon_person_light",
        "Person",
        "Icons (white)",
        "laufzettel/icon_person_light.svg",
    ),
    g(
        "icon_mail_light",
        "Mail",
        "Icons (white)",
        "laufzettel/icon_mail_light.svg",
    ),
    g(
        "icon_phone_light",
        "Phone",
        "Icons (white)",
        "laufzettel/icon_phone_light.svg",
    ),
    g(
        "icon_pin_light",
        "Address",
        "Icons (white)",
        "laufzettel/icon_pin_light.svg",
    ),
    g(
        "icon_warning_light",
        "Diets",
        "Icons (white)",
        "laufzettel/icon_warning_light.svg",
    ),
];

/// The key if `data_url` refers to a built-in graphic that exists.
pub fn builtin_key(data_url: &str) -> Option<&str> {
    let key = data_url.strip_prefix(BUILTIN_PREFIX)?;
    BUILTIN_GRAPHICS
        .iter()
        .any(|graphic| graphic.key == key)
        .then_some(key)
}

pub fn builtin_graphic(key: &str) -> Option<&'static BuiltinGraphic> {
    BUILTIN_GRAPHICS.iter().find(|graphic| graphic.key == key)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Font {
    Chewy,
    Gluten,
    Inter,
    Playfair,
    Bebas,
    Pacifico,
    ArchivoBlack,
}

impl Font {
    pub const ALL: [Font; 7] = [
        Font::Inter,
        Font::ArchivoBlack,
        Font::Bebas,
        Font::Playfair,
        Font::Pacifico,
        Font::Chewy,
        Font::Gluten,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Font::Chewy => "Chewy (playful)",
            Font::Gluten => "Gluten (soft)",
            Font::Inter => "Inter (clean)",
            Font::Playfair => "Playfair (serif)",
            Font::Bebas => "Bebas Neue (condensed)",
            Font::Pacifico => "Pacifico (script)",
            Font::ArchivoBlack => "Archivo Black (heavy)",
        }
    }

    pub fn css(self) -> &'static str {
        match self {
            Font::Chewy => "'Chewy', cursive",
            Font::Gluten => "'Gluten', sans-serif",
            Font::Inter => "'Inter', system-ui, sans-serif",
            Font::Playfair => "'Playfair Display', Georgia, serif",
            Font::Bebas => "'Bebas Neue', Impact, sans-serif",
            Font::Pacifico => "'Pacifico', cursive",
            Font::ArchivoBlack => "'Archivo Black', 'Arial Black', sans-serif",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Font::Chewy => "chewy",
            Font::Gluten => "gluten",
            Font::Inter => "inter",
            Font::Playfair => "playfair",
            Font::Bebas => "bebas",
            Font::Pacifico => "pacifico",
            Font::ArchivoBlack => "archivo_black",
        }
    }

    pub fn from_key(key: &str) -> Option<Font> {
        Font::ALL.into_iter().find(|font| font.key() == key)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Align {
    Left,
    Center,
    Right,
}

impl Align {
    pub fn css(self) -> &'static str {
        match self {
            Align::Left => "left",
            Align::Center => "center",
            Align::Right => "right",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VAlign {
    Top,
    Middle,
    Bottom,
}

impl VAlign {
    pub fn css(self) -> &'static str {
        match self {
            VAlign::Top => "flex-start",
            VAlign::Middle => "center",
            VAlign::Bottom => "flex-end",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fit {
    Cover,
    Contain,
}

impl Fit {
    pub fn css(self) -> &'static str {
        match self {
            Fit::Cover => "cover",
            Fit::Contain => "contain",
        }
    }
}

/// What a text box shows for a value that is not set (the phone number a team did not give).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Missing {
    /// The whole line is left out, with its icon and fixed words ("Phone: {team_phone}").
    #[default]
    Hide,
    /// The line stays, the value is empty. The layout does not move.
    Empty,
    /// The value is replaced by this text, for example a dash.
    Text(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextStyle {
    /// Text with `{placeholders}`, `**bold**`, `{icon:phone}` and `{#guests}` blocks, see
    /// `schedule_text`.
    pub text: String,
    pub font: Font,
    /// In percent of the canvas width.
    pub size: f32,
    pub color: String,
    pub bold: bool,
    pub italic: bool,
    pub uppercase: bool,
    pub align: Align,
    pub valign: VAlign,
    pub line_height: f32,
    /// In percent of the font size.
    pub letter_spacing: f32,
    /// What to show for values that are not set.
    #[serde(default)]
    pub missing: Missing,
    /// The color of the icons in front of lines.
    #[serde(default = "default_tint")]
    pub icon_tint: IconTint,
    /// Make the text smaller if it does not fit into the box (many guests).
    #[serde(default)]
    pub shrink: bool,
    /// Space between two guests of a `{#guests}` block, in `em`.
    #[serde(default = "default_block_gap")]
    pub block_gap: f32,
}

fn default_true() -> bool {
    true
}

fn default_tint() -> IconTint {
    IconTint::Orange
}

fn default_block_gap() -> f32 {
    0.6
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImageStyle {
    pub image: Option<Uuid>,
    pub fit: Fit,
    /// Corner radius in percent of the canvas width.
    pub radius: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShapeStyle {
    pub fill: String,
    /// Corner radius in percent of the canvas width.
    pub radius: f32,
    pub border_color: String,
    /// In percent of the canvas width.
    pub border_width: f32,
}

/// The small pictures in front of a row of a [`ListStyle`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ListIcon {
    Person,
    Mail,
    Phone,
    Pin,
    Warning,
}

impl ListIcon {
    pub const ALL: [ListIcon; 5] = [
        ListIcon::Person,
        ListIcon::Mail,
        ListIcon::Phone,
        ListIcon::Pin,
        ListIcon::Warning,
    ];

    pub fn key(self) -> &'static str {
        match self {
            ListIcon::Person => "person",
            ListIcon::Mail => "mail",
            ListIcon::Phone => "phone",
            ListIcon::Pin => "pin",
            ListIcon::Warning => "warning",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ListIcon::Person => "Person",
            ListIcon::Mail => "Mail",
            ListIcon::Phone => "Phone",
            ListIcon::Pin => "Address",
            ListIcon::Warning => "Diets",
        }
    }

    pub fn from_key(key: &str) -> Option<ListIcon> {
        ListIcon::ALL.into_iter().find(|icon| icon.key() == key)
    }

    /// The key of the built-in graphic.
    pub fn graphic(self, tint: IconTint) -> String {
        format!("icon_{}{}", self.key(), tint.suffix())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IconTint {
    Orange,
    Dark,
    Light,
}

impl IconTint {
    pub const ALL: [IconTint; 3] = [IconTint::Orange, IconTint::Dark, IconTint::Light];

    pub fn suffix(self) -> &'static str {
        match self {
            IconTint::Orange => "",
            IconTint::Dark => "_dark",
            IconTint::Light => "_light",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            IconTint::Orange => "orange",
            IconTint::Dark => "dark",
            IconTint::Light => "light",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            IconTint::Orange => "Orange",
            IconTint::Dark => "Dark",
            IconTint::Light => "White",
        }
    }

    pub fn from_key(key: &str) -> Option<IconTint> {
        IconTint::ALL.into_iter().find(|tint| tint.key() == key)
    }
}

/// Where the values of a [`ListStyle`] come from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ListSource {
    /// The rows are shown once, with the usual placeholders (`{team_phone}`, ...).
    #[default]
    Fixed,
    /// The rows are repeated for every guest team, with the placeholders of the guest:
    /// [`GUEST_ITEM_KEYS`].
    Guests,
}

/// The placeholders of a list that is repeated for every guest.
pub const GUEST_ITEM_KEYS: [(&str, &str); 7] = [
    ("name", "Name of the guest"),
    (
        "name_persons",
        "Name with the number of persons, for example Wok & Roll (3)",
    ),
    ("persons", "Number of persons"),
    ("phone", "Phone"),
    ("diets", "Diets / allergies"),
    (
        "info",
        "Persons, phone and diets in one line (what is missing is left out)",
    ),
    ("n", "Number of the guest (1, 2, ...)"),
];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ListRow {
    pub icon: Option<ListIcon>,
    /// May contain `{placeholders}`. A row whose placeholders are all empty is left out.
    pub text: String,
    pub bold: bool,
}

impl ListRow {
    pub fn new(icon: Option<ListIcon>, text: &str, bold: bool) -> Self {
        ListRow {
            icon,
            text: text.to_string(),
            bold,
        }
    }
}

/// The list element of an earlier version. Designs that still have one are turned into text boxes
/// when they are loaded ([`ListStyle::to_text`]); nothing creates lists any more.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ListStyle {
    pub source: ListSource,
    pub rows: Vec<ListRow>,
    pub font: Font,
    /// In percent of the canvas width.
    pub size: f32,
    pub color: String,
    pub icon_tint: IconTint,
    pub line_height: f32,
    /// Space between the rows of one entry, in `em`.
    pub row_gap: f32,
    /// Space between two entries (guests), in `em`.
    pub item_gap: f32,
    /// The rows after the first start further right (like the contact lines below a name).
    pub indent: bool,
    /// Make the text smaller if it does not fit into the box (many guests).
    #[serde(default = "default_true")]
    pub shrink: bool,
    /// The entries (guests) are put into up to this many columns if they do not fit into one.
    #[serde(default = "default_one")]
    pub max_columns: u8,
}

fn default_one() -> u8 {
    1
}

impl ListStyle {
    /// The same content as a text box: `{icon:...}` in front of the rows, `**bold**`, and a
    /// `{#guests}` block for a list of guests.
    pub fn to_text(&self) -> TextStyle {
        let has_icons = self.rows.iter().any(|row| row.icon.is_some());
        let lines: Vec<String> = self
            .rows
            .iter()
            .map(|row| {
                let prefix = match row.icon {
                    Some(icon) => format!("{{icon:{}}} ", icon.key()),
                    None if has_icons => "{icon:none} ".to_string(),
                    None => String::new(),
                };
                let text = if row.bold {
                    format!("**{}**", row.text)
                } else {
                    row.text.clone()
                };
                format!("{prefix}{text}")
            })
            .collect();
        let mut text = lines.join("\n");
        if self.source == ListSource::Guests {
            text = format!("{GUESTS_OPEN}\n{text}\n{GUESTS_CLOSE}");
        }
        TextStyle {
            text,
            font: self.font,
            size: self.size,
            color: self.color.clone(),
            bold: false,
            italic: false,
            uppercase: false,
            align: Align::Left,
            valign: VAlign::Top,
            line_height: self.line_height,
            letter_spacing: 0.0,
            missing: Missing::Hide,
            icon_tint: self.icon_tint,
            shrink: self.shrink,
            block_gap: self.item_gap,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ElementKind {
    Text(TextStyle),
    Image(ImageStyle),
    Shape(ShapeStyle),
    List(ListStyle),
}

impl ElementKind {
    pub fn label(&self) -> &'static str {
        match self {
            ElementKind::Text(_) => "Text",
            ElementKind::Image(_) => "Image",
            ElementKind::Shape(_) => "Shape",
            ElementKind::List(_) => "List",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Element {
    pub id: Uuid,
    pub name: String,
    /// Left edge / width in percent of the canvas width.
    pub x: f32,
    pub w: f32,
    /// Top edge / height in percent of the canvas height.
    pub y: f32,
    pub h: f32,
    /// Degrees.
    pub rotation: f32,
    /// 0..=1
    pub opacity: f32,
    pub visible: bool,
    /// The key of a placeholder (without braces): the element is only shown for teams for which
    /// this placeholder has a value. For example an icon next to `{team_diets}`.
    #[serde(default)]
    pub show_if: Option<String>,
    pub kind: ElementKind,
}

impl Element {
    fn base(name: &str, x: f32, y: f32, w: f32, h: f32, kind: ElementKind) -> Self {
        Element {
            id: Uuid::new_v4(),
            name: name.to_string(),
            x,
            y,
            w,
            h,
            rotation: 0.0,
            opacity: 1.0,
            visible: true,
            show_if: None,
            kind,
        }
    }

    pub fn text(name: &str, text: &str, x: f32, y: f32, w: f32, h: f32, size: f32) -> Self {
        Self::base(
            name,
            x,
            y,
            w,
            h,
            ElementKind::Text(TextStyle {
                text: text.to_string(),
                font: Font::Inter,
                size,
                color: "#222222".to_string(),
                bold: false,
                italic: false,
                uppercase: false,
                align: Align::Left,
                valign: VAlign::Top,
                line_height: 1.25,
                letter_spacing: 0.0,
                missing: Missing::Hide,
                icon_tint: IconTint::Orange,
                shrink: false,
                block_gap: 0.6,
            }),
        )
    }

    pub fn shape(name: &str, fill: &str, x: f32, y: f32, w: f32, h: f32, radius: f32) -> Self {
        Self::base(
            name,
            x,
            y,
            w,
            h,
            ElementKind::Shape(ShapeStyle {
                fill: fill.to_string(),
                radius,
                border_color: "#000000".to_string(),
                border_width: 0.0,
            }),
        )
    }

    pub fn image(name: &str, image: Option<Uuid>, x: f32, y: f32, w: f32, h: f32) -> Self {
        Self::base(
            name,
            x,
            y,
            w,
            h,
            ElementKind::Image(ImageStyle {
                image,
                fit: Fit::Cover,
                radius: 0.0,
            }),
        )
    }

    // Builder helpers for the presets.
    fn font(mut self, font: Font) -> Self {
        if let ElementKind::Text(style) = &mut self.kind {
            style.font = font;
        }
        self
    }
    fn color(mut self, color: &str) -> Self {
        if let ElementKind::Text(style) = &mut self.kind {
            style.color = color.to_string();
        }
        self
    }
    fn tint(mut self, tint: IconTint) -> Self {
        if let ElementKind::Text(style) = &mut self.kind {
            style.icon_tint = tint;
        }
        self
    }
    fn shrink(mut self) -> Self {
        if let ElementKind::Text(style) = &mut self.kind {
            style.shrink = true;
        }
        self
    }
    fn block_gap(mut self, gap: f32) -> Self {
        if let ElementKind::Text(style) = &mut self.kind {
            style.block_gap = gap;
        }
        self
    }
    fn bold(mut self) -> Self {
        if let ElementKind::Text(style) = &mut self.kind {
            style.bold = true;
        }
        self
    }
    fn align(mut self, align: Align) -> Self {
        if let ElementKind::Text(style) = &mut self.kind {
            style.align = align;
        }
        self
    }
    fn valign(mut self, valign: VAlign) -> Self {
        if let ElementKind::Text(style) = &mut self.kind {
            style.valign = valign;
        }
        self
    }
    fn upper(mut self) -> Self {
        if let ElementKind::Text(style) = &mut self.kind {
            style.uppercase = true;
        }
        self
    }
    fn line(mut self, line_height: f32) -> Self {
        if let ElementKind::Text(style) = &mut self.kind {
            style.line_height = line_height;
        }
        self
    }
    fn spacing(mut self, spacing: f32) -> Self {
        if let ElementKind::Text(style) = &mut self.kind {
            style.letter_spacing = spacing;
        }
        self
    }
    fn rotated(mut self, degrees: f32) -> Self {
        self.rotation = degrees;
        self
    }
    fn border(mut self, color: &str, width: f32) -> Self {
        if let ElementKind::Shape(style) = &mut self.kind {
            style.border_color = color.to_string();
            style.border_width = width;
        }
        self
    }
}

/// What the backend knows about an uploaded picture (answer of the upload, or part of the
/// design when it is loaded). Never stored in the browser or in an exported file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteImage {
    pub content_type: String,
    pub width: u32,
    pub height: u32,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DesignImage {
    pub id: Uuid,
    pub name: String,
    /// What goes into `<img src>`: `builtin:<key>` for a graphic of the project, otherwise
    /// `data:image/...;base64,...` (already scaled down in the browser) or, for a picture that was
    /// loaded from the cloud, a `blob:` URL (empty while it is not loaded).
    pub data_url: String,
    /// `Some` if the picture is stored in the cloud project. Then it does not have to be uploaded
    /// again.
    #[serde(skip)]
    pub remote: Option<RemoteImage>,
}

impl DesignImage {
    pub fn new(id: Uuid, name: impl Into<String>, data_url: impl Into<String>) -> Self {
        DesignImage {
            id,
            name: name.into(),
            data_url: data_url.into(),
            remote: None,
        }
    }

    /// The key of the built-in graphic, if this is one.
    pub fn builtin_key(&self) -> Option<&str> {
        builtin_key(&self.data_url)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScheduleDesign {
    #[serde(default)]
    pub version: u32,
    pub name: String,
    /// Height divided by width of the canvas.
    pub ratio: f32,
    pub background_color: String,
    #[serde(default)]
    pub background_image: Option<Uuid>,
    /// Bottom to top.
    pub elements: Vec<Element>,
    #[serde(default)]
    pub images: Vec<DesignImage>,
}

impl ScheduleDesign {
    pub fn blank(name: &str) -> Self {
        ScheduleDesign {
            version: DESIGN_VERSION,
            name: name.to_string(),
            ratio: 1.25,
            background_color: "#ffffff".to_string(),
            background_image: None,
            elements: Vec::new(),
            images: Vec::new(),
        }
    }

    pub fn image(&self, id: Uuid) -> Option<&DesignImage> {
        self.images.iter().find(|image| image.id == id)
    }

    pub fn element(&self, id: Uuid) -> Option<&Element> {
        self.elements.iter().find(|element| element.id == id)
    }

    pub fn element_mut(&mut self, id: Uuid) -> Option<&mut Element> {
        self.elements.iter_mut().find(|element| element.id == id)
    }

    /// Moves an element one step up (`towards_top`) or down in the stacking order.
    pub fn move_element(&mut self, id: Uuid, towards_top: bool) {
        let Some(index) = self.elements.iter().position(|element| element.id == id) else {
            return;
        };
        let target = if towards_top {
            index + 1
        } else {
            index.wrapping_sub(1)
        };
        if target < self.elements.len() {
            self.elements.swap(index, target);
        }
    }

    /// The id of the built-in graphic `key` in this design (it is added the first time).
    pub fn builtin_image(&mut self, key: &str) -> Uuid {
        let url = format!("{BUILTIN_PREFIX}{key}");
        if let Some(image) = self.images.iter().find(|image| image.data_url == url) {
            return image.id;
        }
        let id = Uuid::new_v4();
        let name = builtin_graphic(key)
            .map_or(key, |graphic| graphic.label)
            .to_string();
        self.images.push(DesignImage::new(id, name, url));
        id
    }

    /// Pictures uploaded by the user (without the built-in graphics).
    pub fn uploaded_images(&self) -> impl Iterator<Item = &DesignImage> {
        self.images
            .iter()
            .filter(|image| builtin_key(&image.data_url).is_none())
    }

    /// A built-in graphic as an element. `aspect` is height divided by width of the graphic, `w`
    /// the width of the element in percent of the sheet; the height follows, so that the box has
    /// the proportions of the graphic.
    pub fn graphic(&mut self, key: &str, x: f32, y: f32, w: f32, aspect: f32) -> Element {
        let id = self.builtin_image(key);
        let label = builtin_graphic(key).map_or(key, |graphic| graphic.label);
        let h = w * aspect / self.ratio;
        let mut element = Element::image(label, Some(id), x, y, w, h);
        if let ElementKind::Image(style) = &mut element.kind {
            style.fit = Fit::Contain;
        }
        element
    }

    pub fn remove_image(&mut self, id: Uuid) {
        self.images.retain(|image| image.id != id);
        if self.background_image == Some(id) {
            self.background_image = None;
        }
        for element in &mut self.elements {
            if let ElementKind::Image(style) = &mut element.kind {
                if style.image == Some(id) {
                    style.image = None;
                }
            }
        }
    }

    /// Gives a picture a new id and rewrites every use of it (the backend refused the old id
    /// because it is taken by a different picture).
    pub fn reassign_image_id(&mut self, old: Uuid, new: Uuid) {
        for image in &mut self.images {
            if image.id == old {
                image.id = new;
            }
        }
        if self.background_image == Some(old) {
            self.background_image = Some(new);
        }
        for element in &mut self.elements {
            if let ElementKind::Image(style) = &mut element.kind {
                if style.image == Some(old) {
                    style.image = Some(new);
                }
            }
        }
    }

    /// Rough size of the stored design in bytes (what the browser storage has to hold).
    pub fn approx_bytes(&self) -> usize {
        self.images
            .iter()
            .map(|image| match &image.remote {
                Some(remote) => remote.bytes as usize,
                None => image.data_url.len(),
            })
            .sum::<usize>()
            + self.elements.len() * 600
            + 400
    }

    /// Makes an imported or stored design safe to render and acceptable for the backend: clamps
    /// numbers, drops images that are not plain raster images and colors that are not plain
    /// colors, cleans texts and names, limits sizes and makes ids unique.
    pub fn sanitize(&mut self) {
        self.version = DESIGN_VERSION;
        self.name = clean_line(&self.name).chars().take(MAX_NAME_LEN).collect();
        if self.name.trim().is_empty() {
            self.name = "Design".to_string();
        }
        self.ratio = clamp(self.ratio, 0.3, 3.0, 1.25);
        self.background_color = safe_color(&self.background_color, "#ffffff");
        self.images.retain(|image| {
            image.remote.is_some()
                || is_safe_data_url(&image.data_url)
                || builtin_key(&image.data_url).is_some()
        });
        let mut seen_images = HashSet::new();
        self.images.retain(|image| seen_images.insert(image.id));
        self.images.truncate(MAX_ALL_IMAGES);
        for image in &mut self.images {
            image.name = clean_line(&image.name).chars().take(MAX_NAME_LEN).collect();
        }
        if let Some(id) = self.background_image {
            if self.image(id).is_none() {
                self.background_image = None;
            }
        }
        self.elements.truncate(MAX_ELEMENTS);
        let known: Vec<Uuid> = self.images.iter().map(|image| image.id).collect();
        let mut seen_elements = HashSet::new();
        for element in &mut self.elements {
            if !seen_elements.insert(element.id) {
                element.id = Uuid::new_v4();
                seen_elements.insert(element.id);
            }
            // Designs of the earlier version have lists: they are text boxes now.
            if let ElementKind::List(list) = &element.kind {
                element.kind = ElementKind::Text(list.to_text());
            }
            element.name = clean_line(&element.name).chars().take(MAX_NAME_LEN).collect();
            element.x = clamp(element.x, -100.0, 200.0, 0.0);
            element.y = clamp(element.y, -100.0, 200.0, 0.0);
            element.w = clamp(element.w, 0.1, 300.0, 20.0);
            element.h = clamp(element.h, 0.1, 300.0, 10.0);
            element.rotation = clamp(element.rotation, -360.0, 360.0, 0.0);
            element.opacity = clamp(element.opacity, 0.0, 1.0, 1.0);
            element.show_if = element
                .show_if
                .take()
                .map(|key| key.trim().trim_matches(['{', '}']).to_string())
                .filter(|key| is_key(key));
            match &mut element.kind {
                ElementKind::Text(style) => {
                    style.text = clean_multiline(&migrate_event_placeholders(&style.text))
                        .chars()
                        .take(MAX_TEXT_LEN)
                        .collect();
                    style.size = clamp(style.size, 0.5, 60.0, 3.0);
                    style.color = safe_color(&style.color, "#222222");
                    style.line_height = clamp(style.line_height, 0.7, 3.0, 1.25);
                    style.letter_spacing = clamp(style.letter_spacing, -10.0, 50.0, 0.0);
                    style.block_gap = clamp(style.block_gap, 0.0, 5.0, 0.6);
                    if let Missing::Text(replacement) = &mut style.missing {
                        *replacement = clean_line(replacement)
                            .chars()
                            .take(MAX_MISSING_LEN)
                            .collect();
                    }
                }
                ElementKind::List(_) => {}
                ElementKind::Image(style) => {
                    style.radius = clamp(style.radius, 0.0, 50.0, 0.0);
                    if style.image.is_some_and(|id| !known.contains(&id)) {
                        style.image = None;
                    }
                }
                ElementKind::Shape(style) => {
                    style.fill = safe_color(&style.fill, "#dddddd");
                    style.border_color = safe_color(&style.border_color, "#000000");
                    style.radius = clamp(style.radius, 0.0, 50.0, 0.0);
                    style.border_width = clamp(style.border_width, 0.0, 20.0, 0.0);
                }
            }
        }
    }

    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|e| e.to_string())
    }

    pub fn from_json(json: &str) -> Result<Self, String> {
        let mut design: ScheduleDesign = serde_json::from_str(json)
            .map_err(|e| format!("This is not a valid design file: {e}"))?;
        design.sanitize();
        Ok(design)
    }
}

fn clamp(value: f32, min: f32, max: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        fallback
    }
}

/// The colors the backend accepts, in the form it accepts them: `#rgb`, `#rgba`, `#rrggbb`,
/// `#rrggbbaa`, `rgb(r, g, b)`, `rgba(r, g, b, a)` and `transparent`, all in lower case and at most
/// 40 characters. Returns `None` for everything else (names, `url(`, `var(`, `;`, ...); a few
/// plain color names of older designs are turned into hex.
pub fn normalize_color(color: &str) -> Option<String> {
    let color = color.trim().to_ascii_lowercase();
    if color.is_empty() || color.len() > 40 {
        return None;
    }
    if color == "transparent" {
        return Some(color);
    }
    if let Some(hex) = color.strip_prefix('#') {
        let valid = matches!(hex.len(), 3 | 4 | 6 | 8) && hex.chars().all(|c| c.is_ascii_hexdigit());
        return valid.then_some(color);
    }
    let channels = |parts: &[&str]| -> Option<Vec<u8>> {
        parts
            .iter()
            .map(|part| {
                let digits = !part.is_empty() && part.len() <= 3 && part.chars().all(|c| c.is_ascii_digit());
                digits.then(|| part.parse::<u16>().ok()).flatten().filter(|v| *v <= 255).map(|v| v as u8)
            })
            .collect()
    };
    if let Some(args) = color.strip_prefix("rgba(").and_then(|rest| rest.strip_suffix(')')) {
        let parts: Vec<&str> = args.split(',').map(str::trim).collect();
        if parts.len() != 4 {
            return None;
        }
        let rgb = channels(&parts[..3])?;
        let alpha = normalize_alpha(parts[3])?;
        return Some(format!("rgba({}, {}, {}, {alpha})", rgb[0], rgb[1], rgb[2]));
    }
    if let Some(args) = color.strip_prefix("rgb(").and_then(|rest| rest.strip_suffix(')')) {
        let parts: Vec<&str> = args.split(',').map(str::trim).collect();
        if parts.len() != 3 {
            return None;
        }
        let rgb = channels(&parts)?;
        return Some(format!("rgb({}, {}, {})", rgb[0], rgb[1], rgb[2]));
    }
    named_color(&color).map(str::to_string)
}

/// `0`..`1` with a digit in front of the point (`.5` becomes `0.5`).
fn normalize_alpha(alpha: &str) -> Option<String> {
    let alpha = if alpha.starts_with('.') { format!("0{alpha}") } else { alpha.to_string() };
    let (whole, fraction) = match alpha.split_once('.') {
        Some((whole, fraction)) => (whole, Some(fraction)),
        None => (alpha.as_str(), None),
    };
    if !matches!(whole, "0" | "1") || alpha.len() > 12 {
        return None;
    }
    if let Some(fraction) = fraction {
        if fraction.is_empty() || !fraction.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        if whole == "1" && fraction.chars().any(|c| c != '0') {
            return None;
        }
    }
    Some(alpha)
}

fn named_color(name: &str) -> Option<&'static str> {
    Some(match name {
        "white" => "#ffffff",
        "black" => "#000000",
        "red" => "#ff0000",
        "green" => "#008000",
        "blue" => "#0000ff",
        "yellow" => "#ffff00",
        "orange" => "#ffa500",
        "gray" | "grey" => "#808080",
        "pink" => "#ffc0cb",
        "purple" => "#800080",
        "brown" => "#a52a2a",
        _ => return None,
    })
}

/// `true` if the backend (and the sheet) would take this color, in any capitalization.
pub fn is_safe_color(color: &str) -> bool {
    normalize_color(color).is_some()
}

/// The color in the form the backend accepts, or `fallback`.
pub fn safe_color(color: &str, fallback: &str) -> String {
    normalize_color(color).unwrap_or_else(|| fallback.to_string())
}

/// Only plain raster images, base64 encoded (no SVG: it could carry scripts). The backend takes
/// PNG, JPEG and WebP only, so GIF is not accepted either.
pub fn is_safe_data_url(url: &str) -> bool {
    [
        "data:image/jpeg;base64,",
        "data:image/png;base64,",
        "data:image/webp;base64,",
    ]
    .iter()
    .find_map(|prefix| url.strip_prefix(prefix))
    .is_some_and(|rest| {
        !rest.is_empty()
            && rest
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "+/=".contains(c))
    })
}

/// Bidi control characters (U+202A..U+202E, U+2066..U+2069): the backend refuses them.
fn is_bidi_control(c: char) -> bool {
    matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
}

/// A text in the form the backend accepts: `\r\n` and `\r` become `\n`, tabs become spaces, every
/// other control character and every bidi control character is removed.
pub fn clean_multiline(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .chars()
        .filter_map(|c| match c {
            '\n' => Some('\n'),
            '\t' => Some(' '),
            c if c.is_control() || is_bidi_control(c) => None,
            c => Some(c),
        })
        .collect()
}

/// A name in the form the backend accepts: like [`clean_multiline`], but in one line.
pub fn clean_line(text: &str) -> String {
    clean_multiline(text).replace('\n', " ")
}

/// The plan title, description and date are no placeholders any more (the backend stores no plan
/// texts). Designs that still use them keep their layout: the title becomes plain text, the
/// description and the date are left out.
fn migrate_event_placeholders(text: &str) -> String {
    if !text.contains("{event_") {
        return text.to_string();
    }
    text.replace("{event_title}", DEFAULT_TITLE)
        .replace("{event_description}", "")
        .replace("{event_date}", "")
}

// ─────────────────────────────────────────────
//  Library of designs of one project
// ─────────────────────────────────────────────

/// All designs of a project plus the one that is used for the schedule sheets.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DesignLibrary {
    pub designs: Vec<ScheduleDesign>,
    pub active: usize,
}

impl DesignLibrary {
    pub fn active_design(&self) -> Option<&ScheduleDesign> {
        self.designs.get(self.active)
    }

    pub fn sanitize(&mut self) {
        self.designs.truncate(MAX_DESIGNS);
        for design in &mut self.designs {
            design.sanitize();
        }
        if self.active >= self.designs.len() {
            self.active = 0;
        }
    }
}

// ─────────────────────────────────────────────
//  Placeholders
// ─────────────────────────────────────────────

/// Replaces every `{key}` for which `resolve` returns a value. Unknown keys stay as they are, so
/// that a typo is visible in the preview.
pub fn fill(template: &str, resolve: &dyn Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find('}') {
            Some(close) if is_key(&after[..close]) => {
                let key = &after[..close];
                match resolve(key) {
                    Some(value) => out.push_str(&value),
                    None => {
                        out.push('{');
                        out.push_str(key);
                        out.push('}');
                    }
                }
                rest = &after[close + 1..];
            }
            _ => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Like [`fill`], but a line that contains placeholders and in which every placeholder is empty is
/// left out completely, including its fixed words ("Diets: {team_diets}" disappears if there are
/// no diets). Lines without any placeholder stay, also empty ones.
pub fn fill_collapsing(template: &str, resolve: &dyn Fn(&str) -> Option<String>) -> String {
    template
        .split('\n')
        .filter(|line| !is_empty_line(line, resolve))
        .map(|line| fill(line, resolve))
        .collect::<Vec<_>>()
        .join("\n")
}

/// `true` if the line has placeholders and all of them are known and empty.
pub fn is_empty_line(line: &str, resolve: &dyn Fn(&str) -> Option<String>) -> bool {
    let keys = placeholders_in(line);
    !keys.is_empty()
        && keys
            .iter()
            .all(|key| resolve(key).is_some_and(|value| value.trim().is_empty()))
}

fn is_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 40
        && key
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// All `{keys}` of a text, in order, without duplicates.
pub fn placeholders_in(template: &str) -> Vec<String> {
    let mut keys: Vec<String> = Vec::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        match after.find('}') {
            Some(close) if is_key(&after[..close]) => {
                let key = after[..close].to_string();
                if !keys.contains(&key) {
                    keys.push(key);
                }
                rest = &after[close + 1..];
            }
            _ => rest = after,
        }
    }
    keys
}

pub struct PlaceholderGroup {
    pub title: String,
    /// `(key, what it is replaced with)`
    pub items: Vec<(String, String)>,
}

/// Every placeholder the sheet knows, for the palette of the designer.
pub fn placeholder_catalog(courses: usize, max_guests: usize) -> Vec<PlaceholderGroup> {
    let item = |key: &str, text: &str| (key.to_string(), text.to_string());
    let mut groups = vec![
        PlaceholderGroup {
            title: "This team".into(),
            items: vec![
                item("team_name", "Name of the team"),
                item("team_address", "Address of the team"),
                item("team_phone", "Phone"),
                item("team_mail", "E-mail"),
                item("team_persons", "Number of persons"),
                item("team_diets", "Diets / allergies"),
            ],
        },
        PlaceholderGroup {
            title: "Your hosting".into(),
            items: vec![
                item("cook_course", "Course this team cooks"),
                item("cook_time", "Time of that course"),
                item("guests", "All guests, one per line"),
                item(
                    "guests_detail",
                    "All guests with phone and diets, a block per guest",
                ),
                item("guest_names", "Names of the guests, separated by commas"),
                item("guest_count", "Number of guest teams"),
            ],
        },
    ];
    let mut per_guest = Vec::new();
    for n in 1..=max_guests {
        per_guest.push(item(
            &format!("guest{n}_name"),
            &format!("Name of guest {n}"),
        ));
        per_guest.push(item(
            &format!("guest{n}_phone"),
            &format!("Phone of guest {n}"),
        ));
        per_guest.push(item(
            &format!("guest{n}_persons"),
            &format!("Persons of guest {n}"),
        ));
        per_guest.push(item(
            &format!("guest{n}_diets"),
            &format!("Diets of guest {n}"),
        ));
    }
    groups.push(PlaceholderGroup {
        title: "Single guests".into(),
        items: per_guest,
    });

    let mut path = vec![item("path", "The whole walking path, one line per course")];
    for n in 1..=courses {
        path.push(item(
            &format!("course{n}_name"),
            &format!("Name of course {n}"),
        ));
        path.push(item(
            &format!("course{n}_time"),
            &format!("Time of course {n}"),
        ));
        path.push(item(
            &format!("course{n}_host"),
            &format!("Host of course {n}"),
        ));
        path.push(item(
            &format!("course{n}_address"),
            &format!("Address of course {n}"),
        ));
        path.push(item(
            &format!("course{n}_phone"),
            &format!("Phone of the host of course {n}"),
        ));
        path.push(item(
            &format!("course{n}_marker"),
            &format!("★ (with a space) if this team cooks course {n}"),
        ));
    }
    groups.push(PlaceholderGroup {
        title: "Walking path".into(),
        items: path,
    });

    groups.push(PlaceholderGroup {
        title: "Start and end".into(),
        items: vec![
            item("start_name", "Name of the start point"),
            item("start_time", "Time of the start point"),
            item("start_address", "Address of the start point"),
            item("end_name", "Name of the end point (after party)"),
            item("end_time", "Time of the end point"),
            item("end_address", "Address of the end point"),
        ],
    });
    groups
}

// ─────────────────────────────────────────────
//  Presets
// ─────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    PizzaParty,
    Sunny,
    LineArt,
}

impl Preset {
    pub const ALL: [Preset; 3] = [
        Preset::PizzaParty,
        Preset::Sunny,
        Preset::LineArt,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Preset::PizzaParty => "Pizza party (pink panel, cocktail)",
            Preset::Sunny => "Sunny (yellow, big title)",
            Preset::LineArt => "Line art (black and white, spaghetti)",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Preset::PizzaParty => "pizza",
            Preset::Sunny => "sunny",
            Preset::LineArt => "lineart",
        }
    }

    pub fn from_key(key: &str) -> Option<Preset> {
        Preset::ALL.into_iter().find(|preset| preset.key() == key)
    }

    pub fn build(self) -> ScheduleDesign {
        let mut design = match self {
            Preset::PizzaParty => pizza_party(),
            Preset::Sunny => sunny(),
            Preset::LineArt => line_art(),
        };
        // The same form as every stored design (lower case colors, clean texts).
        design.sanitize();
        design
    }
}

/// Positions of the presets are written in units of the sheet width (the sheet is 100 wide and
/// 125 high); this turns them into the percentages of the model.
struct Sheet {
    design: ScheduleDesign,
}

impl Sheet {
    fn new(name: &str, background: &str) -> Self {
        let mut design = ScheduleDesign::blank(name);
        design.background_color = background.to_string();
        Sheet { design }
    }

    fn ratio(&self) -> f32 {
        self.design.ratio
    }

    fn text(&self, name: &str, text: &str, x: f32, y: f32, w: f32, h: f32, size: f32) -> Element {
        Element::text(name, text, x, y / self.ratio(), w, h / self.ratio(), size)
    }

    fn rect(&self, name: &str, fill: &str, x: f32, y: f32, w: f32, h: f32, radius: f32) -> Element {
        Element::shape(name, fill, x, y / self.ratio(), w, h / self.ratio(), radius)
    }

    /// A built-in graphic with the proportions `aspect` (height / width).
    fn pic(&mut self, key: &str, x: f32, y: f32, w: f32, aspect: f32) -> Element {
        self.design.graphic(key, x, y / self.ratio(), w, aspect)
    }

    fn add(&mut self, element: Element) {
        self.design.elements.push(element);
    }

    fn finish(self) -> ScheduleDesign {
        self.design
    }
}

fn pizza_party() -> ScheduleDesign {
    use crate::schedule_text::{guests_compact_snippet, host_snippet, team_snippet, without_icons};
    let mut s = Sheet::new("Pizza party", "#fbf1e3");
    let red = "#ff3b14";
    let ink = "#2a2a2a";
    s.add(
        s.text("Title", DEFAULT_TITLE, 6.0, 3.0, 66.0, 14.0, 10.5)
            .font(Font::Playfair)
            .color(red)
            .bold()
            .upper()
            .valign(VAlign::Middle),
    );
    s.add(
        s.text(
            "Team",
            "{team_name}",
            6.0,
            16.5,
            70.0,
            5.0,
            3.6,
        )
        .font(Font::Inter)
        .color(red),
    );
    let pizza = s.pic("pizza", 76.0, 1.5, 22.0, 0.874);
    s.add(pizza.rotated(14.0));
    s.add(s.rect("Panel", "#f6b6c6", 8.0, 24.0, 84.0, 89.0, 0.0));
    let cocktail = s.pic("cocktail", 1.5, 56.0, 22.0, 1.39);
    s.add(cocktail);
    for n in 1..=3 {
        let y = 26.5 + 17.0 * (n as f32 - 1.0);
        s.add(
            s.text(
                "Time",
                &format!("{{course{n}_time}}"),
                26.0,
                y,
                16.0,
                7.0,
                6.0,
            )
            .font(Font::Playfair)
            .color(ink)
            .bold(),
        );
        s.add(
            s.text(
                "Course",
                &format!("{{course{n}_marker}}{{course{n}_name}}"),
                43.0,
                y,
                47.0,
                5.0,
                3.9,
            )
            .font(Font::Playfair)
            .color(ink)
            .bold()
            .valign(VAlign::Middle),
        );
        s.add(
            s.text(
                "Host",
                &without_icons(&host_snippet(n)),
                43.0,
                y + 5.0,
                47.0,
                11.0,
                2.5,
            )
            .color(ink)
            .line(1.45)
            .shrink(),
        );
    }
    s.add(s.rect("Divider", ink, 26.0, 77.5, 64.0, 0.3, 0.0));
    s.add(
        s.text(
            "Hosting",
            "Du kochst: {cook_course} um {cook_time} Uhr",
            26.0,
            79.0,
            64.0,
            5.0,
            3.5,
        )
        .font(Font::Playfair)
        .color(ink)
        .bold()
        .valign(VAlign::Middle),
    );
    s.add(
        s.text("You", &team_snippet(true), 26.0, 85.0, 30.0, 27.0, 2.4)
            .color(ink)
            .line(1.4)
            .shrink(),
    );
    s.add(
        s.text(
            "Guests",
            &guests_compact_snippet(),
            58.0,
            85.0,
            33.0,
            27.0,
            2.5,
        )
        .color(ink)
        .line(1.4)
        .shrink()
        .block_gap(0.5),
    );
    s.add(
        s.text(
            "After party",
            "Danach: {end_name}\n{end_time} · {end_address}",
            6.0,
            115.5,
            88.0,
            8.0,
            3.0,
        )
        .font(Font::Inter)
        .color(red)
        .bold()
        .align(Align::Center)
        .valign(VAlign::Middle)
        .line(1.4),
    );
    s.finish()
}

fn sunny() -> ScheduleDesign {
    use crate::schedule_text::{guests_compact_snippet, host_snippet, team_snippet, without_icons};
    let mut s = Sheet::new("Sunny", "#f0cf6e");
    let ink = "#1a1a1a";
    let plum = "#5a2a3c";
    let left = s.pic("burst_left", -1.0, -1.0, 24.0, 1.05);
    s.add(left);
    let right = s.pic("burst_right", 77.0, -1.0, 24.0, 1.05);
    s.add(right);
    s.add(
        s.text("Title", DEFAULT_TITLE, 16.0, 3.0, 68.0, 19.0, 11.5)
            .font(Font::Chewy)
            .color(ink)
            .align(Align::Center)
            .valign(VAlign::Middle)
            .rotated(-3.0),
    );
    s.add(
        s.text(
            "Subtitle",
            "Laufzettel für {team_name}\nDu kochst {cook_course} um {cook_time} Uhr",
            12.0,
            22.5,
            76.0,
            9.0,
            3.5,
        )
        .font(Font::Inter)
        .color(ink)
        .bold()
        .align(Align::Center)
        .valign(VAlign::Middle)
        .line(1.35),
    );
    s.add(
        s.text(
            "Meeting point",
            "Treffpunkt: {start_time} Uhr · {start_name}\n{start_address}",
            8.0,
            31.5,
            84.0,
            6.5,
            2.5,
        )
        .font(Font::Inter)
        .color(ink)
        .align(Align::Center),
    );
    for n in 1..=3 {
        let y = 38.5 + 15.3 * (n as f32 - 1.0);
        s.add(
            s.text(
                "Time",
                &format!("{{course{n}_time}} Uhr"),
                7.0,
                y,
                30.0,
                6.0,
                4.4,
            )
            .font(Font::Inter)
            .color(ink)
            .bold()
            .valign(VAlign::Middle),
        );
        s.add(
            s.text(
                "Course",
                &format!("{{course{n}_marker}}{{course{n}_name}}"),
                38.0,
                y,
                56.0,
                6.0,
                4.4,
            )
            .font(Font::Inter)
            .color(ink)
            .bold()
            .valign(VAlign::Middle),
        );
        s.add(
            s.text(
                "Host",
                &without_icons(&host_snippet(n)),
                38.0,
                y + 5.8,
                56.0,
                9.5,
                2.4,
            )
            .color(ink)
            .line(1.3)
            .shrink(),
        );
    }
    s.add(
        s.text(
            "After party",
            "ab {end_time} Uhr: {end_name}\n{end_address}",
            7.0,
            85.5,
            87.0,
            9.0,
            3.4,
        )
        .font(Font::Inter)
        .color(ink)
        .bold()
        .valign(VAlign::Middle)
        .line(1.25),
    );
    s.add(
        s.rect("Host card", "#ffffff", 5.0, 95.5, 90.0, 27.5, 3.0)
            .border(plum, 0.35),
    );
    s.add(
        s.text("You", &team_snippet(true), 8.0, 97.0, 28.0, 24.5, 2.4)
            .color(ink)
            .tint(IconTint::Dark)
            .line(1.3)
            .shrink(),
    );
    s.add(
        s.text(
            "Guests",
            &guests_compact_snippet(),
            40.0,
            97.0,
            34.0,
            24.5,
            2.4,
        )
        .color(ink)
        .tint(IconTint::Dark)
        .line(1.3)
        .shrink()
        .block_gap(0.5),
    );
    let hat = s.pic("chef_hat", 76.0, 101.0, 17.0, 0.857);
    s.add(hat);
    s.finish()
}

fn line_art() -> ScheduleDesign {
    use crate::schedule_text::{guests_compact_snippet, host_snippet, team_snippet, without_icons};
    let mut s = Sheet::new("Line art", "#f6f4f0");
    let ink = "#111111";
    let orange = "#d2541f";
    s.add(
        s.rect("Frame", "transparent", 2.0, 1.6, 96.0, 121.8, 0.0)
            .border(ink, 0.25),
    );
    s.add(
        s.text("Title", DEFAULT_TITLE, 6.0, 5.0, 88.0, 15.0, 12.0)
            .font(Font::ArchivoBlack)
            .color(ink)
            .valign(VAlign::Middle)
            .spacing(-3.0),
    );
    s.add(
        s.text("Team", "{team_name}", 40.0, 19.0, 54.0, 9.0, 5.8)
            .font(Font::Pacifico)
            .color(orange)
            .align(Align::Right)
            .valign(VAlign::Middle)
            .rotated(-5.0),
    );
    s.add(
        s.text("Subtitle", "Laufzettel", 6.0, 22.0, 34.0, 5.0, 3.4)
            .font(Font::ArchivoBlack)
            .color(ink)
            .valign(VAlign::Middle),
    );
    let spaghetti = s.pic("spaghetti", 31.0, 31.0, 38.0, 0.925);
    s.add(spaghetti);
    s.add(
        s.text("You", &team_snippet(true), 5.0, 33.0, 27.0, 36.0, 2.2)
            .color(ink)
            .tint(IconTint::Dark)
            .line(1.3)
            .shrink(),
    );
    s.add(
        s.text(
            "Guests",
            &guests_compact_snippet(),
            70.0,
            33.0,
            27.0,
            36.0,
            2.2,
        )
        .color(ink)
        .tint(IconTint::Dark)
        .line(1.3)
        .shrink()
        .block_gap(0.7),
    );
    s.add(
        s.text(
            "Hosting",
            "Du kochst {cook_course} um {cook_time} Uhr",
            6.0,
            72.0,
            88.0,
            5.0,
            3.2,
        )
        .font(Font::Inter)
        .color(ink)
        .bold()
        .align(Align::Center)
        .valign(VAlign::Middle),
    );
    s.add(s.rect("Line", ink, 6.0, 80.0, 88.0, 0.25, 0.0));
    for (i, n) in [1, 2, 3].into_iter().enumerate() {
        let x = 6.0 + 30.0 * i as f32;
        s.add(
            s.text(
                "Time",
                &format!("{{course{n}_marker}}{{course{n}_time}}"),
                x,
                82.0,
                28.0,
                8.0,
                7.5,
            )
            .font(Font::Bebas)
            .color(ink)
            .valign(VAlign::Middle),
        );
        s.add(
            s.text(
                "Course",
                &format!("{{course{n}_name}}"),
                x,
                90.0,
                28.0,
                4.0,
                3.0,
            )
            .font(Font::ArchivoBlack)
            .color(orange)
            .valign(VAlign::Middle),
        );
        s.add(
            s.text(
                "Host",
                &without_icons(&host_snippet(n)),
                x,
                94.5,
                28.0,
                14.0,
                2.2,
            )
            .color(ink)
            .line(1.3)
            .shrink(),
        );
    }
    s.add(
        s.text(
            "After party",
            "Danach: {end_name}\n{end_time} Uhr · {end_address}",
            6.0,
            113.0,
            88.0,
            8.0,
            2.7,
        )
        .font(Font::Inter)
        .color(ink)
        .bold()
        .align(Align::Center)
        .valign(VAlign::Middle)
        .line(1.35),
    );
    s.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_replaces_known_keys_and_keeps_unknown_ones() {
        let resolve = |key: &str| match key {
            "team_name" => Some("Pasta Power".to_string()),
            _ => None,
        };
        assert_eq!(
            fill("Hi {team_name}, {typo}! {", &resolve),
            "Hi Pasta Power, {typo}! {"
        );
        assert_eq!(
            fill("{team_name}{team_name}", &resolve),
            "Pasta PowerPasta Power"
        );
        assert_eq!(fill("no braces", &resolve), "no braces");
        assert_eq!(
            fill("{ team_name } {Team_Name}", &resolve),
            "{ team_name } {Team_Name}"
        );
    }

    #[test]
    fn placeholders_are_listed_once() {
        assert_eq!(placeholders_in("{a} x {b_2} {a} {C} {"), vec!["a", "b_2"]);
    }

    #[test]
    fn presets_survive_json_and_sanitizing_unchanged() {
        for preset in Preset::ALL {
            let design = preset.build();
            let again = ScheduleDesign::from_json(&design.to_json().unwrap()).unwrap();
            assert_eq!(design, again, "{}", design.name);
        }
    }

    #[test]
    fn colors_cannot_carry_other_css() {
        assert!(is_safe_color("#fff"));
        assert!(is_safe_color("#a1b2c3"));
        assert!(is_safe_color("rgba(0, 0, 0, 0.5)"));
        assert!(is_safe_color("transparent"));
        assert!(!is_safe_color("red; background:url(//evil)"));
        assert!(!is_safe_color("url(x)"));
        assert!(!is_safe_color("#12"));
        assert!(!is_safe_color(""));
    }

    #[test]
    fn only_raster_data_urls_are_accepted() {
        assert!(is_safe_data_url("data:image/png;base64,iVBORw0KGgo="));
        assert!(!is_safe_data_url("data:image/svg+xml;base64,PHN2Zz4="));
        assert!(!is_safe_data_url("https://example.com/a.png"));
        assert!(!is_safe_data_url("data:image/png;base64,\"onload=alert(1)"));
    }

    #[test]
    fn sanitize_cleans_an_imported_design() {
        let mut design = Preset::Sunny.build();
        design.ratio = f32::NAN;
        design.background_color = "red; x:y".into();
        design.images.push(DesignImage::new(Uuid::new_v4(), "x", "javascript:1"));
        design.elements[0].x = 99999.0;
        let dangling = Uuid::new_v4();
        design.background_image = Some(dangling);
        design.sanitize();
        assert_eq!(design.ratio, 1.25);
        assert_eq!(design.background_color, "#ffffff");
        assert!(
            design
                .images
                .iter()
                .all(|image| image.data_url != "javascript:1")
        );
        assert_eq!(design.elements[0].x, 200.0);
        assert_eq!(design.background_image, None);
    }

    #[test]
    fn removing_an_image_clears_every_use() {
        let mut design = ScheduleDesign::blank("t");
        let id = Uuid::new_v4();
        design.images.push(DesignImage::new(id, "a", "data:image/png;base64,AA=="));
        design.background_image = Some(id);
        design
            .elements
            .push(Element::image("img", Some(id), 0.0, 0.0, 10.0, 10.0));
        design.remove_image(id);
        assert!(design.images.is_empty() && design.background_image.is_none());
        assert!(matches!(&design.elements[0].kind, ElementKind::Image(s) if s.image.is_none()));
    }

    #[test]
    fn elements_can_be_reordered() {
        let mut design = Preset::LineArt.build();
        let first = design.elements[0].id;
        design.move_element(first, true);
        assert_eq!(design.elements[1].id, first);
        design.move_element(first, false);
        assert_eq!(design.elements[0].id, first);
        design.move_element(first, false); // already at the bottom
        assert_eq!(design.elements[0].id, first);
    }

    #[test]
    fn every_builtin_graphic_has_its_file_and_a_unique_key() {
        let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
        let mut keys = std::collections::HashSet::new();
        for graphic in BUILTIN_GRAPHICS {
            assert!(keys.insert(graphic.key), "duplicate key {}", graphic.key);
            assert!(
                assets.join(graphic.file).exists(),
                "missing file assets/{}",
                graphic.file
            );
        }
    }

    #[test]
    fn presets_use_only_known_graphics_and_stay_on_the_sheet() {
        for preset in Preset::ALL {
            let design = preset.build();
            for image in &design.images {
                assert!(
                    builtin_key(&image.data_url).is_some(),
                    "{}: unknown graphic {}",
                    design.name,
                    image.data_url
                );
            }
            for element in &design.elements {
                assert!(
                    element.x >= -5.0 && element.x + element.w <= 105.0,
                    "{}: {} sticks out horizontally",
                    design.name,
                    element.name
                );
                assert!(
                    element.y >= -5.0 && element.y + element.h <= 105.0,
                    "{}: {} sticks out vertically",
                    design.name,
                    element.name
                );
            }
        }
    }

    #[test]
    fn show_if_is_cleaned_up_on_import() {
        let mut design = ScheduleDesign::blank("t");
        let mut a = Element::text("a", "x", 0.0, 0.0, 10.0, 10.0, 3.0);
        a.show_if = Some(" {team_diets} ".into());
        let mut b = Element::text("b", "x", 0.0, 0.0, 10.0, 10.0, 3.0);
        b.show_if = Some("not a key!".into());
        design.elements = vec![a, b];
        design.sanitize();
        assert_eq!(design.elements[0].show_if.as_deref(), Some("team_diets"));
        assert_eq!(design.elements[1].show_if, None);
    }

    #[test]
    fn builtin_references_survive_sanitizing_but_fake_ones_do_not() {
        let mut design = ScheduleDesign::blank("t");
        let good = design.builtin_image("pot");
        design.images.push(DesignImage::new(Uuid::new_v4(), "x", "builtin:../../etc/passwd"));
        design.sanitize();
        assert_eq!(design.images.len(), 1);
        assert_eq!(design.images[0].id, good);
        assert_eq!(
            design.builtin_image("pot"),
            good,
            "the same graphic is added only once"
        );
    }

    fn all_text(design: &ScheduleDesign) -> String {
        design
            .elements
            .iter()
            .filter_map(|element| match &element.kind {
                ElementKind::Text(style) => Some(style.text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn every_preset_only_uses_known_placeholders() {
        let mut known: Vec<String> = placeholder_catalog(3, 5)
            .into_iter()
            .flat_map(|group| group.items.into_iter().map(|(key, _)| key))
            .collect();
        // Inside a `{#guests}` block the placeholders of the guest.
        known.extend(GUEST_ITEM_KEYS.iter().map(|(key, _)| key.to_string()));
        for preset in Preset::ALL {
            let design = preset.build();
            for key in placeholders_in(&all_text(&design)) {
                assert!(
                    known.contains(&key),
                    "{} uses unknown {{{key}}}",
                    design.name
                );
            }
        }
    }

    #[test]
    fn presets_show_the_whole_team_plan() {
        // Everything TeamPlanData carries must be reachable in every preset.
        for preset in Preset::ALL {
            let design = preset.build();
            let all = all_text(&design);
            for needed in [
                "{team_name}",
                "{team_persons}",
                "{team_phone}",
                "{team_address}",
                "{team_diets}",
                "{#guests}",
                "{course1_host}",
                "{course1_phone}",
                "{course1_address}",
                "{course3_address}",
                "{course1_time}",
                "{course2_name}",
            ] {
                assert!(
                    all.contains(needed),
                    "{} does not show {needed}",
                    design.name
                );
            }
            let detailed = ["{name_persons}", "{phone}", "{diets}"]
                .iter()
                .all(|key| all.contains(key));
            assert!(
                detailed || all.contains("{info}"),
                "{} does not show the guest details",
                design.name
            );
            assert!(
                all.contains("{name}") || all.contains("{name_persons}"),
                "{} does not show the guests",
                design.name
            );
        }
    }

    #[test]
    fn every_icon_in_a_preset_exists_in_every_tint() {
        for icon in ListIcon::ALL {
            for tint in IconTint::ALL {
                let key = icon.graphic(tint);
                assert!(builtin_graphic(&key).is_some(), "no graphic {key}");
            }
        }
    }

    #[test]
    fn a_list_of_the_earlier_version_becomes_a_text_box() {
        let json = r##"{
            "name": "old", "ratio": 1.25, "background_color": "#ffffff", "elements": [{
              "id": "00000000-0000-0000-0000-000000000001", "name": "Guests", "x": 1, "w": 40, "y": 1, "h": 20,
              "rotation": 0, "opacity": 1, "visible": true,
              "kind": { "type": "list", "source": "guests", "font": "gluten", "size": 2.7, "color": "#5b3f2e",
                "icon_tint": "dark", "line_height": 1.3, "row_gap": 0.2, "item_gap": 0.7, "indent": true,
                "shrink": true, "max_columns": 2,
                "rows": [
                  {"icon": "person", "text": "{name}", "bold": true},
                  {"icon": null, "text": "{persons} Personen", "bold": false},
                  {"icon": "phone", "text": "{phone}", "bold": false}
                ] } }] }"##;
        let design = ScheduleDesign::from_json(json).unwrap();
        let ElementKind::Text(style) = &design.elements[0].kind else {
            panic!("not a text box")
        };
        assert_eq!(
            style.text,
            "{#guests}\n{icon:person} **{name}**\n{icon:none} {persons} Personen\n{icon:phone} {phone}\n{/guests}"
        );
        assert_eq!(style.icon_tint, IconTint::Dark);
        assert!(style.shrink);
        assert_eq!(style.missing, Missing::Hide);
        assert_eq!(style.block_gap, 0.7);
    }

    #[test]
    fn text_boxes_of_the_earlier_version_still_load() {
        // The old `collapse` field is simply ignored; new fields get their defaults.
        let json = r##"{
            "name": "old", "ratio": 1.25, "background_color": "#ffffff", "elements": [{
              "id": "00000000-0000-0000-0000-000000000002", "name": "T", "x": 1, "w": 40, "y": 1, "h": 20,
              "rotation": 0, "opacity": 1, "visible": true,
              "kind": { "type": "text", "text": "{team_name}", "font": "inter", "size": 3, "color": "#111111",
                "bold": false, "italic": false, "uppercase": false, "align": "left", "valign": "top",
                "line_height": 1.2, "letter_spacing": 0, "collapse": false } }] }"##;
        let design = ScheduleDesign::from_json(json).unwrap();
        let ElementKind::Text(style) = &design.elements[0].kind else {
            panic!()
        };
        assert_eq!(style.missing, Missing::Hide);
        assert!(!style.shrink);
    }

    #[test]
    fn the_missing_text_survives_json_and_is_limited() {
        let mut design = ScheduleDesign::blank("t");
        let mut element = Element::text("t", "{team_phone}", 0.0, 0.0, 10.0, 10.0, 3.0);
        if let ElementKind::Text(style) = &mut element.kind {
            style.missing = Missing::Text("–".repeat(100));
        }
        design.elements.push(element);
        let again = ScheduleDesign::from_json(&design.to_json().unwrap()).unwrap();
        let ElementKind::Text(style) = &again.elements[0].kind else {
            panic!()
        };
        assert_eq!(style.missing, Missing::Text("–".repeat(40)));
    }

    #[test]
    fn colors_are_written_the_way_the_backend_wants_them() {
        assert_eq!(normalize_color("#A1B2C3").as_deref(), Some("#a1b2c3"));
        assert_eq!(normalize_color(" #FFF ").as_deref(), Some("#fff"));
        assert_eq!(normalize_color("#abcd").as_deref(), Some("#abcd"));
        assert_eq!(normalize_color("#abcdef12").as_deref(), Some("#abcdef12"));
        assert_eq!(normalize_color("Transparent").as_deref(), Some("transparent"));
        assert_eq!(normalize_color("RGB(1,2,3)").as_deref(), Some("rgb(1, 2, 3)"));
        assert_eq!(normalize_color("rgba(0, 0, 0, .5)").as_deref(), Some("rgba(0, 0, 0, 0.5)"));
        assert_eq!(normalize_color("rgba(0, 0, 0, 1)").as_deref(), Some("rgba(0, 0, 0, 1)"));
        assert_eq!(normalize_color("red").as_deref(), Some("#ff0000"));
        for bad in [
            "#12", "#12345", "#ggg", "rgb(256, 0, 0)", "rgb(1, 2)", "rgb(1, 2, 3, 4)", "rgba(0, 0, 0, 1.5)",
            "rgba(0, 0, 0, 2)", "rgba(0, 0, 0, 0.5.5)", "rgb(10%, 0, 0)", "hsl(0, 0%, 0%)", "var(--x)",
            "url(x)", "red; x:y", "a}", "chartreuse", "",
        ] {
            assert_eq!(normalize_color(bad), None, "{bad}");
        }
        assert_eq!(normalize_color(&format!("#{}", "a".repeat(40))), None);
    }

    #[test]
    fn texts_and_names_are_cleaned_for_the_backend() {
        assert_eq!(clean_multiline("a\r\nb\rc\td\u{0}e\u{202E}f\u{2066}g"), "a\nb\nc defg");
        assert_eq!(clean_line("one\ntwo\r\nthree"), "one two three");
        assert_eq!(clean_line("bad\u{7f}\u{85}name"), "badname");
    }

    #[test]
    fn sanitize_makes_a_design_acceptable_for_the_backend() {
        let mut design = ScheduleDesign::blank("  \r\n ");
        design.background_color = "#ABCDEF".into();
        let mut element = Element::text("a\nb", "x\r\ny", 0.0, 0.0, 10.0, 10.0, 3.0);
        let duplicate = element.clone();
        if let ElementKind::Text(style) = &mut element.kind {
            style.color = "RGBA(1, 2, 3, .25)".into();
            style.missing = Missing::Text("a\nb".into());
        }
        design.elements.push(element);
        design.elements.push(duplicate);
        design.sanitize();
        assert_eq!(design.name, "Design");
        assert_eq!(design.background_color, "#abcdef");
        assert_eq!(design.elements[0].name, "a b");
        assert_ne!(design.elements[0].id, design.elements[1].id, "ids are unique");
        match &design.elements[0].kind {
            ElementKind::Text(style) => {
                assert_eq!(style.text, "x\ny");
                assert_eq!(style.color, "rgba(1, 2, 3, 0.25)");
                assert_eq!(style.missing, Missing::Text("a b".into()));
            }
            _ => panic!("text expected"),
        }
    }

    #[test]
    fn duplicate_pictures_are_dropped_and_gifs_refused() {
        let mut design = ScheduleDesign::blank("x");
        let id = Uuid::new_v4();
        design.images.push(DesignImage::new(id, "a", "data:image/png;base64,AA=="));
        design.images.push(DesignImage::new(id, "b", "data:image/png;base64,AA=="));
        design.images.push(DesignImage::new(Uuid::new_v4(), "c", "data:image/gif;base64,AA=="));
        design.sanitize();
        assert_eq!(design.images.len(), 1);
        assert_eq!(design.images[0].name, "a");
        assert!(!is_safe_data_url("data:image/gif;base64,AA=="));
        assert!(is_safe_data_url("data:image/webp;base64,AA=="));
    }

    #[test]
    fn a_picture_that_lives_in_the_cloud_survives_sanitizing() {
        let mut design = ScheduleDesign::blank("x");
        let mut image = DesignImage::new(Uuid::new_v4(), "logo", "blob:https://app.example/1234");
        image.remote = Some(RemoteImage {
            content_type: "image/png".into(),
            width: 1,
            height: 1,
            bytes: 1,
            sha256: "ab".into(),
        });
        design.images.push(image);
        design.images.push(DesignImage::new(Uuid::new_v4(), "stray", "blob:https://app.example/9"));
        design.sanitize();
        assert_eq!(design.images.len(), 1, "a blob URL without a stored picture is dropped");
        assert_eq!(design.images[0].name, "logo");
    }

    #[test]
    fn the_plan_texts_are_no_placeholders_any_more() {
        let mut design = ScheduleDesign::blank("old");
        design.elements.push(Element::text(
            "t",
            "{event_title}\n{event_description}|{event_date}|{team_name}",
            0.0,
            0.0,
            10.0,
            10.0,
            3.0,
        ));
        design.sanitize();
        match &design.elements[0].kind {
            ElementKind::Text(style) => assert_eq!(style.text, "Cook & Run\n||{team_name}"),
            _ => panic!("text expected"),
        }
        for preset in Preset::ALL {
            let json = preset.build().to_json().unwrap();
            assert!(!json.contains("event_"), "{preset:?} still uses a plan text");
        }
        assert!(placeholder_catalog(3, 5).iter().flat_map(|group| &group.items).all(|(key, _)| !key.starts_with("event_")));
    }

    #[test]
    fn reassigning_a_picture_id_rewrites_every_use() {
        let mut design = ScheduleDesign::blank("x");
        let (old, new) = (Uuid::new_v4(), Uuid::new_v4());
        design.images.push(DesignImage::new(old, "a", "data:image/png;base64,AA=="));
        design.background_image = Some(old);
        design.elements.push(Element::image("i", Some(old), 0.0, 0.0, 10.0, 10.0));
        design.reassign_image_id(old, new);
        assert_eq!(design.images[0].id, new);
        assert_eq!(design.background_image, Some(new));
        match &design.elements[0].kind {
            ElementKind::Image(style) => assert_eq!(style.image, Some(new)),
            _ => panic!("image expected"),
        }
    }
}
