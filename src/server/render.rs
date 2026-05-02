use crate::index::{Index, RecipeFamily};
use crate::recipe;
use cooklang::model::{Content, Item};
use cooklang::Recipe;
use std::fmt::Write;

pub fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

#[derive(Clone, Copy)]
pub struct Layout<'a> {
    pub base_path: &'a str,
    pub title: &'a str,
}

impl<'a> Layout<'a> {
    pub fn url(&self, suffix: &str) -> String {
        let bp = self.base_path.trim_end_matches('/');
        if suffix.starts_with('/') {
            format!("{bp}{suffix}")
        } else {
            format!("{bp}/{suffix}")
        }
    }
}

pub fn page(layout: Layout, body: &str) -> String {
    let mut out = String::with_capacity(body.len() + 1024);
    write!(
        out,
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>{title}</title>
<link rel="stylesheet" href="{css}">
</head>
<body>
<header class="app">
  <a href="{home}">Recipes</a>
  <span class="muted">{title}</span>
</header>
<main>
{body}
</main>
<footer class="app">
  Served by <a href="https://github.com/justinabrahms/recipe-server" rel="noopener">recipe-server</a> by <a href="https://justin.abrah.ms" rel="noopener">Justin Abrahms</a>.
</footer>
<script src="{js}" defer></script>
</body>
</html>
"#,
        title = esc(layout.title),
        css = layout.url("/static/style.css"),
        js = layout.url("/static/app.js"),
        home = layout.url("/"),
        body = body,
    )
    .unwrap();
    out
}

