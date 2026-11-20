//! Renders a freely designed schedule sheet ([`ScheduleDesign`]) for one team: the placeholders of
//! the texts are replaced with the data of the [`Schedule`].
//!
//! The sheet is a box with a fixed aspect ratio; everything inside is positioned in percent and
//! sized in `cqw` (percent of the sheet width), so that the preview and the exported image look
//! the same at every size.

use dioxus::prelude::*;
use uuid::Uuid;

use super::builtin::image_src;
use super::{Course, MeetingPoint, Schedule, SheetInfo, Team};
use crate::schedule_design::{safe_color, Element as DesignElement, ElementKind, ScheduleDesign, TextStyle};
use crate::schedule_text::{bold_spans, expand, fit_scale, has_icon_column, FitBox, Line};

/// html-to-image 1.11.11 (MIT), served from our own assets (same file as the classic sheet).
const HTML_TO_IMAGE: Asset = asset!("/assets/html-to-image.js");

fn split_index(rest: &str) -> Option<(usize, &str)> {
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    let (number, field) = rest.split_at(digits);
    Some((number.parse().ok()?, field.strip_prefix('_')?))
}

fn persons_text(team: &Team, german: bool) -> String {
    match team.members {
        Some(members) => format!("{members} {}", if german { "Pers." } else { "persons" }),
        None => String::new(),
    }
}

fn guest_line(team: &Team, german: bool) -> String {
    let mut parts = vec![team.name.clone()];
    for part in [persons_text(team, german), team.phone.clone().unwrap_or_default(), team.diets.clone().unwrap_or_default()] {
        if !part.is_empty() {
            parts.push(part);
        }
    }
    parts.join(" · ")
}

impl Schedule {
    /// Index of the course this team cooks.
    fn cook_index(&self) -> Option<usize> {
        self.walking_path.iter().position(|(_, host)| host.name == self.host.name)
    }

    /// The value of `{key}`. `None` only for keys the sheet does not know at all; a known key
    /// without data (a fourth course, a guest that does not exist) gives an empty text.
    pub(crate) fn placeholder(&self, key: &str, config: &SheetInfo) -> Option<String> {
        let german = config.german();
        let text = |value: &Option<String>| value.clone().unwrap_or_default();
        let point = |point: &Option<MeetingPoint>, f: fn(&MeetingPoint) -> String| {
            point.as_ref().map(f).unwrap_or_default()
        };
        Some(match key {
            "team_name" => self.host.name.clone(),
            "team_address" => self.host.address.clone(),
            "team_phone" => text(&self.host.phone),
            "team_mail" => text(&self.host.mail),
            "team_persons" => self.host.members.map(|m| m.to_string()).unwrap_or_default(),
            "team_diets" => text(&self.host.diets),
            "cook_course" => self.cook_index().map(|i| self.walking_path[i].0.name.clone()).unwrap_or_default(),
            "cook_time" => self.cook_index().map(|i| self.walking_path[i].0.time.clone()).unwrap_or_default(),
            "guests" => self.guest_list.iter().map(|g| guest_line(g, german)).collect::<Vec<_>>().join("\n"),
            "guests_detail" => self
                .guest_list
                .iter()
                .map(|guest| {
                    let mut lines = vec![guest.name.clone()];
                    lines.extend(guest.phone.clone().filter(|phone| !phone.is_empty()));
                    lines.extend(guest.diets.clone().filter(|diets| !diets.is_empty()));
                    lines.join("\n")
                })
                .collect::<Vec<_>>()
                .join("\n\n"),
            "guest_names" => self.guest_list.iter().map(|g| g.name.clone()).collect::<Vec<_>>().join(", "),
            "guest_count" => self.guest_list.len().to_string(),
            "path" => self
                .walking_path
                .iter()
                .map(|(course, host)| {
                    let address = if host.address.is_empty() { String::new() } else { format!(", {}", host.address) };
                    format!("{}  {} – {}{}", course.time, course.name, host.name, address)
                })
                .collect::<Vec<_>>()
                .join("\n"),
            "start_name" => point(&self.start_point, |p| p.name.clone()),
            "start_time" => point(&self.start_point, |p| p.time.clone()),
            "start_address" => point(&self.start_point, |p| p.address.clone()),
            "end_name" => point(&self.end_point, |p| p.name.clone()),
            "end_time" => point(&self.end_point, |p| p.time.clone()),
            "end_address" => point(&self.end_point, |p| p.address.clone()),
            _ => return self.indexed_placeholder(key),
        })
    }

