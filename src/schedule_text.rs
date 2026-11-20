//! What a text box of the schedule designer can contain, and how it is turned into lines.
//!
//! A text is plain text with a few extras:
//!
//! | write                            | result                                                      |
//! |----------------------------------|-------------------------------------------------------------|
//! | `{team_phone}`                   | the value for this team                                     |
//! | `**text**`                       | bold                                                        |
//! | `{icon:phone} {team_phone}`      | a small icon in front of the line (`{icon:none}`: only room)|
//! | `{#guests}` ... `{/guests}`      | the lines in between, once for every guest team             |
//!
//! Optional values (a phone number the team did not give) are handled per text box, see
//! [`Missing`]: the line is left out, stays empty, or shows a replacement text.

use crate::schedule_design::{ListIcon, Missing, TextStyle, fill, is_empty_line};

pub const GUESTS_OPEN: &str = "{#guests}";
pub const GUESTS_CLOSE: &str = "{/guests}";

/// One line of a text box with the values filled in.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub icon: Option<ListIcon>,
    /// Keeps the room of an icon free (`{icon:none}`), so that the text lines up with the lines that
    /// have an icon.
    pub reserve_icon: bool,
    pub text: String,
    /// An empty space between two guests.
    pub spacer: bool,
}

pub type Resolver<'a> = &'a dyn Fn(&str) -> Option<String>;

/// Splits `{icon:phone} rest` into the icon and the rest.
fn parse_icon(line: &str) -> (Option<ListIcon>, bool, &str) {
    let trimmed = line.trim_start();
    if let Some(after) = trimmed.strip_prefix("{icon:") {
        if let Some(close) = after.find('}') {
            let key = &after[..close];
            let rest = after[close + 1..]
                .strip_prefix(' ')
                .unwrap_or(&after[close + 1..]);
            if key == "none" {
                return (None, true, rest);
            }
            if let Some(icon) = ListIcon::from_key(key) {
                return (Some(icon), false, rest);
            }
        }
    }
    (None, false, line)
}

fn render_line(line: &str, missing: &Missing, resolve: Resolver) -> Option<Line> {
    let (icon, reserve_icon, rest) = parse_icon(line);
    if matches!(missing, Missing::Hide) && is_empty_line(rest, resolve) {
        return None;
    }
    let text = match missing {
        Missing::Text(replacement) => fill(rest, &|key| {
            resolve(key).map(|value| {
                if value.trim().is_empty() {
                    replacement.clone()
                } else {
                    value
                }
            })
        }),
        _ => fill(rest, resolve),
    };
    Some(Line {
        icon,
        reserve_icon,
        text,
        spacer: false,
    })
}

fn render_lines(lines: &[&str], missing: &Missing, resolve: Resolver) -> Vec<Line> {
    lines
        .iter()
        .filter_map(|line| render_line(line, missing, resolve))
        .collect()
}

/// The lines of a text box. `global` resolves the usual placeholders, `guests` has one resolver per
/// guest team (they answer `{name}`, `{phone}`, ... and fall back to `global`).
pub fn expand(
    template: &str,
    missing: &Missing,
    global: Resolver,
    guests: &[Box<dyn Fn(&str) -> Option<String> + '_>],
) -> Vec<Line> {
    let lines: Vec<&str> = template.split('\n').collect();
    let mut out = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        if lines[index].trim() == GUESTS_OPEN {
            let start = index + 1;
            let mut end = start;
            while end < lines.len() && lines[end].trim() != GUESTS_CLOSE {
                end += 1;
            }
            let body = &lines[start..end];
            let blocks: Vec<Vec<Line>> = guests
                .iter()
                .map(|guest| render_lines(body, missing, &**guest))
                .filter(|block| !block.is_empty())
                .collect();
            for (n, block) in blocks.into_iter().enumerate() {
                if n > 0 {
                    out.push(Line {
                        icon: None,
                        reserve_icon: false,
                        text: String::new(),
                        spacer: true,
                    });
                }
                out.extend(block);
            }
            index = end + 1;
            continue;
        }
        out.extend(render_line(lines[index], missing, global));
        index += 1;
    }
    out
}

