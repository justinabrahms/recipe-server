use super::{LineItem, ShoppingList, SourceRef};
use crate::server::render::esc;
use std::collections::HashMap;
use std::fmt::Write;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Text,
    Html,
}

/// Optional context the renderer can use to embed links back to the server.
#[derive(Debug, Clone, Default)]
pub struct RenderOpts<'a> {
    /// Absolute URL prefix for recipe links, without trailing slash. The
    /// renderer appends `/r/<slug>`. Example:
    /// `"https://recipes.example.com"` or `"https://example.com/recipes"`.
    /// `None` means: emit no source-recipe URLs.
    pub recipe_link_base: Option<&'a str>,
}

impl Format {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "text" | "txt" | "plain" => Some(Format::Text),
            "html" => Some(Format::Html),
            _ => None,
        }
    }

    pub fn content_type(self) -> &'static str {
        match self {
            Format::Text => "text/plain; charset=utf-8",
            Format::Html => "text/html; charset=utf-8",
        }
    }
}

pub fn render(list: &ShoppingList, format: Format) -> String {
    render_with(list, format, &RenderOpts::default())
}

pub fn render_with(list: &ShoppingList, format: Format, opts: &RenderOpts<'_>) -> String {
    match format {
        Format::Text => render_text(list, opts),
        Format::Html => render_html(list, opts),
    }
}

/// 1-based source index keyed by `(slug, version)`.
fn source_indices(list: &ShoppingList) -> HashMap<(String, String), usize> {
    list.sources
        .iter()
        .enumerate()
        .map(|(i, s)| ((s.slug.clone(), s.version.to_string()), i + 1))
        .collect()
}

fn item_indices(item: &LineItem, idx: &HashMap<(String, String), usize>) -> Vec<usize> {
    let mut v: Vec<usize> = item
        .sources
        .iter()
        .filter_map(|s: &SourceRef| idx.get(&(s.slug.clone(), s.version.to_string())).copied())
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// Render a list of source indices as Unicode superscript digits.
/// Multiple indices are space-separated: `¹` `² ³` etc.
fn unicode_superscript(indices: &[usize]) -> String {
    if indices.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    for (i, n) in indices.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(&digits_to_superscript(*n));
    }
    out
}

fn digits_to_superscript(mut n: usize) -> String {
    const SUPER: [char; 10] = ['⁰', '¹', '²', '³', '⁴', '⁵', '⁶', '⁷', '⁸', '⁹'];
    if n == 0 {
        return SUPER[0].to_string();
    }
    let mut digits = Vec::new();
    while n > 0 {
        digits.push(SUPER[n % 10]);
        n /= 10;
    }
    digits.reverse();
    digits.into_iter().collect()
}

/// Plain-text format: one ingredient per line, then a blank line, then a
/// numbered "From:" footer with absolute URLs to each source recipe.
/// Designed to paste into Apple Notes — each line becomes its own paragraph,
/// ⌘⇧L converts the items to a checklist, URLs auto-link, and the
/// superscripts persist verbatim. When categories are present (i.e. an
/// `aisle.conf` was loaded), each section gets a `# <Category>` heading;
/// uncategorised items collect under `# Other`.
fn render_text(list: &ShoppingList, opts: &RenderOpts<'_>) -> String {
    let mut out = String::new();
    if list.items.is_empty() {
        out.push_str("(empty)\n");
        return out;
    }
    let multi_source = list.sources.len() > 1;
    let src_idx = source_indices(list);

    let any_categorised = list.items.iter().any(|i| i.category.is_some());
    let mut current_cat: Option<String> = None;
    for item in &list.items {
        if any_categorised {
            let label = item.category.as_deref().unwrap_or("Other").to_string();
            if current_cat.as_deref() != Some(&label) {
                if current_cat.is_some() {
                    out.push('\n');
                }
                writeln!(out, "# {label}").ok();
                current_cat = Some(label);
            }
        }
        let notes = if item.notes.is_empty() {
            String::new()
        } else {
            format!(" ({})", item.notes.join("; "))
        };
        let warn = if item.warning.is_some() { " ⚠" } else { "" };
        // Only add superscripts when there's more than one source — single-source
        // lists don't need disambiguation.
        let sup = if multi_source {
            let indices = item_indices(item, &src_idx);
            let s = unicode_superscript(&indices);
            if s.is_empty() {
                String::new()
            } else {
                format!(" {s}")
            }
        } else {
            String::new()
        };
        writeln!(
            out,
            "{} {}{}{}{}",
            item.display, item.display_name, notes, warn, sup,
        )
        .ok();
    }
    if !list.sources.is_empty() {
        out.push('\n');
        out.push_str("From:\n");
        for (i, s) in list.sources.iter().enumerate() {
            let n = i + 1;
            match opts.recipe_link_base {
                Some(base) => writeln!(
                    out,
                    "[{n}] {} {} (×{} servings) — {}/r/{}",
                    s.title,
                    s.version,
                    s.servings,
                    base.trim_end_matches('/'),
                    s.slug,
                )
                .ok(),
                None => writeln!(
                    out,
                    "[{n}] {} {} (×{} servings)",
                    s.title, s.version, s.servings,
                )
                .ok(),
            };
        }
    }
    out
}

