//! Markdown → HTML for the plan title and description.
//!
//! The text is entered by project organizers but rendered with
//! `dangerous_inner_html` for everybody who opens the plan (including participants
//! with a share link). `pulldown_cmark` passes raw HTML and `javascript:` links
//! through unchanged, which would allow stored XSS. This module keeps the markdown
//! features and the few harmless HTML tags the default text uses (`<center>`), and
//! neutralizes everything else.

use pulldown_cmark::{html, CowStr, Event, Parser, Tag};

/// HTML tags that are passed through – *without attributes* (no `style`, no `on*`).
const ALLOWED_TAGS: [&str; 8] = ["center", "b", "i", "u", "em", "strong", "small", "br"];

/// `Some(normalized tag)` for an allowed tag, `None` for anything else
/// (unknown tags, tags with attributes, comments, …).
fn allowed_tag(raw: &str) -> Option<String> {
    let inner = raw.strip_prefix('<')?.strip_suffix('>')?.trim();
    let (closing, name) = match inner.strip_prefix('/') {
        Some(rest) => (true, rest.trim()),
        None => (false, inner.trim_end_matches('/').trim()),
    };
    let name = name.to_ascii_lowercase();
    // "b onclick=…" contains a space and therefore never matches.
    if !ALLOWED_TAGS.contains(&name.as_str()) {
        return None;
    }
    Some(if closing { format!("</{name}>") } else { format!("<{name}>") })
}

fn escape(text: &str) -> String {
    text.replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Sanitizes a fragment of raw HTML coming from the markdown parser.
fn sanitize_fragment(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(start) = rest.find('<') {
        out.push_str(&escape(&rest[..start]));
        let from_tag = &rest[start..];
        match from_tag.find('>') {
            Some(end) => {
                let tag = &from_tag[..=end];
                match allowed_tag(tag) {
                    Some(clean) => out.push_str(&clean),
                    None => out.push_str(&escape(tag)),
                }
                rest = &from_tag[end + 1..];
            }
            None => {
                out.push_str(&escape(from_tag));
                rest = "";
            }
        }
    }
    out.push_str(&escape(rest));
    out
}

/// Only plain web/mail/phone links and in-page or relative paths.
fn safe_url(url: &str) -> bool {
    let url = url.trim().to_ascii_lowercase();
    url.starts_with("http://")
        || url.starts_with("https://")
        || url.starts_with("mailto:")
        || url.starts_with("tel:")
        || url.starts_with('#')
        || url.starts_with('/')
}

pub fn render(text: &str) -> String {
    let events = Parser::new(text).map(|event| match event {
        Event::Html(raw) | Event::InlineHtml(raw) => {
            Event::Html(CowStr::from(sanitize_fragment(&raw)))
        }
        Event::Start(Tag::Link { link_type, dest_url, title, id }) if !safe_url(&dest_url) => {
            Event::Start(Tag::Link { link_type, dest_url: CowStr::from("#"), title, id })
        }
        Event::Start(Tag::Image { link_type, dest_url, title, id }) if !safe_url(&dest_url) => {
            Event::Start(Tag::Image { link_type, dest_url: CowStr::from("#"), title, id })
        }
        other => other,
    });
    let mut out = String::new();
    html::push_html(&mut out, events);
    out
}

#[cfg(test)]
mod tests {
    use super::render;

    #[test]
    fn default_description_keeps_its_center_tag() {
        let html = render("<center>This is the plan for the cook and run event.</center>");
        assert!(html.contains("<center>"));
        assert!(html.contains("</center>"));
        assert!(html.contains("This is the plan"));
    }

    #[test]
    fn markdown_features_still_work() {
        let html = render("# Title\n\n**bold** and *italic* and [site](https://example.com)");
        assert!(html.contains("<h1>Title</h1>"));
        assert!(html.contains("<strong>bold</strong>"));
        assert!(html.contains("<em>italic</em>"));
        assert!(html.contains(r#"href="https://example.com""#));
    }

    #[test]
    fn script_tags_are_escaped() {
        let html = render("hello <script>alert(1)</script>");
        assert!(!html.contains("<script"));
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn event_handler_attributes_are_escaped() {
        let html = render("<img src=x onerror=alert(1)>");
        assert!(!html.contains("<img"));
        let html = render("text <b onclick=\"alert(1)\">x</b>");
        assert!(!html.contains("onclick=\"alert"));
        assert!(!html.contains("<b onclick"));
    }

    #[test]
    fn html_blocks_with_attributes_are_escaped() {
        let html = render("<div style=\"x\" onmouseover=\"alert(1)\">hi</div>");
        assert!(!html.contains("<div"));
    }

    #[test]
    fn javascript_links_are_neutralized() {
        for md in [
            "[x](javascript:alert(1))",
            "[x](JaVaScRiPt:alert(1))",
            "[x](jav&#x09;ascript:alert(1))",
            "[x](data:text/html;base64,AAAA)",
            "![x](javascript:alert(1))",
        ] {
            let html = render(md);
            assert!(!html.to_lowercase().contains("javascript:"), "{md} -> {html}");
            assert!(!html.contains("data:"), "{md} -> {html}");
        }
    }

    #[test]
    fn unterminated_tags_do_not_leak() {
        let html = render("a <b onclick=alert(1)");
        assert!(!html.contains("<b onclick"));
    }
}
