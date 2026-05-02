# Cooklang Recipe Server — Implementation Plan

A small, self-hosted Cooklang recipe viewer with bulk shopping list generation and per-recipe version history. Single static binary, deployable on a small server behind a reverse proxy (Caddy, nginx, Traefik).

-----

## 1. Goals

- Single static binary deploy — drop on a server, point at a folder of `.cook` files, run.
- Read recipes from a configurable directory; reflect file changes without restart.
- Validate `.cook` files via CLI (suitable for pre-commit / CI).
- Web UI: list recipes, view a recipe, multi-select recipes and generate a consolidated grocery list.
- Per-recipe version history, derived from filename conventions (`food-v1.cook`, `food-v1-1.cook`, `food-v2.cook`), with a diff view between versions.
- Normalize ingredient quantities (metric) and aggregate across selected recipes.
- Export grocery list as plain text, markdown, or printable HTML.
- Stateless — no database, no auth (relies on reverse proxy for access control).

## 2. Non-Goals

- In-app recipe editing. Recipes are edited externally via Claude in Dropbox; the server is read-only.
- Direct Dropbox API integration. The server reads from a local folder kept in sync by `rclone` or the official Dropbox client; sync is out of scope for this binary.
- User accounts, sharing, social features.
- Imperial units, unit toggles, locale-aware formatting.
- Mobile-native apps.
- Importing from other recipe formats.
- Git integration.

## 3. Stack