pub fn list_page(layout: Layout, idx: &Index) -> String {
    let mut body = String::new();
    let fcount = idx.family_count();
    let extra_versions = idx.recipe_count().saturating_sub(fcount);
    let recipe_word = if fcount == 1 { "recipe" } else { "recipes" };
    let versions_note = match extra_versions {
        0 => String::new(),
        1 => " (+1 older version)".to_string(),
        n => format!(" (+{n} older versions)"),
    };
    write!(
        body,
        r#"<h1>Recipes</h1>
<p class="muted">{fcount} {recipe_word}{versions_note}.</p>
<input id="search" type="search" placeholder="Filter by title, category, ingredient…" autocomplete="off">
<form id="recipe-list-form" method="post" action="{shopping}">
<div id="recipe-list">
"#,
        fcount = fcount,
        recipe_word = recipe_word,
        versions_note = versions_note,
        shopping = esc(&layout.url("/shopping")),
    )
    .unwrap();

    if idx.families.is_empty() {
        body.push_str("<p class=\"muted\">No recipes found.</p>");
    } else {
        for (category, families) in idx.families_by_category() {
            let label = if category.is_empty() {
                "Uncategorised".to_string()
            } else {
                category.clone()
            };
            write!(
                body,
                r#"<section class="category-group">
<h2>{label}</h2>
<ul class="recipe-list">
"#,
                label = esc(&label),
            )
            .unwrap();
            for f in families {
                let cur = f.current();
                let ingredient_blob: String = cur
                    .parsed
                    .recipe
                    .ingredients
                    .iter()
                    .map(|i| i.name.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");
                let search_text = format!("{} {} {}", f.title, category, ingredient_blob);
                write!(
                    body,
                    r#"<li class="recipe" data-search-text="{search}" data-slug="{slug}">
  <input type="checkbox" name="slugs[]" value="{slug}">
  <span class="row-title"><a href="{href}">{title}</a> <span class="tag">{ver}</span></span>
  <span class="multiplier" hidden>
    <button type="button" class="step-down" aria-label="decrease">−</button>
    <output class="step-value">1</output>×
    <button type="button" class="step-up" aria-label="increase">+</button>
    <input type="hidden" name="multiplier[{slug}]" value="1" disabled>
  </span>
</li>
"#,
                    search = esc(&search_text),
                    slug = esc(f.slug.as_str()),
                    href = esc(&layout.url(&format!("/r/{}", f.slug))),
                    title = esc(&f.title),
                    ver = esc(&cur.key.to_string()),
                )
                .unwrap();
            }
            body.push_str("</ul></section>\n");
        }
    }
    body.push_str("</div>\n</form>\n");
    body.push_str(
        r#"<div id="action-bar" class="action-bar">
  <span><strong id="action-count">0</strong> selected</span>
  <button type="submit" form="recipe-list-form">Generate shopping list</button>
</div>
"#,
    );
    page(layout, &body)
}

pub fn recipe_view(layout: Layout, family: &RecipeFamily, version_idx: usize) -> String {
    let v = &family.versions[version_idx];
    let recipe = &v.parsed.recipe;
    let is_current = version_idx == family.versions.len() - 1;
    let mut body = String::new();
    write!(
        body,
        r#"<h1>{title}</h1>
<p class="muted">
  Version <strong>{ver}</strong>
  {current_note}
  · <a href="{history}">History ({count})</a>
</p>
"#,
        title = esc(&family.title),
        ver = esc(&v.key.to_string()),
        current_note = if is_current {
            "<span class=\"tag\">current</span>"
        } else {
            "<span class=\"tag\">older</span>"
        },
        history = esc(&layout.url(&format!("/r/{}/history", family.slug))),
        count = family.versions.len(),
    )
    .unwrap();

    if !is_current {
        if let Some(cl) = recipe::recipe_changelog(recipe) {
            write!(
                body,
                r#"<div class="changelog"><strong>Changelog:</strong> {}</div>"#,
                esc(&cl)
            )
            .unwrap();
        }
    }
    if version_idx > 0 {
        let prev = family.versions[version_idx - 1].key;
        write!(
            body,
            r#"<p class="muted"><a href="{}">Compare to {}</a></p>"#,
            esc(&layout.url(&format!(
                "/r/{}/diff?from={}&to={}",
                family.slug, prev, v.key
            ))),
            esc(&prev.to_string()),
        )
        .unwrap();
    }

    let servings = recipe::declared_servings(recipe);
    write!(body, r#"<p class="muted">Serves {}</p>"#, servings).unwrap();

    body.push_str("<h2>Ingredients</h2>\n<ul class=\"ingredient-list\">\n");
    let converter = crate::recipe::parse::converter();
    // `IngredientList::from_recipe` groups by name (case-sensitive) and sums
    // quantities, handling unit conversion across mentions. We pass
    // `list_references=true` so `&salt` references contribute too.
    let list = cooklang::ingredient_list::IngredientList::from_recipe(recipe, converter, true);
    let mut notes_by_name: std::collections::HashMap<&str, Vec<&str>> =
        std::collections::HashMap::new();
    for ing in &recipe.ingredients {
        if let Some(n) = ing.note.as_deref() {
            let n = n.trim();
            if !n.is_empty() {
                notes_by_name.entry(ing.name.as_str()).or_default().push(n);
            }
        }
    }
    for (name, grouped) in list.iter() {
        body.push_str("<li>");
        body.push_str(&render_named_grouped(
            name,
            grouped,
            notes_by_name.get(name.as_str()).map(|v| v.as_slice()),
        ));
        body.push_str("</li>\n");
    }
    body.push_str("</ul>\n");

    body.push_str("<h2>Steps</h2>\n");
    body.push_str(&render_steps(recipe));

    page(layout, &body)
}

pub fn history_page(layout: Layout, family: &RecipeFamily) -> String {
    let mut body = String::new();
    write!(
        body,
        r#"<h1>{title} — History</h1>
<p class="muted"><a href="{back}">← Back to recipe</a></p>
<table class="versions-table">
<thead><tr><th>Version</th><th>Modified</th><th>Changelog</th><th></th></tr></thead>
<tbody>
"#,
        title = esc(&family.title),
        back = esc(&layout.url(&format!("/r/{}", family.slug))),
    )
    .unwrap();

    let by_index: Vec<_> = family.versions.iter().enumerate().collect();
    for (i, v) in by_index.iter().rev() {
        let cl = recipe::recipe_changelog(&v.parsed.recipe).unwrap_or_default();
        let mtime = v
            .mtime
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| format_datetime(d.as_secs() as i64))
            .unwrap_or_else(|| "—".to_string());
        let diff_link = if *i > 0 {
            let prev = family.versions[*i - 1].key;
            format!(
                r#"<a href="{}">diff vs {}</a>"#,
                esc(&layout.url(&format!(
                    "/r/{}/diff?from={}&to={}",
                    family.slug, prev, v.key
                ))),
                esc(&prev.to_string()),
            )
        } else {
            String::new()
        };
        write!(
            body,
            r#"<tr>
  <td><a href="{href}">{ver}</a></td>
  <td>{mtime}</td>
  <td>{cl}</td>
  <td>{diff_link}</td>
</tr>
"#,
            ver = esc(&v.key.to_string()),
            href = esc(&layout.url(&format!("/r/{}/v/{}", family.slug, v.key))),
            mtime = esc(&mtime),
            cl = esc(&cl),
            diff_link = diff_link,
        )
        .unwrap();
    }
    body.push_str("</tbody></table>\n");
    page(layout, &body)
}

pub fn diff_page(
    layout: Layout,
    family: &crate::index::RecipeFamily,
    from_version: crate::index::scan::VersionKey,
    to_version: crate::index::scan::VersionKey,
    diff_html: &str,
) -> String {
    let mut body = String::new();
    write!(
        body,
        r#"<h1>{title}</h1>
<p class="muted">
  Comparing <strong>{from}</strong> → <strong>{to}</strong>
  · <a href="{back}">Back to recipe</a>
  · <a href="{history}">History</a>
</p>
"#,
        title = esc(&family.title),
        from = esc(&from_version.to_string()),
        to = esc(&to_version.to_string()),
        back = esc(&layout.url(&format!("/r/{}", family.slug))),
        history = esc(&layout.url(&format!("/r/{}/history", family.slug))),
    )
    .unwrap();
    body.push_str(diff_html);
    page(layout, &body)
}

pub fn shopping_page(
    layout: Layout,
    list: &crate::shopping::ShoppingList,
    token: &str,
    absolute_base: &str,
) -> String {
    use crate::shopping::render as sr;

    let mut body = String::new();
    body.push_str("<h1>Shopping list</h1>\n");
    body.push_str(r#"<p class="muted">Tip: copy as plain text, paste into Apple Notes, then ⌘⇧L to convert to a checklist.</p>"#);
    body.push('\n');

    body.push_str(r#"<div class="shopping-actions">"#);
    body.push('\n');
    body.push_str(r#"<button id="copy-text-btn" class="btn">Copy as plain text</button>"#);
    body.push_str(&format!(
        r#"<a class="btn" href="{}?format=text">View plain</a>"#,
        esc(&layout.url(&format!("/shopping/{token}")))
    ));
    body.push_str(&format!(
        r#"<a class="btn" href="{}">Home</a>"#,
        esc(&layout.url("/"))
    ));
    body.push_str("</div>\n");

    // Hidden plain-text payload for the copy button to lift verbatim.
    // Absolute URLs in the "From:" footer let pasted text in Apple Notes
    // turn the recipe references into clickable links automatically.
    let opts = sr::RenderOpts {
        recipe_link_base: Some(absolute_base),
    };
    let plain = sr::render_with(list, sr::Format::Text, &opts);
    body.push_str(&format!(
        r#"<textarea id="copy-text-source" hidden>{}</textarea>"#,
        esc(&plain)
    ));
    body.push('\n');

    // Inline-rendered HTML version with linked source recipes.
    body.push_str(&sr::render_with(list, sr::Format::Html, &opts));

    page(layout, &body)
}

pub fn not_found(layout: Layout, what: &str) -> String {
    let body = format!(
        r#"<h1>Not found</h1><p class="muted">{}</p><p><a href="{}">← Home</a></p>"#,
        esc(what),
        esc(&layout.url("/"))
    );
    page(layout, &body)
}

/// Render one entry from an `IngredientList`: a named group with summed
/// quantities (possibly multiple if units are incompatible) and an optional
/// list of preserved notes.
fn render_named_grouped(
    name: &str,
    grouped: &cooklang::quantity::GroupedQuantity,
    notes: Option<&[&str]>,
) -> String {
    let mut out = String::new();
    let parts: Vec<String> = grouped
        .iter()
        .map(|q| {
            let val = render_value(q.value());
            match q.unit() {
                Some(u) => format!("{} {}", val, u),
                None => val,
            }
        })
        .collect();
    if !parts.is_empty() {
        write!(
            out,
            r#"<span class="qty">{}</span> "#,
            esc(&parts.join(" + "))
        )
        .ok();
    }
    write!(out, r#"<span class="ingredient">{}</span>"#, esc(name)).ok();
    if let Some(ns) = notes {
        if !ns.is_empty() {
            let mut seen = std::collections::HashSet::new();
            let unique: Vec<&str> = ns.iter().filter(|n| seen.insert(**n)).copied().collect();
            write!(
                out,
                " <span class=\"muted\">({})</span>",
                esc(&unique.join("; "))
            )
            .ok();
        }
    }
    out
}

fn render_value(v: &cooklang::quantity::Value) -> String {
    use cooklang::quantity::Value;
    match v {
        Value::Number(n) => format_number(n.value()),
        Value::Range { start, end } => {
            format!(
                "{}–{}",
                format_number(start.value()),
                format_number(end.value())
            )
        }
        Value::Text(s) => s.clone(),
    }
}

fn format_number(n: f64) -> String {
    if (n - n.round()).abs() < 1e-9 {
        format!("{}", n as i64)
    } else {
        format!("{:.2}", n)
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    }
}

fn render_steps(recipe: &Recipe) -> String {
    let mut out = String::new();
    out.push_str("<ol class=\"step-list\">\n");
    for section in &recipe.sections {
        if let Some(name) = &section.name {
            writeln!(out, "<h3>{}</h3>", esc(name)).unwrap();
        }
        for content in &section.content {
            match content {
                Content::Step(step) => {
                    out.push_str("<li>");
                    for item in &step.items {
                        match item {
                            Item::Text { value } => out.push_str(&esc(value)),
                            Item::Ingredient { index } => {
                                let ing = &recipe.ingredients[*index];
                                let qty = ing
                                    .quantity
                                    .as_ref()
                                    .map(|q| {
                                        let v = render_value(q.value());
                                        match q.unit() {
                                            Some(u) => format!(" ({} {})", v, u),
                                            None => format!(" ({})", v),
                                        }
                                    })
                                    .unwrap_or_default();
                                write!(
                                    out,
                                    r#"<span class="ingredient">{}</span><span class="qty">{}</span>"#,
                                    esc(&ing.name),
                                    esc(&qty)
                                )
                                .unwrap();
                            }
                            Item::Cookware { index } => {
                                let cw = &recipe.cookware[*index];
                                write!(out, r#"<span class="cookware">{}</span>"#, esc(&cw.name))
                                    .unwrap();
                            }
                            Item::Timer { index } => {
                                let t = &recipe.timers[*index];
                                let label = match (&t.name, &t.quantity) {
                                    (Some(n), Some(q)) => format!("{} ({})", n, format_quantity(q)),
                                    (Some(n), None) => n.clone(),
                                    (None, Some(q)) => format_quantity(q),
                                    (None, None) => "(timer)".into(),
                                };
                                write!(out, r#"<span class="timer">⏲ {}</span>"#, esc(&label))
                                    .unwrap();
                            }
                            Item::InlineQuantity { index } => {
                                let q = &recipe.inline_quantities[*index];
                                out.push_str(&esc(&format_quantity(q)));
                            }
                        }
                    }
                    out.push_str("</li>\n");
                }
                Content::Text(t) => {
                    writeln!(out, "<p>{}</p>", esc(t)).unwrap();
                }
            }
        }
    }
    out.push_str("</ol>\n");
    out
}

fn format_quantity(q: &cooklang::quantity::Quantity) -> String {
    let v = render_value(q.value());
    match q.unit() {
        Some(u) => format!("{} {}", v, u),
        None => v,
    }
}

/// Format a unix timestamp as `YYYY-MM-DD HH:MM` (UTC). Civil-day arithmetic
/// without pulling in chrono.
fn format_datetime(secs: i64) -> String {
    let secs = secs.max(0);
    let days = secs / 86_400;
    let rem = secs % 86_400;
    let hours = rem / 3600;
    let minutes = (rem % 3600) / 60;
    let (y, m, d) = days_to_ymd(days);
    format!("{y:04}-{m:02}-{d:02} {hours:02}:{minutes:02}")
}

fn days_to_ymd(days_since_epoch: i64) -> (i32, u32, u32) {
    // Algorithm from Howard Hinnant's date library (date.h).
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i32 + era as i32 * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_html_meta() {
        assert_eq!(
            esc(r#"<script>alert("x" & 'y')</script>"#),
            "&lt;script&gt;alert(&quot;x&quot; &amp; &#39;y&#39;)&lt;/script&gt;"
        );
    }

    #[test]
    fn url_root_and_subpath() {
        let root = Layout {
            base_path: "/",
            title: "x",
        };
        assert_eq!(root.url("/"), "/");
        assert_eq!(root.url("/static/x.css"), "/static/x.css");

        let sub = Layout {
            base_path: "/recipes",
            title: "x",
        };
        assert_eq!(sub.url("/"), "/recipes/");
        assert_eq!(sub.url("/static/x.css"), "/recipes/static/x.css");
    }

    #[test]
    fn formats_unix_epoch() {
        assert_eq!(format_datetime(0), "1970-01-01 00:00");
        assert_eq!(format_datetime(86_400), "1970-01-02 00:00");
        // 2026-05-02 17:30 UTC ≈ 1777750200
        let dt = format_datetime(1_777_750_200);
        assert!(dt.starts_with("2026-05-02"), "got {dt}");
    }
}
