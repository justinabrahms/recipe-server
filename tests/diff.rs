use recipes::diff::{render_html, unified_diff, LineKind};

#[test]
fn diff_marks_changed_lines() {
    let old = ">> title: A\n\nMix @flour{200%g}.\n";
    let new = ">> title: A\n\nMix @flour{180%g}.\n";
    let lines = unified_diff(old, new);
    assert!(lines.iter().any(|l| l.kind == LineKind::Removed));
    assert!(lines.iter().any(|l| l.kind == LineKind::Added));
}

#[test]
fn diff_html_highlights_ingredient_token() {
    let lines = unified_diff(">> title: A\n", ">> title: A\nMix @flour{180%g}.\n");
    let html = render_html(&lines);
    assert!(html.contains("class=\"diff\""));
    assert!(html.contains("class=\"tk-ing\""));
    assert!(html.contains("@flour{180%g}"));
}