|Concern         |Choice                                                    |
|----------------|----------------------------------------------------------|
|Language        |Rust (stable, edition 2021+)                              |
|Cooklang parsing|[`cooklang`](https://crates.io/crates/cooklang) crate     |
|Web framework   |`axum`                                                    |
|Async runtime   |`tokio`                                                   |
|CLI             |`clap` (derive)                                           |
|Templates       |`askama` (compile-time; no runtime template files to ship)|
|Static assets   |`rust-embed` (bundle CSS/JS into the binary)              |
|File watching   |`notify` (debounced)                                      |
|Diff            |`similar`                                                 |
|Logging         |`tracing` + `tracing-subscriber`                          |
|Errors          |`anyhow` (app), `thiserror` (library boundaries)          |
|Config          |CLI flags + env vars; no config file required             |

Build a true static binary. Release targets: `x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl`. CI should produce stripped binaries and SHA256 sums.

## 4. Recipe Source & Versioning

### 4.1 File layout

Recipes live in a directory tree rooted at `--recipes-dir`. Subdirectories are flattened into category strings (`mains/pasta/carbonara-v1.cook` → category `mains/pasta`). All `.cook` files anywhere under the root are picked up.

### 4.2 Version filename convention

A recipe family is identified by its **base name**. Versioned files follow the pattern:

```
<base>-v<MAJOR>[-<MINOR>].cook
```

Examples for one recipe family `carbonara`:

```
carbonara-v1.cook
carbonara-v1-1.cook
carbonara-v1-2.cook
carbonara-v2.cook
```

Parsing rules:

- A trailing `-v<N>` or `-v<N>-<M>` segment, with `N` and `M` as positive integers, marks the version. Anything before it is the base name.
- A file with no version suffix (e.g. `carbonara.cook`) is treated as a single-version family with implicit version `v1`.
- Sort order within a family: by `(MAJOR, MINOR)` ascending; missing minor sorts as `0`. The highest version is **current**.
- Two files with the same `(base, MAJOR, MINOR)` is an error surfaced by `validate`.

### 4.3 Logical recipe model

The server groups files by base name into a `RecipeFamily`:

```
RecipeFamily {
    slug: "carbonara",
    title: <from current version's `>> title:` metadata, falling back to base name>,
    category: "mains/pasta",
    versions: [Version { v: (1,0), path, parsed }, Version { v: (1,1), ... }, ...],
    current: &versions[last],
}
```

The slug is the base name, lowercased, with spaces and underscores replaced by `-`. Slugs must be unique across the whole tree; collisions are a `validate` error.

### 4.4 Changelog

Cooklang supports `>> key: value` metadata lines. Each version file may include:

```
>> title: Spaghetti Carbonara
>> changelog: Reduced guanciale by 50g; egg whites no longer used.
```

The `changelog` value is free-form prose describing what changed in *that version* relative to the previous one. The first version typically has no changelog or “Initial version.”

The server displays the changelog above the recipe body when viewing a non-latest version, and in the version history list. No structure is required — whatever Claude writes is shown verbatim.

### 4.5 Diff view

For any two versions in a family, the server renders a unified diff of the raw `.cook` source using the `similar` crate. The UI offers a “compare with previous” link on every version page and a version picker for arbitrary pairs.

## 5. CLI

```
recipes serve     --recipes-dir <PATH> [--bind 127.0.0.1:8080] [--base-path /]
recipes validate  <PATH>                          # file or directory, recursive
recipes list      --recipes-dir <PATH>            # prints slug + current version + title
recipes versions  <slug> --recipes-dir <PATH>     # prints version table for one family
recipes shopping  <slug>[@<version>] [<slug>...] --recipes-dir <PATH>
                  [--format text|md|html] [--servings <slug>=<N>,...]
recipes version                                   # binary version
```

Global flags: `--log-level <trace|debug|info|warn|error>`, `--recipes-dir <PATH>` (also via `RECIPES_DIR` env var).

`validate` exit codes:

- `0` — all files parse and family/version invariants hold
- `1` — parse errors or invariant violations (slug collision, duplicate versions, malformed version suffix)
- `2` — IO or usage errors

`validate` output, one line per file:

```
OK    path/to/file.cook
FAIL  path/to/file.cook: <message>
```

`shopping` slug syntax: `<slug>` selects the current version; `<slug>@v1-1` selects a specific version. `--servings carbonara=2` overrides the recipe’s declared servings for scaling.

## 6. Architecture

### 6.1 In-memory index

On startup, walk `--recipes-dir`, parse every `.cook` file, group into families. Hold the result in an `Arc<RwLock<Index>>`:

```rust
struct Index {
    families: HashMap<Slug, RecipeFamily>,
    by_path: HashMap<PathBuf, (Slug, Version)>,
    errors: Vec<IndexError>,   // surfaced on /health
}
```

Parsing is cheap; parse eagerly at index time and cache the AST. Memory cost for a few thousand recipes is negligible.

### 6.2 File watching

Use `notify` with a 500ms debounce. On any change in the recipes directory:

1. Re-parse the affected file(s).
1. Recompute the family they belong to.
1. Swap into the index under write lock.

Sync clients (rclone, Dropbox) often write files in bursts; debounce avoids thrashing. If a parse fails, keep the previous good version of the family in the index and record the error for `/health`.

### 6.3 Routes

```
GET  /                                Recipe list (grouped by category, search box)
GET  /r/<slug>                        Latest version of a recipe
GET  /r/<slug>/v/<version>            Specific version
GET  /r/<slug>/history                Version list with changelogs
GET  /r/<slug>/diff?from=<v>&to=<v>   Unified diff between two versions
POST /shopping                        Generate grocery list from selected recipes
GET  /shopping/<token>                Render previously-generated list (?format=text|md|html)
GET  /health                          Index status, parse errors, file count
GET  /static/*                        Embedded CSS/JS (rust-embed)
```

`POST /shopping` accepts form-encoded `slugs[]` (with optional `@version`) and `servings[<slug>]` overrides, generates the list server-side, stores it in an in-memory LRU keyed by a short random token, and 303-redirects to `/shopping/<token>`. This keeps URLs shareable within the session and avoids huge query strings. LRU size: 256 entries, 1-hour TTL.

### 6.4 Reverse proxy

The server binds to `127.0.0.1:<port>` by default. Caddy example:

```
recipes.example.com {
    reverse_proxy 127.0.0.1:8080
    basicauth {
        cook JDJhJDE0...   # bcrypt hash
    }
}
```

`--base-path` lets the app live under a subpath (e.g. `/recipes`) when the proxy doesn’t strip prefixes.

## 7. Unit Normalization & Shopping List

### 7.1 Canonical units

All ingredient quantities normalize to a canonical metric unit per dimension before aggregation:

|Dimension|Canonical        |Accepted aliases                                                     |
|---------|-----------------|---------------------------------------------------------------------|
|Mass     |gram (`g`)       |`g`, `gram`, `grams`, `kg`, `kilogram`, `kilograms`                  |
|Volume   |millilitre (`ml`)|`ml`, `millilitre`, `millilitres`, `cl`, `dl`, `l`, `litre`, `litres`|
|Count    |piece (`pc`)     |`pc`, `piece`, `pieces`, no unit specified                           |

Conversions: `1 kg = 1000 g`; `1 l = 1000 ml`; `1 dl = 100 ml`; `1 cl = 10 ml`. Imperial units in source files are an error caught by `validate`.

### 7.2 Aggregation

For each selected recipe and version, scale each ingredient by `requested_servings / declared_servings` (declared servings comes from `>> servings:` metadata, defaulting to `1` if missing).

Group ingredients by `(normalized_name, dimension)` where `normalized_name` is the ingredient name lowercased and trimmed. Sum quantities within a group.

Render with a “best display unit” rule: mass ≥ 1000 g shown as kg with 2 decimals; volume ≥ 1000 ml shown as l; otherwise canonical unit. Counts shown as integers when whole, otherwise 1 decimal.

Ingredients with **incompatible dimensions** under the same name (e.g. `@salt{1%tsp}` and `@salt{5%g}`) are listed as separate line items with a small warning glyph; the UI shows a tooltip explaining the mismatch.

### 7.3 Note preservation

If multiple recipes specify prep notes for the same ingredient (`@onion{2}(diced)` vs `@onion{1}(sliced)`), aggregate the quantity and list both notes: `3 onions (diced; sliced)`.

### 7.4 Output formats

- **text**: plain UTF-8, one ingredient per line, grouped by category if categories are present in metadata.
- **markdown**: `## Shopping list` header, `- [ ] 200 g flour` checklist items, recipe attribution footer.
- **html**: minimal printable HTML with `@media print` styles; no JS.

## 8. UI

Server-rendered HTML with progressive enhancement. No SPA. Vanilla JS only for: (a) the multi-select checkboxes on the list page, (b) the “generate shopping list” button posting selected slugs.

### 8.1 Pages

**Recipe list (`/`)**

- Search box (client-side filter over rendered list; titles, categories, ingredients).
- Category groupings, collapsible.
- Each row: checkbox, title, current version label, last-modified date.
- Sticky bottom bar appears when ≥1 checkbox is checked: “Generate shopping list (3 recipes)” → POSTs to `/shopping`.

**Recipe view (`/r/<slug>`)**

- Title, current version label (e.g. “v2.1”).
- Servings, total time.
- Ingredients list (with links to scale).
- Step-by-step instructions with Cooklang’s inline ingredient/cookware/timer rendering.
- Sidebar: “Versions” with link to history, “Add to shopping list” button.

**Version history (`/r/<slug>/history`)**

- Table: version, date (file mtime), changelog text, links to view & diff against previous.

**Diff view (`/r/<slug>/diff`)**

- Unified diff with light syntax highlighting for `>> metadata`, `@ingredients`, `#cookware`, `~timers`.
- Header: “Comparing v1.1 → v2”.

**Shopping list (`/shopping/<token>`)**

- Aggregated list, grouped by category if available.
- Format toggle: plain / markdown / printable.
- “Copy to clipboard” and “Print” buttons.
- Footer listing source recipes with version pinning, so the URL is reproducible-ish even though the token is ephemeral.

### 8.2 Visual style

Simple. System font stack. Single CSS file (~200 lines). Light mode by default; respect `prefers-color-scheme: dark`. No frameworks. Target mobile-first; everything works at 360px wide.

## 9. Validation Rules

`validate` enforces:

- Each `.cook` file parses with the `cooklang` crate without errors.
- Every ingredient has a parseable quantity-and-unit, or no unit (counted item).
- Every unit is a recognised metric alias (see §7.1) or absent.
- Filename matches `<base>[-v<N>[-<M>]].cook`.
- No duplicate `(base, MAJOR, MINOR)` triples across the tree.
- No two distinct base names produce the same slug.
- `>> servings:` if present, is a positive integer.

Warnings (non-failing, printed to stderr):

- Recipe missing `>> title:` (falls back to filename).
- Non-current version missing `>> changelog:`.
- Ingredient name appears with mismatched dimensions across versions in the same family.

## 10. Error Handling & Observability

- Structured JSON logs to stdout via `tracing-subscriber` when `LOG_FORMAT=json`, human-readable otherwise.
- `/health` returns `200` with JSON: `{ recipe_count, family_count, parse_errors: [...], last_index_at }`. Returns `503` if the index has never been built.
- Per-request access log line at `info` level: method, path, status, duration.
- Panics caught by axum middleware; respond `500` with a request ID for correlation.

## 11. Project Layout

```
recipes/
├── Cargo.toml
├── README.md
├── PLAN.md
├── src/
│   ├── main.rs                # CLI entry, dispatches to subcommands
│   ├── cli.rs                 # clap definitions
│   ├── config.rs
│   ├── index/
│   │   ├── mod.rs             # Index, RecipeFamily, Version types
│   │   ├── scan.rs            # directory walk + filename parsing
│   │   ├── watch.rs           # notify integration
│   │   └── slug.rs
│   ├── recipe/
│   │   ├── mod.rs             # parsed recipe model
│   │   ├── parse.rs           # wraps cooklang crate
│   │   └── units.rs           # normalization + aggregation
│   ├── shopping/
│   │   ├── mod.rs             # aggregation
│   │   └── render.rs          # text/md/html outputs
│   ├── diff.rs
│   ├── server/
│   │   ├── mod.rs
│   │   ├── routes.rs
│   │   ├── templates.rs       # askama templates wired here
│   │   └── assets.rs          # rust-embed
│   └── commands/
│       ├── serve.rs
│       ├── validate.rs
│       ├── list.rs
│       ├── versions.rs
│       └── shopping.rs
├── templates/                 # askama .html files
├── static/                    # css, minimal js
├── tests/
│   ├── fixtures/              # sample .cook files including version families
│   ├── parse.rs
│   ├── units.rs
│   ├── shopping.rs
│   └── http.rs
└── .github/workflows/
    └── release.yml            # cross-compile musl binaries, sha256, GH release
```

## 12. Milestones

1. **M1 — Parse & validate.** Project skeleton, `cooklang` integration, filename version parsing, `validate` and `list` commands. Test fixtures for valid/invalid trees.
1. **M2 — Index & watch.** In-memory index, `notify` watcher with debounce, slug rules, error surfacing.
1. **M3 — Web UI read-only.** `serve` command, list page, recipe view, version history page. Static assets bundled.
1. **M4 — Units & shopping.** Unit normalization module + tests, `shopping` CLI command, `POST /shopping` route, three output formats.
1. **M5 — Diff view.** `diff.rs` using `similar`, diff UI page, lightweight Cooklang-aware syntax highlighting.
1. **M6 — Polish & release.** Mobile CSS pass, `/health` endpoint, GitHub Actions release workflow producing musl binaries, README with Caddy/nginx examples and rclone setup.

## 13. Testing Strategy

- Unit tests for filename parsing, unit normalization, ingredient aggregation, slug derivation.
- Snapshot tests (`insta`) for shopping list rendering across formats and for diff output.
- Integration tests using `axum::Router` directly with `tower::ServiceExt`, against a tempdir of fixture recipes.
- A fixture recipe family `carbonara-v1`, `carbonara-v1-1`, `carbonara-v2` exercising the version logic end-to-end.
- `validate` exit codes covered by a CLI integration test.

## 14. Open Questions

Deferred but worth flagging before implementation starts:

1. **Search.** Client-side filtering is fine up to a few hundred recipes. If the corpus grows, consider server-side full-text via `tantivy` — but this adds binary size meaningfully.
1. **Image attachments.** Cooklang allows `image:` metadata. For v1, ignore; for v2, optionally serve sibling images (`carbonara-v1.jpg`).
1. **Ingredient synonyms.** “Spring onion” vs “scallion” don’t aggregate. A `synonyms.toml` in the recipes dir could solve this later; punt for v1.
1. **Sync conflict files.** Dropbox creates `(conflicted copy)` files. The indexer should ignore filenames containing `conflicted copy` and surface them in `/health` as warnings.