    fn indexed_placeholder(&self, key: &str) -> Option<String> {
        if let Some(rest) = key.strip_prefix("course") {
            let (number, field) = split_index(rest)?;
            let stop: Option<&(Course, Team)> = self.walking_path.get(number.wrapping_sub(1));
            return Some(match field {
                "name" => stop.map(|(c, _)| c.name.clone()),
                "time" => stop.map(|(c, _)| c.time.clone()),
                "host" => stop.map(|(_, h)| h.name.clone()),
                "address" => stop.map(|(_, h)| h.address.clone()),
                "phone" => stop.and_then(|(_, h)| h.phone.clone()),
                "marker" => stop.filter(|_| self.cook_index() == Some(number - 1)).map(|_| "★ ".to_string()),
                _ => return None,
            }
            .unwrap_or_default());
        }
        if let Some(rest) = key.strip_prefix("guest") {
            let (number, field) = split_index(rest)?;
            let guest = self.guest_list.get(number.wrapping_sub(1));
            return Some(match field {
                "name" => guest.map(|g| g.name.clone()),
                "phone" => guest.and_then(|g| g.phone.clone()),
                "persons" => guest.map(|g| g.members.map(|m| m.to_string()).unwrap_or_default()),
                "diets" => guest.and_then(|g| g.diets.clone()),
                _ => return None,
            }
            .unwrap_or_default());
        }
        None
    }

    /// Example data for the designer, so that a design can be made without a real plan.
    pub(crate) fn sample(german: bool) -> Schedule {
        let team = |name: &str, address: &str, members: u16, diets: &str| Team {
            name: name.to_string(),
            address: address.to_string(),
            mail: Some("team@example.org".to_string()),
            phone: Some("0170 1234567".to_string()),
            members: Some(members),
            diets: if diets.is_empty() { None } else { Some(diets.to_string()) },
        };
        let me = team("Pasta Power", if german { "Beispielweg 3" } else { "Example Road 3" }, 2, "");
        let road = if german { "straße" } else { " Street" };
        let starter = team("Die Gabelhelden", &format!("Lindenallee 12, Fulda{}", ""), 2, "");
        let dessert = team("Löffelgarde", &format!("Bahnhof{road} 7, Fulda"), 3, if german { "vegetarisch" } else { "vegetarian" });
        let course = |name: &str, time: &str| Course { name: name.to_string(), time: time.to_string() };
        let (c1, c2, c3) = if german {
            ("Vorspeise", "Hauptspeise", "Dessert")
        } else {
            ("Starter", "Main course", "Dessert")
        };
        let point = |name: &str, time: &str, address: &str| MeetingPoint {
            name: name.to_string(),
            time: time.to_string(),
            address: address.to_string(),
        };
        Schedule {
            host: me.clone(),
            guest_list: vec![
                team("Kochmützen", "", 2, if german { "ohne Nüsse" } else { "no nuts" }),
                team("Wok & Roll", "", 2, ""),
            ],
            walking_path: vec![
                (course(c1, "18:00"), starter),
                (course(c2, "19:30"), me),
                (course(c3, "21:00"), dessert),
            ],
            start_point: None,
            end_point: Some(point(
                if german { "After-Dinner-Party im Flame" } else { "After-dinner party at Flame" },
                "22:00",
                "Marktstraße 1, Fulda",
            )),
        }
    }
}

fn geometry(element: &DesignElement) -> String {
    format!(
        "position:absolute; left:{:.3}%; top:{:.3}%; width:{:.3}%; height:{:.3}%; transform:rotate({:.2}deg); opacity:{:.2};",
        element.x, element.y, element.w, element.h, element.rotation, element.opacity
    )
}

/// A press on an element of the sheet (designer): which one and where on the screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ElementPress {
    pub id: Uuid,
    pub x: f64,
    pub y: f64,
    pub shift: bool,
}