pub fn has_icon_column(lines: &[Line]) -> bool {
    lines
        .iter()
        .any(|line| line.icon.is_some() || line.reserve_icon)
}

/// `**bold**` parts of a line: `(text, bold)`. An unclosed `**` stays as it is.
pub fn bold_spans(text: &str) -> Vec<(String, bool)> {
    let parts: Vec<&str> = text.split("**").collect();
    if parts.len() % 2 == 0 {
        // An odd number of markers: not a pair, show the text as it is.
        return vec![(text.to_string(), false)];
    }
    parts
        .into_iter()
        .enumerate()
        .filter(|(_, part)| !part.is_empty())
        .map(|(index, part)| (part.to_string(), index % 2 == 1))
        .collect()
}

/// The box a text has to fit into: `width` and `height` are percent of the sheet width and height,
/// `ratio` is height / width of the sheet.
#[derive(Clone, Copy, Debug)]
pub struct FitBox {
    pub width: f32,
    pub height: f32,
    pub ratio: f32,
}

/// Rough height in units of the sheet width that `lines` need at font size `size`, wrapped lines
/// included (a character is about 0.56 em wide).
fn measure(lines: &[Line], style: &TextStyle, width: f32, size: f32) -> f32 {
    let icons = has_icon_column(lines);
    let text_width = width - if icons { size * 1.5 } else { 0.0 };
    let em_per_line = (text_width / size).max(1.0);
    lines
        .iter()
        .map(|line| {
            if line.spacer {
                return style.block_gap * size;
            }
            let plain = line.text.replace("**", "");
            let wrapped: f32 = plain
                .split('\n')
                .map(|part| {
                    ((part.chars().count() as f32 * 0.56) / em_per_line)
                        .ceil()
                        .max(1.0)
                })
                .sum();
            wrapped * style.line_height * size
        })
        .sum()
}

/// How much smaller than `style.size` the text has to be to fit into the box (1.0: not at all).
/// Only used for boxes with `shrink`.
pub fn fit_scale(lines: &[Line], style: &TextStyle, area: FitBox) -> f32 {
    if !style.shrink || lines.is_empty() {
        return 1.0;
    }
    let available = area.height * area.ratio;
    let mut scale = 1.0f32;
    while scale >= 0.4 {
        if measure(lines, style, area.width, style.size * scale) <= available {
            return scale;
        }
        scale -= 0.05;
    }
    0.4
}

// ─────────────────────────────────────────────
//  Text blocks: what the designer offers to insert
// ─────────────────────────────────────────────

pub struct Block {
    pub label: String,
    /// What is inserted (may have several lines).
    pub snippet: String,
}

pub struct BlockGroup {
    pub title: String,
    /// One sentence on how the blocks behave (repeated per guest, ...).
    pub note: String,
    pub blocks: Vec<Block>,
}

fn block(label: &str, snippet: &str) -> Block {
    Block {
        label: label.to_string(),
        snippet: snippet.to_string(),
    }
}

