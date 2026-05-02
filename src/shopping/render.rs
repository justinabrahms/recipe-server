use super::ShoppingList;
use crate::server::render::esc;
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

/// Plain-text format: one ingredient per line, then a blank line, then a
/// "From:" footer with absolute URLs to each source recipe. Designed to paste
/// into Apple Notes — each line becomes its own paragraph, ⌘⇧L converts the
/// items to a checklist, and the URLs auto-link. When categories are present
/// (i.e. an `aisle.conf` was loaded), each section gets a `# <Category>`
/// heading; uncategorised items collect under `# Other`.
fn render_text(list: &ShoppingList, opts: &RenderOpts<'_>) -> String {
    let mut out = String::new();
    if list.items.is_empty() {
        out.push_str("(empty)\n");
        return out;
    }
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
        writeln!(
            out,
            "{} {}{}{}",
            item.display, item.display_name, notes, warn
        )
        .ok();
    }
    if !list.sources.is_empty() {
        out.push('\n');
        out.push_str("From:\n");
        for s in &list.sources {
            match opts.recipe_link_base {
                Some(base) => writeln!(
                    out,
                    "{} {} (×{} servings) — {}/r/{}",
                    s.title,
                    s.version,
                    s.servings,
                    base.trim_end_matches('/'),
                    s.slug,
                )
                .ok(),
                None => writeln!(out, "{} {} (×{} servings)", s.title, s.version, s.servings,).ok(),
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
            writeln!(
                out,
                r#"<li><label><input type="checkbox"> <strong>{qty}</strong> {name}{notes}{warn}</label></li>"#,
                qty = esc(&item.display),
                name = esc(&item.display_name),
                notes = notes,
                warn = warn,
            )
            .ok();
        }
        if in_list {
            out.push_str("</ul>\n");
        }
    }
    if !list.sources.is_empty() {
        out.push_str("<h3>From</h3>\n<ul class=\"muted\">\n");
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
        out.push_str("</ul>\n");
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