/// The sheet. In the designer the elements react to the pointer (`onelementdown`, `ondoubleclick`),
/// a press on the empty sheet calls `onselect(None)`, and `children` (selection frame, guides) are
/// drawn on top; everything in `children` must be marked `data-no-export`, so that it is not part
/// of the downloaded image.
#[component]
pub fn DesignedSchedule(
    design: ScheduleDesign,
    config: SheetInfo,
    schedule: Schedule,
    #[props(default)] selected: Option<Uuid>,
    #[props(default)] onselect: Option<EventHandler<Option<Uuid>>>,
    #[props(default)] onelementdown: Option<EventHandler<ElementPress>>,
    #[props(default)] ondoubleclick: Option<EventHandler<Uuid>>,
    /// `id` of the sheet element. Only the off-screen sheet of "download all" sets another one.
    #[props(default)] sheet_id: Option<String>,
    children: Element,
) -> Element {
    let sheet_id = sheet_id.unwrap_or_else(|| "section-to-print".to_string());
    let background = safe_color(&design.background_color, "#FFFFFF");
    let background_image = design.background_image.and_then(|id| design.image(id)).map(|image| image_src(&image.data_url));
    let interactive = onelementdown.is_some();
    let ratio = design.ratio;
    let canvas_style = format!(
        "aspect-ratio: 1 / {ratio:.4}; container-type: inline-size; background: {background};"
    );

    rsx! {
        document::Script { src: HTML_TO_IMAGE }
        div {
            id: "{sheet_id}",
            class: "relative w-full overflow-hidden select-none",
            style: "{canvas_style}",
            onpointerdown: move |_| {
                if let Some(handler) = onselect {
                    handler.call(None);
                }
            },
            if let Some(url) = background_image {
                img { class: "absolute inset-0 h-full w-full object-cover", src: "{url}", alt: "", draggable: "false" }
            }
            for element in design.elements.iter().filter(|element| {
                element.visible
                    && element.show_if.as_ref().is_none_or(|key| {
                        schedule.placeholder(key, &config).is_some_and(|value| !value.trim().is_empty())
                    })
            }) {
                {
                    let id = element.id;
                    let style = geometry(element);
                    let behaviour = match (interactive, selected == Some(id)) {
                        (false, _) => "",
                        (true, true) => "cursor:move; touch-action:none;",
                        (true, false) => "cursor:move;",
                    };
                    let image_url = match &element.kind {
                        ElementKind::Image(style) => {
                            style.image.and_then(|image| design.image(image)).map(|image| image_src(&image.data_url))
                        }
                        _ => None,
                    };
                    rsx! {
                        div {
                            key: "{id}",
                            style: "{style} {behaviour}",
                            onpointerdown: move |e| {
                                if let Some(handler) = onelementdown {
                                    e.stop_propagation();
                                    let point = e.client_coordinates();
                                    handler.call(ElementPress { id, x: point.x, y: point.y, shift: e.modifiers().shift() });
                                }
                            },
                            ondoubleclick: move |_| {
                                if let Some(handler) = ondoubleclick {
                                    handler.call(id);
                                }
                            },
                            ElementBody { element: element.clone(), image_url, ratio, config: config.clone(), schedule: schedule.clone() }
                        }
                    }
                }
            }
            {children}
        }
    }
}

/// The value of a guest-block placeholder (`{name}`, `{phone}`, ...), `None` for everything else
/// (which then resolves like in the rest of the text).
fn guest_value(guest: &Team, index: usize, key: &str, german: bool) -> Option<String> {
    Some(match key {
        "name" => guest.name.clone(),
        "name_persons" => match guest.members {
            Some(members) => format!("{} ({members})", guest.name),
            None => guest.name.clone(),
        },
        "persons" => guest.members.map(|m| m.to_string()).unwrap_or_default(),
        "phone" => guest.phone.clone().unwrap_or_default(),
        "diets" => guest.diets.clone().unwrap_or_default(),
        "info" => guest_info(guest, german),
        "n" => (index + 1).to_string(),
        _ => return None,
    })
}

/// Persons, phone and diets of a guest in one line; what is missing is left out.
fn guest_info(guest: &Team, german: bool) -> String {
    [persons_text(guest, german), guest.phone.clone().unwrap_or_default(), guest.diets.clone().unwrap_or_default()]
        .into_iter()
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" · ")
}

/// The lines of a text box for this team.
pub(crate) fn text_lines(style: &TextStyle, schedule: &Schedule, config: &SheetInfo) -> Vec<Line> {
    let german = config.german();
    let global = |key: &str| schedule.placeholder(key, config);
    let guests: Vec<Box<dyn Fn(&str) -> Option<String> + '_>> = schedule
        .guest_list
        .iter()
        .enumerate()
        .map(|(index, guest)| {
            Box::new(move |key: &str| guest_value(guest, index, key, german).or_else(|| schedule.placeholder(key, config)))
                as Box<dyn Fn(&str) -> Option<String> + '_>
        })
        .collect();
    expand(&style.text, &style.missing, &global, &guests)
}