/// Removes the icons from a snippet.
pub fn without_icons(snippet: &str) -> String {
    snippet
        .split('\n')
        .map(|line| {
            let (_, _, rest) = parse_icon(line);
            rest.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn persons_word(german: bool) -> &'static str {
    if german { "Personen" } else { "persons" }
}

/// All lines about the own team (`TeamPlanData::host`). The e-mail only exists in local projects.
pub fn team_snippet(german: bool) -> String {
    format!(
        "{{icon:person}} **{{team_name}}**\n{{icon:none}} {{team_persons}} {}\n{{icon:mail}} {{team_mail}}\n{{icon:phone}} {{team_phone}}\n{{icon:pin}} {{team_address}}\n{{icon:warning}} {{team_diets}}",
        persons_word(german)
    )
}

/// Every guest team with all details (`TeamPlanData::host_guest`), three lines each.
pub fn guests_snippet(_german: bool) -> String {
    format!(
        "{GUESTS_OPEN}\n{{icon:person}} **{{name_persons}}**\n{{icon:phone}} {{phone}}\n{{icon:warning}} {{diets}}\n{GUESTS_CLOSE}"
    )
}

/// Every guest team in two lines.
pub fn guests_compact_snippet() -> String {
    format!("{GUESTS_OPEN}\n{{icon:person}} **{{name}}**\n{{icon:none}} {{info}}\n{GUESTS_CLOSE}")
}

/// The host of course `n` (`TeamPlanData::visitations`).
pub fn host_snippet(n: usize) -> String {
    format!(
        "{{icon:person}} {{course{n}_host}}\n{{icon:phone}} {{course{n}_phone}}\n{{icon:pin}} {{course{n}_address}}"
    )
}

/// Time, name and host of course `n`.
pub fn course_snippet(n: usize) -> String {
    format!(
        "**{{course{n}_marker}}{{course{n}_time}} – {{course{n}_name}}**\n{}",
        host_snippet(n)
    )
}

/// The groups of the "Text blocks" panel. With `icons` the snippets contain icons.
pub fn block_catalog(courses: usize, german: bool, icons: bool) -> Vec<BlockGroup> {
    let keep = |snippet: String| {
        if icons {
            snippet
        } else {
            without_icons(&snippet)
        }
    };
    let line = |icon: &str, key: &str| keep(format!("{{icon:{icon}}} {{{key}}}"));

    let mut groups = vec![
        BlockGroup {
            title: "This team".into(),
            note: "Lines without a value are left out (or shown as you choose below).".into(),
            blocks: vec![
                block("Everything about this team", &keep(team_snippet(german))),
                block("Name", &line("person", "team_name")),
                block(
                    "Persons",
                    &format!("{{team_persons}} {}", persons_word(german)),
                ),
                block("Phone", &line("phone", "team_phone")),
                block("E-mail (local projects)", &line("mail", "team_mail")),
                block("Address", &line("pin", "team_address")),
                block("Diets / allergies", &line("warning", "team_diets")),
            ],
        },
        BlockGroup {
            title: "Guests".into(),
            note: "Repeated for every guest team: two guests give two blocks, five give five."
                .into(),
            blocks: vec![
                block("Every guest with details", &keep(guests_snippet(german))),
                block("Every guest in two lines", &keep(guests_compact_snippet())),
                block("Names of all guests, in one line", "{guest_names}"),
                block("Number of guests", "{guest_count}"),
            ],
        },
    ];

    let mut course_blocks = Vec::new();
    for n in 1..=courses {
        course_blocks.push(block(
            &format!("Course {n}: time, name and host"),
            &keep(course_snippet(n)),
        ));
        course_blocks.push(block(
            &format!("Course {n}: host and address"),
            &keep(host_snippet(n)),
        ));
        course_blocks.push(block(
            &format!("Course {n}: time"),
            &format!("{{course{n}_time}}"),
        ));
        course_blocks.push(block(
            &format!("Course {n}: name"),
            &format!("{{course{n}_marker}}{{course{n}_name}}"),
        ));
    }
    course_blocks.push(block(
        "The course this team cooks",
        &format!("{{cook_course}}, {{cook_time}}"),
    ));
    groups.push(BlockGroup {
        title: "Courses".into(),
        note: "The hosts of the courses. The star marks the course this team cooks.".into(),
        blocks: course_blocks,
    });

    groups.push(BlockGroup {
        title: "Start and end".into(),
        note: String::new(),
        blocks: vec![
            block(
                "Start: time and place",
                &keep(format!(
                    "{{start_time}} – {{start_name}}\n{{icon:pin}} {{start_address}}"
                )),
            ),
            block(
                "End (after party): time and place",
                &keep(format!(
                    "{{end_time}} – {{end_name}}\n{{icon:pin}} {{end_address}}"
                )),
            ),
        ],
    });
    groups
}

// ─────────────────────────────────────────────
//  Inserting at the cursor
// ─────────────────────────────────────────────

/// Index in chars of a position given in UTF-16 units (what the browser reports).
fn char_index(text: &str, utf16: usize) -> usize {
    let mut units = 0;
    for (index, c) in text.chars().enumerate() {
        if units >= utf16 {
            return index;
        }
        units += c.len_utf16();
    }
    text.chars().count()
}

/// Puts `snippet` into `text` at the cursor (UTF-16 units), on a line of its own. Returns the new
/// text and the cursor behind the snippet (also in UTF-16 units).
pub fn insert_block(text: &str, cursor_utf16: usize, snippet: &str) -> (String, usize) {
    let at = char_index(text, cursor_utf16);
    let chars: Vec<char> = text.chars().collect();
    let before: String = chars[..at].iter().collect();
    let after: String = chars[at..].iter().collect();
    let lead = if before.is_empty() || before.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    let trail = if after.is_empty() || after.starts_with('\n') {
        ""
    } else {
        "\n"
    };
    let inserted = format!("{lead}{snippet}{trail}");
    let cursor = before.encode_utf16().count() + format!("{lead}{snippet}").encode_utf16().count();
    (format!("{before}{inserted}{after}"), cursor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schedule_design::Missing;

    fn global(key: &str) -> Option<String> {
        match key {
            "team_name" => Some("Pasta Power".into()),
            "team_phone" => Some(String::new()),
            "team_diets" => Some("vegan".into()),
            _ => None,
        }
    }

    fn guest(name: &'static str, phone: &'static str) -> Box<dyn Fn(&str) -> Option<String>> {
        Box::new(move |key| match key {
            "name" => Some(name.into()),
            "phone" => Some(phone.into()),
            other => global(other),
        })
    }

    fn texts(lines: &[Line]) -> Vec<String> {
        lines.iter().map(|l| l.text.clone()).collect()
    }

    #[test]
    fn missing_lines_are_hidden_by_default() {
        let lines = expand(
            "{icon:person} {team_name}\n{icon:phone} {team_phone}\nDiäten: {team_diets}",
            &Missing::Hide,
            &global,
            &[],
        );
        assert_eq!(texts(&lines), vec!["Pasta Power", "Diäten: vegan"]);
        assert_eq!(lines[0].icon, Some(ListIcon::Person));
    }

    #[test]
    fn missing_values_can_stay_empty_or_be_replaced() {
        let template = "{icon:phone} {team_phone}\nTel: {team_phone}";
        let kept = expand(template, &Missing::Empty, &global, &[]);
        assert_eq!(texts(&kept), vec!["", "Tel: "]);
        let replaced = expand(template, &Missing::Text("–".into()), &global, &[]);
        assert_eq!(texts(&replaced), vec!["–", "Tel: –"]);
        assert_eq!(replaced[0].icon, Some(ListIcon::Phone));
    }

    #[test]
    fn a_guest_block_is_repeated_for_every_guest() {
        let guests = vec![guest("Kochmützen", "0170 1"), guest("Wok & Roll", "")];
        let lines = expand(
            "Gäste:\n{#guests}\n**{name}**\n{icon:phone} {phone}\n{/guests}\nEnde",
            &Missing::Hide,
            &global,
            &guests,
        );
        assert_eq!(
            texts(&lines),
            vec![
                "Gäste:",
                "**Kochmützen**",
                "0170 1",
                "",
                "**Wok & Roll**",
                "Ende"
            ]
        );
        assert!(lines[3].spacer);
        // The second guest has no phone: its icon is gone with the line.
        assert_eq!(lines.iter().filter(|l| l.icon.is_some()).count(), 1);
    }

    #[test]
    fn without_guests_the_block_disappears() {
        let lines = expand(
            "A\n{#guests}\n{name}\n{/guests}\nB",
            &Missing::Hide,
            &global,
            &[],
        );
        assert_eq!(texts(&lines), vec!["A", "B"]);
    }

    #[test]
    fn an_unclosed_guest_block_runs_to_the_end() {
        let guests = vec![guest("A", "1")];
        let lines = expand(
            "{#guests}\n{name}\n{phone}",
            &Missing::Hide,
            &global,
            &guests,
        );
        assert_eq!(texts(&lines), vec!["A", "1"]);
    }

    #[test]
    fn icons_and_room_for_icons() {
        let lines = expand(
            "{icon:none} indented\n{icon:pin} here\n{icon:unknown} stays",
            &Missing::Hide,
            &global,
            &[],
        );
        assert!(lines[0].reserve_icon && lines[0].icon.is_none());
        assert_eq!(lines[1].icon, Some(ListIcon::Pin));
        assert_eq!(lines[2].text, "{icon:unknown} stays");
        assert!(has_icon_column(&lines));
    }

    #[test]
    fn bold_parts() {
        assert_eq!(
            bold_spans("a **b** c"),
            vec![
                ("a ".into(), false),
                ("b".into(), true),
                (" c".into(), false)
            ]
        );
        assert_eq!(bold_spans("**all**"), vec![("all".into(), true)]);
        assert_eq!(
            bold_spans("open ** only"),
            vec![("open ** only".into(), false)]
        );
        assert_eq!(bold_spans("plain"), vec![("plain".into(), false)]);
    }

    #[test]
    fn long_texts_shrink_when_asked_to() {
        let mut style =
            match crate::schedule_design::Element::text("t", "", 0.0, 0.0, 30.0, 10.0, 3.0).kind {
                crate::schedule_design::ElementKind::Text(style) => style,
                _ => unreachable!(),
            };
        let lines: Vec<Line> = (0..12)
            .map(|i| Line {
                icon: None,
                reserve_icon: false,
                text: format!("Zeile {i}"),
                spacer: false,
            })
            .collect();
        let area = FitBox {
            width: 30.0,
            height: 12.0 / 1.25,
            ratio: 1.25,
        };
        assert_eq!(
            fit_scale(&lines, &style, area),
            1.0,
            "only boxes with shrink shrink"
        );
        style.shrink = true;
        let scale = fit_scale(&lines, &style, area);
        assert!(scale < 0.8 && scale >= 0.4, "{scale}");
        assert_eq!(fit_scale(&lines[..2], &style, area), 1.0);
    }

    #[test]
    fn wrapped_lines_are_counted() {
        let mut style =
            match crate::schedule_design::Element::text("t", "", 0.0, 0.0, 20.0, 10.0, 3.0).kind {
                crate::schedule_design::ElementKind::Text(style) => style,
                _ => unreachable!(),
            };
        style.shrink = true;
        let long = vec![Line {
            icon: None,
            reserve_icon: false,
            text: "ein sehr langer Text, der umbrechen muss".into(),
            spacer: false,
        }];
        let narrow = FitBox {
            width: 20.0,
            height: 4.0 / 1.25,
            ratio: 1.25,
        };
        let wide = FitBox {
            width: 80.0,
            height: 4.0 / 1.25,
            ratio: 1.25,
        };
        assert!(fit_scale(&long, &style, narrow) < fit_scale(&long, &style, wide));
    }

    #[test]
    fn inserting_puts_the_block_on_its_own_line() {
        assert_eq!(
            insert_block("", 0, "{team_name}"),
            ("{team_name}".to_string(), 11)
        );
        assert_eq!(insert_block("abc", 3, "X"), ("abc\nX".to_string(), 5));
        assert_eq!(
            insert_block("abc\ndef", 4, "X"),
            ("abc\nX\ndef".to_string(), 5)
        );
        assert_eq!(insert_block("ab|cd", 2, "X"), ("ab\nX\n|cd".to_string(), 4));
        // The browser counts in UTF-16 units: "★" is one, so the cursor after "★ a" is 3.
        assert_eq!(insert_block("★ ab", 3, "X"), ("★ a\nX\nb".to_string(), 5));
    }

    #[test]
    fn the_catalog_covers_the_team_plan_and_can_drop_icons() {
        let with = block_catalog(3, true, true);
        let all: String = with
            .iter()
            .flat_map(|g| g.blocks.iter().map(|b| b.snippet.clone()))
            .collect::<Vec<_>>()
            .join("\n");
        for needed in [
            "{team_name}",
            "{team_phone}",
            "{team_diets}",
            "{#guests}",
            "{name}",
            "{phone}",
            "{diets}",
            "{course3_address}",
            "{course1_host}",
            "{icon:phone}",
        ] {
            assert!(all.contains(needed), "{needed}");
        }
        let without = block_catalog(3, true, false);
        let all: String = without
            .iter()
            .flat_map(|g| g.blocks.iter().map(|b| b.snippet.clone()))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!all.contains("{icon:"));
        assert!(all.contains("{team_phone}"));
    }
}