fn render_html(list: &ShoppingList, opts: &RenderOpts<'_>) -> String {
    let mut out = String::new();
    out.push_str("<h2>Shopping list</h2>\n");
    if list.items.is_empty() {
        out.push_str("<p><em>(empty)</em></p>\n");
    } else {
        let multi_source = list.sources.len() > 1;
        let src_idx = source_indices(list);

        let any_categorised = list.items.iter().any(|i| i.category.is_some());
        let mut current_cat: Option<String> = None;
        let mut in_list = false;
        for item in &list.items {
            if any_categorised {
                let label = item.category.clone().unwrap_or_else(|| "Other".to_string());
                if current_cat.as_deref() != Some(&label) {
                    if in_list {
                        out.push_str("</ul>\n");
                        in_list = false;
                    }
                    writeln!(out, r#"<h3 class="aisle">{}</h3>"#, esc(&label)).ok();
                    current_cat = Some(label);
                }
            }
            if !in_list {
                out.push_str("<ul class=\"shopping\">\n");
                in_list = true;
            }
            let warn = match &item.warning {
                Some(w) => format!(r#" <span class="warn" title="{}">⚠</span>"#, esc(w)),
                None => String::new(),
            };
            let notes = if item.notes.is_empty() {
                String::new()
            } else {
                format!(
                    r#" <span class="muted">({})</span>"#,
                    esc(&item.notes.join("; "))
                )
            };
            // Use real Unicode superscripts here — wrapping in `<sup>` would
            // normally do the same job, but the parent `<label>` is `display:
            // flex` so flex layout strips `vertical-align: super` and the
            // numerals end up on the baseline. Unicode characters render as
            // proper superscripts regardless of layout context.
            let sup = if multi_source {
                let indices = item_indices(item, &src_idx);
                let s = unicode_superscript(&indices);
                if s.is_empty() {
                    String::new()
                } else {
                    format!(r#" <span class="srcref">{}</span>"#, esc(&s))
                }
            } else {
                String::new()
            };
            writeln!(
                out,
                r#"<li><label><input type="checkbox"> <strong>{qty}</strong> {name}{notes}{warn}{sup}</label></li>"#,
                qty = esc(&item.display),
                name = esc(&item.display_name),
                notes = notes,
                warn = warn,
                sup = sup,
            )
            .ok();
        }
        if in_list {
            out.push_str("</ul>\n");
        }
    }
    if !list.sources.is_empty() {
        out.push_str("<h3>From</h3>\n<ol class=\"sources\">\n");
        for s in &list.sources {
            let title_html = match opts.recipe_link_base {
                Some(base) => format!(
                    r#"<a href="{}/r/{}">{}</a>"#,
                    esc(base.trim_end_matches('/')),
                    esc(&s.slug),
                    esc(&s.title),
                ),
                None => esc(&s.title),
            };
            writeln!(
                out,
                "<li>{title} <code>{ver}</code> (×{servings} servings)</li>",
                title = title_html,
                ver = esc(&s.version.to_string()),
                servings = s.servings,
            )
            .ok();
        }
        out.push_str("</ol>\n");
    }
    if !list.errors.is_empty() {
        out.push_str("<h3>Errors</h3>\n<ul class=\"muted\">\n");
        for e in &list.errors {
            writeln!(
                out,
                "<li><code>{}</code>: {}</li>",
                esc(&e.slug),
                esc(&e.message)
            )
            .ok();
        }
        out.push_str("</ul>\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn superscripts_single_digit() {
        assert_eq!(digits_to_superscript(1), "¹");
        assert_eq!(digits_to_superscript(7), "⁷");
    }

    #[test]
    fn superscripts_multi_digit() {
        assert_eq!(digits_to_superscript(12), "¹²");
        assert_eq!(digits_to_superscript(2026), "²⁰²⁶");
    }
}