#[component]
fn ElementBody(
    element: DesignElement,
    image_url: Option<String>,
    ratio: f32,
    config: SheetInfo,
    schedule: Schedule,
) -> Element {
    match &element.kind {
        ElementKind::Text(style) => {
            let lines = text_lines(style, &schedule, &config);
            let scale = fit_scale(&lines, style, FitBox { width: element.w, height: element.h, ratio });
            let icons = has_icon_column(&lines);
            let container = format!(
                "display:flex; flex-direction:column; justify-content:{}; width:100%; height:100%; overflow:hidden; \
                 font-family:{}; font-size:{:.3}cqw; color:{}; font-weight:{}; font-style:{}; text-align:{}; \
                 line-height:{:.3}; letter-spacing:{:.3}em; text-transform:{};",
                style.valign.css(),
                style.font.css(),
                style.size * scale,
                safe_color(&style.color, "#222222"),
                if style.bold { 700 } else { 400 },
                if style.italic { "italic" } else { "normal" },
                style.align.css(),
                style.line_height,
                style.letter_spacing / 100.0,
                if style.uppercase { "uppercase" } else { "none" },
            );
            let justify = match style.align {
                crate::schedule_design::Align::Left => "flex-start",
                crate::schedule_design::Align::Center => "center",
                crate::schedule_design::Align::Right => "flex-end",
            };
            let row_style = format!(
                "display:flex; align-items:flex-start; justify-content:{justify}; gap:0.4em; min-height:{:.3}em;",
                style.line_height
            );
            let icon_box = format!(
                "flex:none; width:1.1em; height:{:.3}em; display:flex; align-items:center;",
                style.line_height
            );
            let gap = format!("height:{:.3}em;", style.block_gap);
            let tint = style.icon_tint;
            rsx! {
                div { style: "{container}",
                    for (index, line) in lines.into_iter().enumerate() {
                        {
                            let icon = line.icon.map(|icon| image_src(&format!("builtin:{}", icon.graphic(tint))));
                            let spans = bold_spans(&line.text);
                            rsx! {
                                if line.spacer {
                                    div { key: "{index}", style: "{gap}" }
                                } else {
                                    div { key: "{index}", style: "{row_style}",
                                        if icons {
                                            div { style: "{icon_box}",
                                                if let Some(src) = icon {
                                                    img { src: "{src}", alt: "", draggable: "false", style: "width:1.05em; height:1.05em;" }
                                                }
                                            }
                                        }
                                        div { style: "white-space:pre-wrap; overflow-wrap:anywhere; min-width:0;",
                                            for (span_index, (text, bold)) in spans.into_iter().enumerate() {
                                                if bold {
                                                    strong { key: "{span_index}", style: "font-weight:700;", "{text}" }
                                                } else {
                                                    span { key: "{span_index}", "{text}" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        ElementKind::Image(style) => {
            let url = image_url;
            let css = format!(
                "width:100%; height:100%; overflow:hidden; border-radius:{:.3}cqw;",
                style.radius
            );
            let fit = format!("width:100%; height:100%; object-fit:{};", style.fit.css());
            rsx! {
                div { style: "{css}",
                    if let Some(url) = url {
                        img { style: "{fit}", src: "{url}", alt: "", draggable: "false" }
                    }
                }
            }
        }
        ElementKind::List(_) => rsx! {},
        ElementKind::Shape(style) => {
            let css = format!(
                "width:100%; height:100%; box-sizing:border-box; background:{}; border:{:.3}cqw solid {}; border-radius:{:.3}cqw;",
                safe_color(&style.fill, "#DDDDDD"),
                style.border_width,
                safe_color(&style.border_color, "#000000"),
                style.radius
            );
            rsx! { div { style: "{css}" } }
        }
    }
}

/// Downloads the sheet (`#section-to-print`) as PNG with `width` pixels.
pub async fn download_design(file_name: &str, width: u32) -> Result<(), String> {
    // `file_name` is already cleaned (see `export::file_stem`); JSON quoting keeps it a valid string.
    let name = serde_json::to_string(&format!("{file_name}.png")).map_err(|e| e.to_string())?;
    let js = format!(
        r#"
        const el = document.getElementById('section-to-print');
        if (!el) {{ return 'The sheet is not on the page.'; }}
        if (typeof htmlToImage === 'undefined') {{ return 'The image library is not loaded yet. Try again in a moment.'; }}
        try {{
            const dataUrl = await htmlToImage.toPng(el, {{
                pixelRatio: {width} / el.clientWidth,
                cacheBust: false,
                filter: (node) => !(node.dataset && node.dataset.noExport),
            }});
            const link = document.createElement('a');
            link.download = {name};
            link.href = dataUrl;
            link.click();
            return 'ok';
        }} catch (e) {{
            return String(e);
        }}
        "#
    );
    match document::eval(&js).await {
        Ok(serde_json::Value::String(result)) if result == "ok" => Ok(()),
        Ok(serde_json::Value::String(reason)) => Err(reason),
        Ok(other) => Err(format!("Unexpected response: {other}")),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schedule_design::{Element, Missing};
    use crate::schedule_text::{guests_compact_snippet, guests_snippet, team_snippet};

    fn config(german: bool) -> SheetInfo {
        let language = if german { crate::storage::Language::German } else { crate::storage::Language::English };
        SheetInfo::new("Cook & Run", None, language)
    }

    fn team(name: &str, members: Option<u16>, phone: Option<&str>, diets: Option<&str>) -> Team {
        Team {
            name: name.to_string(),
            address: String::new(),
            mail: None,
            phone: phone.map(str::to_string),
            members,
            diets: diets.map(str::to_string),
        }
    }

    fn style(text: &str) -> TextStyle {
        match Element::text("t", text, 0.0, 0.0, 10.0, 10.0, 3.0).kind {
            ElementKind::Text(style) => style,
            _ => unreachable!(),
        }
    }

    fn texts(lines: &[Line]) -> Vec<String> {
        lines.iter().map(|l| l.text.clone()).collect()
    }

    #[test]
    fn a_guest_block_is_repeated_per_guest_and_drops_empty_rows() {
        let mut schedule = Schedule::sample(true);
        schedule.guest_list = vec![
            team("Kochmützen", Some(2), Some("0170 1"), Some("vegan")),
            team("Wok & Roll", None, None, None),
        ];
        let lines = text_lines(&style(&guests_snippet(true)), &schedule, &config(true));
        assert_eq!(
            texts(&lines),
            vec!["**Kochmützen (2)**", "0170 1", "vegan", "", "**Wok & Roll**"],
            "the second guest only has a name: no icons without text, no gaps"
        );
    }

    #[test]
    fn info_joins_what_is_there() {
        let mut schedule = Schedule::sample(true);
        schedule.guest_list = vec![
            team("A", Some(3), None, Some("vegan")),
            team("B", None, Some("0170 2"), None),
            team("C", None, None, None),
        ];
        let lines = text_lines(&style(&guests_compact_snippet()), &schedule, &config(true));
        let non_spacer: Vec<String> = lines.iter().filter(|l| !l.spacer).map(|l| l.text.clone()).collect();
        assert_eq!(non_spacer, vec!["**A**", "3 Pers. · vegan", "**B**", "0170 2", "**C**"]);
    }

    #[test]
    fn the_team_block_shows_only_what_the_team_gave() {
        let mut schedule = Schedule::sample(true);
        schedule.host = team("Nur Name", None, None, None);
        let lines = text_lines(&style(&team_snippet(true)), &schedule, &config(true));
        assert_eq!(texts(&lines), vec!["**Nur Name**"]);
        // Same box, but missing values show a dash instead.
        let mut dash = style(&team_snippet(true));
        dash.missing = Missing::Text("–".into());
        let lines = text_lines(&dash, &schedule, &config(true));
        assert!(texts(&lines).contains(&"–".to_string()));
        assert_eq!(lines.len(), 6);
    }

    #[test]
    fn guest_placeholders_fall_back_to_the_normal_ones() {
        let mut schedule = Schedule::sample(true);
        schedule.guest_list = vec![team("A", None, None, None)];
        let lines = text_lines(&style("{#guests}\n{n}. {name} bei {team_name}\n{/guests}"), &schedule, &config(true));
        assert_eq!(texts(&lines), vec!["1. A bei Pasta Power"]);
    }

    #[test]
    fn every_preset_has_content_for_the_sample_team() {
        let schedule = Schedule::sample(true);
        for preset in crate::schedule_design::Preset::ALL {
            let design = preset.build();
            for element in &design.elements {
                if let ElementKind::Text(style) = &element.kind {
                    if style.text.contains("{#guests}") || style.text.contains("{team_name}") {
                        assert!(
                            !text_lines(style, &schedule, &config(true)).is_empty(),
                            "{}: {} is empty",
                            design.name,
                            element.name
                        );
                    }
                }
            }
        }
    }
}
