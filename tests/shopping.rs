use recipes::index::build_index;
use recipes::index::scan::VersionKey;
use recipes::shopping::render::{render, Format};
use recipes::shopping::{aggregate, Selection};

fn fixtures() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn sel(slug: &str, version: Option<VersionKey>, servings: Option<u32>) -> Selection {
    Selection {
        slug: slug.into(),
        version,
        override_servings: servings,
        ..Default::default()
    }
}

#[test]
fn aggregates_two_recipes() {
    let idx = build_index(&fixtures()).unwrap();
    let list = aggregate(
        &idx,
        &[
            sel("carbonara", None, None),
            sel("garlic-bread", None, None),
        ],
    );
    assert!(list.errors.is_empty(), "errors: {:?}", list.errors);
    let names: Vec<_> = list.items.iter().map(|i| i.display_name.clone()).collect();
    assert!(names.iter().any(|n| n == "spaghetti"));
    assert!(names.iter().any(|n| n == "baguette"));
    assert!(names.iter().any(|n| n == "butter"));
}

#[test]
fn aggregates_two_versions_of_same_recipe() {
    let idx = build_index(&fixtures()).unwrap();
    let list = aggregate(
        &idx,
        &[
            sel("carbonara", VersionKey::parse("v1"), None),
            sel("carbonara", VersionKey::parse("v2"), None),
        ],
    );
    let pecorino = list
        .items
        .iter()
        .find(|i| i.display_name == "pecorino")
        .expect("pecorino present");
    let canonical = pecorino.canonical.expect("canonical");
    assert!(
        (canonical.amount - 110.0).abs() < 1e-6,
        "expected 50+60=110g, got {}",
        canonical.amount
    );
}

#[test]
fn multiplier_scales_by_batch() {
    let idx = build_index(&fixtures()).unwrap();
    let mut s = sel("carbonara", None, None);
    s.multiplier = Some(3);
    let list = aggregate(&idx, &[s]);
    let spaghetti = list
        .items
        .iter()
        .find(|i| i.display_name == "spaghetti")
        .unwrap();
    let c = spaghetti.canonical.unwrap();
    // recipe: 200g spaghetti × 3 batches = 600g
    assert!((c.amount - 600.0).abs() < 1e-6, "got {}", c.amount);
}

#[test]
fn override_servings_wins_over_multiplier() {
    let idx = build_index(&fixtures()).unwrap();
    let mut s = sel("carbonara", None, None);
    s.multiplier = Some(3);
    s.override_servings = Some(4);
    let list = aggregate(&idx, &[s]);
    let spaghetti = list
        .items
        .iter()
        .find(|i| i.display_name == "spaghetti")
        .unwrap();
    let c = spaghetti.canonical.unwrap();
    // override_servings=4 → 200g × (4/2) = 400g; multiplier ignored
    assert!((c.amount - 400.0).abs() < 1e-6, "got {}", c.amount);
}

#[test]
fn servings_override_scales() {
    let idx = build_index(&fixtures()).unwrap();
    let list = aggregate(&idx, &[sel("carbonara", None, Some(4))]);
    let spaghetti = list
        .items
        .iter()
        .find(|i| i.display_name == "spaghetti")
        .unwrap();
    let c = spaghetti.canonical.unwrap();
    // recipe declares 2 servings, 200g spaghetti → at 4 servings, 400g.
    assert!((c.amount - 400.0).abs() < 1e-6, "got {}", c.amount);
}

#[test]
fn unknown_slug_becomes_resolution_error() {
    let idx = build_index(&fixtures()).unwrap();
    let list = aggregate(&idx, &[sel("no-such-slug", None, None)]);
    assert_eq!(list.items.len(), 0);
    assert_eq!(list.errors.len(), 1);
    assert!(list.errors[0].message.contains("no such recipe"));
}

#[test]
fn text_format_is_notes_friendly() {
    let idx = build_index(&fixtures()).unwrap();
    let list = aggregate(&idx, &[sel("garlic-bread", None, None)]);
    let text = render(&list, Format::Text);
    // Each line is one item, no leading "- ", no header.
    for line in text.lines().filter(|l| !l.is_empty()) {
        assert!(
            !line.starts_with('-') && !line.starts_with('#'),
            "line should not start with sigil: {line:?}"
        );
    }
    assert!(text.contains("baguette"));
}

#[test]
fn text_format_includes_absolute_urls_when_link_base_set() {
    use recipes::shopping::render::{render_with, RenderOpts};
    let idx = build_index(&fixtures()).unwrap();
    let list = aggregate(&idx, &[sel("carbonara", None, None)]);
    let text = render_with(
        &list,
        Format::Text,
        &RenderOpts {
            recipe_link_base: Some("https://recipes.example.com"),
        },
    );
    assert!(text.contains("From:\n"), "missing footer: {text}");
    assert!(
        text.contains("https://recipes.example.com/r/carbonara"),
        "missing absolute URL: {text}"
    );
}

#[test]
fn text_format_omits_url_when_link_base_unset() {
    let idx = build_index(&fixtures()).unwrap();
    let list = aggregate(&idx, &[sel("carbonara", None, None)]);
    let text = render(&list, Format::Text);
    assert!(text.contains("From:\n"));
    assert!(!text.contains("http"));
}

