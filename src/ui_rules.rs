//! Design rules, checked by a test. `docs/DESIGN_GUIDE.md` explains them; this file makes them
//! binding: a page that builds its own button, input, hint box or caption style instead of using
//! `crate::ui` fails the tests.
//!
//! Adding to an allow-list below needs a reason in a comment. The lists are meant to shrink.

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    /// Every source file outside `ui/` as (path relative to `src/`, content).
    fn sources() -> Vec<(String, String)> {
        fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
            for entry in std::fs::read_dir(dir).expect("read src") {
                let path = entry.expect("entry").path();
                if path.is_dir() {
                    walk(&path, out);
                } else if path.extension().is_some_and(|ext| ext == "rs") {
                    out.push(path);
                }
            }
        }
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        walk(&root, &mut files);
        files
            .into_iter()
            .map(|path| {
                let name = path.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/");
                (name, std::fs::read_to_string(&path).expect("read file"))
            })
            .filter(|(name, _)| !name.starts_with("ui/") && name != "ui_rules.rs")
            .collect()
    }

    /// Lines of the pages that match `bad`, minus the allowed `(file, needle)` pairs.
    fn offenders(bad: impl Fn(&str) -> bool, allowed: &[(&str, &str)]) -> Vec<String> {
        let mut found = Vec::new();
        for (name, text) in sources() {
            for (number, line) in text.lines().enumerate() {
                if line.trim_start().starts_with("//") || !bad(line) {
                    continue;
                }
                if allowed.iter().any(|(file, needle)| *file == name && line.contains(needle)) {
                    continue;
                }
                found.push(format!("{name}:{}: {}", number + 1, line.trim_start().chars().take(110).collect::<String>()));
            }
        }
        found
    }

    fn assert_none(rule: &str, found: Vec<String>) {
        assert!(
            found.is_empty(),
            "\nDesign rule broken: {rule}\nUse the component from `crate::ui` (see docs/DESIGN_GUIDE.md):\n  {}\n",
            found.join("\n  ")
        );
    }

    /// Buttons, fields and selects come from `ui`: they carry the focus ring, the disabled look,
    /// the sizes and the accessibility attributes of the whole app.
    #[test]
    fn no_raw_form_controls_in_pages() {
        let controls = ["button {", "input {", "textarea {", "select {", "dialog {"];
        assert_none(
            "raw <button>/<input>/<textarea>/<select> in a page",
            offenders(
                |line| {
                    let t = line.trim_start();
                    controls.iter().any(|c| t.starts_with(c))
                },
                // The honeypot of the registration is an input that must NOT look like a field.
                &[("side/participant/register.rs", "input {")],
            ),
        );
    }

    /// One caption style (`Eyebrow` / `EYEBROW`) instead of one per page.
    #[test]
    fn captions_use_the_eyebrow_style() {
        assert_none(
            "own uppercase caption style",
            offenders(|line| line.contains("uppercase") && (line.contains("tracking-") || line.contains("text-[11px]")), &[]),
        );
    }

    /// Hints, warnings and errors are `Notice`; soft inset groups are `Panel`; folded rows are `Disclosure`.
    #[test]
    fn messages_use_notice_and_panel() {
        let tones = ["red", "amber", "green", "emerald", "zinc"];
        assert_none(
            "own bordered+tinted message box",
            offenders(
                |line| {
                    tones.iter().any(|tone| line.contains(&format!("border-{tone}-")) && line.contains(&format!("bg-{tone}-")))
                        && (line.contains("rounded-xl") || line.contains("rounded-lg"))
                },
                // The drop target that shows where a move would break a rule: a dashed frame, no message box.
                &[("side/details/plan_editor/board.rs", "border-dashed")],
            ),
        );
    }

    /// The brand palette is amber/zinc (+ green/red/emerald for states). Other color families would
    /// make a page look foreign.
    #[test]
    fn only_palette_colors() {
        let foreign = [
            "blue", "indigo", "violet", "purple", "pink", "rose", "sky", "cyan", "teal", "lime", "fuchsia", "slate", "gray",
            "neutral", "stone", "orange", "yellow",
        ];
        assert_none(
            "color family outside the palette",
            offenders(
                |line| {
                    foreign.iter().any(|family| {
                        ["bg", "text", "border", "ring", "from", "to"].iter().any(|prefix| line.contains(&format!("{prefix}-{family}-")))
                    })
                },
                &[],
            ),
        );
    }

    /// Page headings: `Headline1..3` or `PageHeader`. Only the shells that frame pages (legal text
    /// pages, the sidebar of a project, the project cards) have their own.
    #[test]
    fn headings_come_from_typography() {
        assert_none(
            "raw h1/h2/h3 in a page",
            offenders(
                |line| {
                    let t = line.trim_start();
                    t.starts_with("h1 {") || t.starts_with("h2 {") || t.starts_with("h3 {")
                },
                &[
                    ("side/legal/mod.rs", "h1 {"),
                    ("side/legal/mod.rs", "h2 {"),
                    ("side/details/mod.rs", "h2 {"),
                    ("side/dashboard.rs", "h3 {"),
                ],
            ),
        );
    }

    /// Texts a visitor reads on the guest pages come from `participant/lang.rs` (German and
    /// English), never as a literal in the page: a literal is only ever in one language.
    #[test]
    fn guest_pages_have_no_literal_sentences() {
        let guest = [
            "side/participant/register.rs",
            "side/participant/views.rs",
            "side/participant/fields.rs",
            "side/participant/team/mod.rs",
            "side/participant/team/edit.rs",
            "side/participant/team/status.rs",
            "side/participant/team/plan.rs",
        ];
        let mut found = Vec::new();
        for (name, text) in sources() {
            if !guest.contains(&name.as_str()) {
                continue;
            }
            for (number, line) in text.lines().enumerate() {
                let t = line.trim_start();
                // an rsx text node: a quoted string that is not an interpolation or an attribute
                if t.starts_with('"')
                    && t.ends_with('"')
                    && !t.starts_with("\"{")
                    && t.chars().filter(|c| c.is_alphabetic()).count() > 6
                    && !t.contains(": ")
                {
                    found.push(format!("{name}:{}: {t}", number + 1));
                }
            }
        }
        assert_none("literal text on a guest page (belongs in lang.rs, both languages)", found);
    }
}
