//! Per-line unified diff with lightweight Cooklang-aware syntax highlighting.

use crate::server::render::esc;
use similar::{ChangeTag, TextDiff};
use std::fmt::Write;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    Equal,
    Added,
    Removed,
}

#[derive(Debug, Clone)]
pub struct DiffLine {
    pub kind: LineKind,
    pub old_lineno: Option<usize>,
    pub new_lineno: Option<usize>,
    pub html: String, // already escaped + highlighted
}

pub fn unified_diff(old: &str, new: &str) -> Vec<DiffLine> {
    let diff = TextDiff::from_lines(old, new);
    let mut lines = Vec::new();
    for change in diff.iter_all_changes() {
        let kind = match change.tag() {
            ChangeTag::Equal => LineKind::Equal,
            ChangeTag::Insert => LineKind::Added,
            ChangeTag::Delete => LineKind::Removed,
        };
        let raw = change.value();
        let raw = raw.strip_suffix('\n').unwrap_or(raw);
        let highlighted = highlight_line(raw);
        lines.push(DiffLine {
            kind,
            old_lineno: change.old_index().map(|i| i + 1),
            new_lineno: change.new_index().map(|i| i + 1),
            html: highlighted,
        });
    }
    lines
}

/// Render the diff lines to HTML.
pub fn render_html(lines: &[DiffLine]) -> String {
    let mut out = String::new();
    out.push_str("<div class=\"diff\">\n");
    for line in lines {
        let cls = match line.kind {
            LineKind::Equal => "eq",
            LineKind::Added => "add",
            LineKind::Removed => "del",
        };
        let sigil = match line.kind {
            LineKind::Equal => " ",
            LineKind::Added => "+",
            LineKind::Removed => "-",
        };
        let old = line.old_lineno.map(|n| n.to_string()).unwrap_or_default();
        let new = line.new_lineno.map(|n| n.to_string()).unwrap_or_default();
        write!(
            out,
            r#"<div class="diff-line {cls}"><span class="ln old">{old}</span><span class="ln new">{new}</span><span class="sigil">{sigil}</span><span class="src">{src}</span></div>"#,
            cls = cls,
            old = old,
            new = new,
            sigil = sigil,
            src = line.html,
        )
        .ok();
        out.push('\n');
    }
    out.push_str("</div>\n");
    out
}

/// Tag Cooklang-y tokens within a line so the CSS can colour them.
/// Operates on raw (unescaped) text and produces escaped HTML.
fn highlight_line(line: &str) -> String {
    if line.is_empty() {
        return String::new();
    }
    if let Some(rest) = line.strip_prefix(">>") {
        return format!(r#"<span class="meta-line">&gt;&gt;{}</span>"#, esc(rest));
    }
    if line.starts_with("---") {
        return format!(r#"<span class="meta-line">{}</span>"#, esc(line));
    }
    let mut out = String::with_capacity(line.len() + 16);
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        match c {
            b'@' | b'#' | b'~' => {
                let class = match c {
                    b'@' => "ing",
                    b'#' => "cook",
                    _ => "timer",
                };
                let (consumed, body) = take_component(&line[i..]);
                if consumed > 0 {
                    let token = &line[i..i + consumed];
                    write!(out, r#"<span class="tk-{class}">{}</span>"#, esc(token)).ok();
                    i += consumed;
                } else {
                    out.push_str(&esc(body));
                    i += 1;
                }
            }
            _ => {
                // Append until the next sigil.
                let start = i;
                while i < bytes.len() && !matches!(bytes[i], b'@' | b'#' | b'~') {
                    i += 1;
                }
                out.push_str(&esc(&line[start..i]));
            }
        }
    }
    out
}

/// Given a slice starting at `@`, `#`, or `~`, return how many bytes to
/// consume as a single Cooklang component (sigil + name + optional `{…}` +
/// optional `(…)` note). Returns `(0, "")` if the sigil isn't followed by a
/// recognisable component.
fn take_component(s: &str) -> (usize, &str) {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return (0, "");
    }
    let first = bytes[0];
    if !matches!(first, b'@' | b'#' | b'~') {
        return (0, "");
    }
    if bytes.len() == 1 {
        return (0, "");
    }

    // Single-word form: sigil immediately followed by alphabetic char until
    // whitespace or punctuation.
    let mut i = 1;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'{' {
            // Multi-word braced form: consume until `}`
            i += 1;
            while i < bytes.len() && bytes[i] != b'}' {
                i += 1;
            }
            if i < bytes.len() && bytes[i] == b'}' {
                i += 1;
            }
            // Optional note in (…)
            if i < bytes.len() && bytes[i] == b'(' {
                i += 1;
                while i < bytes.len() && bytes[i] != b')' {
                    i += 1;
                }
                if i < bytes.len() && bytes[i] == b')' {
                    i += 1;
                }
            }
            break;
        }
        if c.is_ascii_whitespace() || matches!(c, b',' | b'.' | b';' | b':' | b'!' | b'?') {
            break;
        }
        i += 1;
    }

    if i == 1 {
        // No name characters → not a component.
        return (0, "");
    }
    (i, &s[..i])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_added_and_removed_lines() {
        let lines = unified_diff("a\nb\nc\n", "a\nbb\nc\n");
        let kinds: Vec<_> = lines.iter().map(|l| l.kind).collect();
        assert!(kinds.contains(&LineKind::Removed));
        assert!(kinds.contains(&LineKind::Added));
    }

    #[test]
    fn highlights_metadata_line() {
        let h = highlight_line(">> title: Carbonara");
        assert!(h.contains(r#"class="meta-line""#));
    }

    #[test]
    fn highlights_braced_ingredient() {
        let h = highlight_line("Mix @flour{200%g} into bowl.");
        assert!(h.contains(r#"class="tk-ing""#));
        assert!(h.contains("@flour{200%g}"));
    }

    #[test]
    fn highlights_cookware_and_timer() {
        let h = highlight_line("Heat #pan{} for ~{30%seconds}.");
        assert!(h.contains(r#"class="tk-cook""#));
        assert!(h.contains(r#"class="tk-timer""#));
    }

    #[test]
    fn does_not_consume_lone_sigil() {
        let h = highlight_line("Mix @ rest");
        // `@ ` should be escaped as `@` rather than treated as a token.
        assert!(!h.contains("class=\"tk-"), "got: {h}");
    }
}