#[test]
fn aisle_conf_categorises_and_orders_items() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("aisle.conf"),
        "[produce]\n\
         garlic\n\
         parsley\n\n\
         [bakery]\n\
         baguette\n\n\
         [dairy]\n\
         butter\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("garlic-bread.cook"),
        ">> title: Garlic Bread\n>> servings: 1\n\n\
         Slice @baguette{1}.\n\n\
         Mix @butter{40%g} with @garlic{3%pc} and @parsley{2%g}.\n\n\
         Spread on bread; bake.\n",
    )
    .unwrap();

    let idx = build_index(dir.path()).unwrap();
    let list = aggregate(&idx, &[sel("garlic-bread", None, None)]);

    // Categories present
    let cats: Vec<_> = list.items.iter().map(|i| i.category.clone()).collect();
    assert!(cats.iter().any(|c| c.as_deref() == Some("produce")));
    assert!(cats.iter().any(|c| c.as_deref() == Some("bakery")));
    assert!(cats.iter().any(|c| c.as_deref() == Some("dairy")));

    // Ordering: produce before bakery before dairy (file order).
    let position_of = |target: &str| {
        list.items
            .iter()
            .position(|i| i.display_name == target)
            .unwrap_or(usize::MAX)
    };
    assert!(position_of("garlic") < position_of("baguette"));
    assert!(position_of("baguette") < position_of("butter"));
}

#[test]
fn unknown_ingredient_falls_into_other() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("aisle.conf"), "[produce]\nonion\n").unwrap();
    std::fs::write(
        dir.path().join("dish.cook"),
        ">> title: D\n>> servings: 1\n\nMix @onion{1} with @mystery_powder{1%g}.\n",
    )
    .unwrap();

    let idx = build_index(dir.path()).unwrap();
    let list = aggregate(&idx, &[sel("dish", None, None)]);
    let onion = list
        .items
        .iter()
        .find(|i| i.display_name == "onion")
        .unwrap();
    let mystery = list
        .items
        .iter()
        .find(|i| i.display_name == "mystery_powder")
        .unwrap();
    assert_eq!(onion.category.as_deref(), Some("produce"));
    assert!(
        mystery.category.is_none(),
        "mystery should be uncategorised"
    );
}

#[test]
fn no_aisle_conf_means_no_categories() {
    let idx = build_index(&fixtures()).unwrap();
    assert!(idx.aisle_source.is_none());
    let list = aggregate(&idx, &[sel("carbonara", None, None)]);
    assert!(list.items.iter().all(|i| i.category.is_none()));
}

#[test]
fn aisle_text_render_emits_section_headings() {
    use recipes::shopping::render::{render, Format};
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("aisle.conf"),
        "[produce]\nonion\n\n[bakery]\nbaguette\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("d.cook"),
        ">> title: D\n>> servings: 1\n\nMix @onion{1} with @baguette{1}.\n",
    )
    .unwrap();
    let idx = build_index(dir.path()).unwrap();
    let list = aggregate(&idx, &[sel("d", None, None)]);
    let text = render(&list, Format::Text);
    assert!(text.contains("# produce\n"), "text missing heading: {text}");
    assert!(text.contains("# bakery\n"), "text missing heading: {text}");
    let produce_pos = text.find("# produce").unwrap();
    let bakery_pos = text.find("# bakery").unwrap();
    assert!(produce_pos < bakery_pos, "wrong section order: {text}");
}

#[test]
fn small_mass_demotes_to_a_bit() {
    // 2g parsley is well under the 5g threshold → "a bit of".
    let idx = build_index(&fixtures()).unwrap();
    let list = aggregate(&idx, &[sel("garlic-bread", None, None)]);
    let parsley = list
        .items
        .iter()
        .find(|i| i.display_name == "parsley")
        .expect("parsley present");
    assert!(
        parsley.canonical.is_none(),
        "parsley should be demoted from mass-dim"
    );
    assert_eq!(parsley.display, "a bit of");
}

#[test]
fn small_mass_merges_with_to_taste_sibling() {
    // Build a tiny tree on the fly: one recipe has 2g salt, another has
    // unitless salt (to taste). They should collapse to one line item.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("a.cook"),
        ">> title: A\n>> servings: 1\n\nMix @salt{2%g} into @flour{200%g}.\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("b.cook"),
        ">> title: B\n>> servings: 1\n\nSeason with @salt{} to taste.\n",
    )
    .unwrap();
    let idx = build_index(dir.path()).unwrap();
    let list = aggregate(&idx, &[sel("a", None, None), sel("b", None, None)]);
    let salt_lines: Vec<_> = list
        .items
        .iter()
        .filter(|i| i.display_name == "salt")
        .collect();
    assert_eq!(
        salt_lines.len(),
        1,
        "salt should be one merged line: {:#?}",
        list.items
    );
    // After merging a <5g mass with an unitless sibling, the unified label is
    // "a bit of" — that's what the trivial-amount path produces.
    assert_eq!(salt_lines[0].display, "a bit of");
    assert_eq!(salt_lines[0].sources.len(), 2);
}
