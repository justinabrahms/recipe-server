# recipes

[![ci](https://github.com/justinabrahms/recipe-server/actions/workflows/ci.yml/badge.svg)](https://github.com/justinabrahms/recipe-server/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

A self-hosted [Cooklang][cooklang] recipe viewer with bulk shopping-list
generation and per-recipe version history. One static binary, drops behind
any reverse proxy.

```
# produce
3 garlic
1 onion (diced)
a bit of parsley

# dairy
80 g butter
60 g pecorino

# pantry
200 g spaghetti
to taste black pepper

From:
Spaghetti Carbonara v2 (×2 servings) — https://recipes.example.com/r/carbonara
Garlic Bread v1 (×4 servings) — https://recipes.example.com/r/garlic-bread
```

[cooklang]: https://cooklang.org

## Features

- **Browse recipes** in a folder of `.cook` files. Categories come from the
  directory tree.
- **Per-recipe version history** by filename convention
  (`carbonara-v1.cook`, `carbonara-v1-1.cook`, `carbonara-v2.cook`), with a
  syntax-highlighted unified diff between any two versions.
- **Bulk shopping lists**: tick recipes on the home page, get a consolidated
  list with metric-normalised quantities, sectioned by store aisle.
- **Apple-Notes-friendly output**: plain text with auto-linkable absolute
  URLs in the footer; ⌘⇧L turns it into a checklist on paste.
- **Admin preview UI**: `/admin` shows index health, category coverage, and
  recipe metadata. Each recipe links to a Cooklang editor preview.
- **Live reload**: edits in the recipes directory take effect without a
  restart (500 ms debounced).
- **Stateless**: no database, no auth, no JS framework. Authn is the
  reverse proxy's job.

## Install

Pre-built static binaries for `x86_64-unknown-linux-musl` and
`aarch64-unknown-linux-musl` are attached to each release on the
[Releases page][releases].

From source (Rust 1.75+):

```sh
git clone https://github.com/justinabrahms/recipe-server
cd recipe-server
cargo build --release
./target/release/recipes --help
```

[releases]: https://github.com/justinabrahms/recipe-server/releases

## Quick start

```sh
recipes serve --recipes-dir /path/to/your/cook/files
```

then open <http://127.0.0.1:8080/>.

Admin views live at <http://127.0.0.1:8080/admin/>. The app does not
protect those routes itself; put auth in front of it with your reverse
proxy if the server is reachable by anyone else.

## CLI

```
recipes serve     [--bind 127.0.0.1:8080] [--base-path /]
                  [--public-url https://recipes.example.com]
recipes validate  <PATH>            # file or directory; non-zero exit on errors
recipes list                        # one line per family
recipes versions  <slug>            # version table for one family
recipes shopping  <slug>[@<v>] [<slug>...] [--format text|html]
                                    [--servings <slug>=<N>,<slug>=<N>]
                                    [--link-base https://recipes.example.com]
recipes version
```

The recipes directory is set via `--recipes-dir` or the `RECIPES_DIR` env
var. Public URL via `--public-url` or `RECIPES_PUBLIC_URL`. Logging level
via `--log-level trace|debug|info|warn|error` or `RUST_LOG`. Set
`LOG_FORMAT=json` for structured logs.

## Recipe format

The full Cooklang spec lives at <https://cooklang.org/docs/spec/>. The
[checklist below](#cooklang-conventions-checklist) covers the rules
`recipes validate` enforces.

### Filename / version grammar

A **recipe family** is identified by its base name. Versioned files follow
`<base>-v<MAJOR>[-<MINOR>].cook`:

```
carbonara.cook            # implicit v1
carbonara-v1-1.cook       # v1.1
carbonara-v2.cook         # v2 — current
```

The slug is the base name lowercased with spaces and underscores replaced
by `-`. Slugs must be unique across the whole tree.

### Cooklang conventions checklist

When writing or converting recipes, follow these to keep `recipes
validate` happy:

1. **Filename**: `<slug>[-v<N>[-<M>]].cook`, lowercase + hyphens only.
2. **Frontmatter**: at minimum `title`; `servings` a positive integer;
   `changelog` recommended for non-v1.
3. **Sigils are markup**: `@`, `#`, `~`, `&` never appear as prose.
   - "about" / "around" / "approx", **not** `~`.
   - "and", **not** `&`.
4. **Ingredients**: `@flour{200%g}`, `@onion{2}`, `@salt{}` (to taste).
   Multi-word names use braces: `@egg yolks{4}`. Notes:
   `@onion{1}(diced)`.
5. **Units**: metric only — `g`, `kg`, `ml`, `cl`, `dl`, `l`, `pc`. No
   imperial, no `tsp`/`tbsp`/`cup`.
6. **Timers**: always quantified — `~{10%minutes}` or
   `~rest{30%minutes}`.
7. **Cookware**: `#pan{}`, `#dutch oven{}`.
8. **Steps**: separated by blank lines.

### Example recipe

```cooklang
---
title: Spaghetti Carbonara
servings: 2
changelog: Reduced guanciale by 50g; egg whites no longer used.
---

Bring a #pot{} of salted water to the boil and add @spaghetti{200%g}.
Cook for ~{8%minutes}.

Whisk together @egg yolks{4} with @pecorino{60%g}, grated.

Render @guanciale{70%g} in a cold #pan{} until crisp, about ~{6%minutes}.

Drain the pasta, reserving @pasta water{100%ml}. Combine pasta, fat, and
egg mixture off the heat. Season with @black pepper{}.
```

## Aisle / store-section grouping

Drop an `aisle.conf` at the root of `--recipes-dir` to have shopping
lists group by section in the order you walk the store. The format is
the [Cooklang shopping list spec][aisle-spec]:

```
[produce]
onion
garlic
parsley

[bakery]
baguette
sourdough loaf

[dairy]
butter
pecorino
greek yogurt

[pantry]
olive oil
salt
```

Names are case-insensitive; ingredients you don't list go to an "Other"
bucket at the end. The watcher reloads the file on change.

[aisle-spec]: https://cooklang.org/docs/spec/#shopping-lists

## Admin UI

`GET /admin` renders a small operational view over the current index:

- index build status, root path, and error/warning counts
- recipe family, version, and category totals
- category coverage
- a recipe table with links to the public recipe and editor preview
- current index issues when parse warnings or errors exist

`GET /admin/r/<slug>/edit` renders an editor preview for the current
version of one recipe. It shows editable-looking metadata fields, the raw
Cooklang source, file facts, ingredients, and a rendered step preview.

The editor is deliberately non-mutating right now. The buttons are there
to shape the UI, but the app does not write `.cook` files or create new
versions yet. Treat it as a local design surface until save/validate
endpoints exist.

The admin routes do not implement application-level auth. The intended
deployment model is still "the reverse proxy owns auth", typically with
basic auth in Caddy or nginx.

## Reverse proxy examples

### Caddy

```caddy
recipes.example.com {
    reverse_proxy 127.0.0.1:8080
    basicauth {
        cook JDJhJDE0...   # bcrypt hash; generate with `caddy hash-password`
    }
}
```

### nginx

```nginx
server {
    listen 443 ssl;
    server_name recipes.example.com;

    auth_basic "Recipes";
    auth_basic_user_file /etc/nginx/.htpasswd-recipes;

    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
```

### Subpath deployment

To mount under a subpath, run with `--base-path /recipes` and configure
the proxy to **not** strip the prefix:

```caddy
example.com {
    handle_path /recipes/* {
        reverse_proxy 127.0.0.1:8080
    }
}
```

## Syncing recipes from Dropbox

The server reads from a local directory; sync is out of scope. Common
setups:

- **Official Dropbox client** on the host: point `--recipes-dir` at the
  synced path. Sync conflict files (`name (conflicted copy …).cook`) are
  ignored and surfaced as warnings on `/health`.
- **rclone** for headless servers:

  ```sh
  rclone bisync dropbox:family-shared/recipes/cooklang /srv/recipes \
      --resync --check-access=false
  ```

  Add a systemd timer that runs `rclone bisync` every few minutes.

## `/health`

```
GET /health
{
  "recipe_count": 42,
  "family_count": 38,
  "last_built_unix": 1717000000,
  "parse_errors": [],
  "warnings": []
}
```

Returns 503 until the first index build completes; 200 thereafter, with
all current parse errors and warnings.

## Development

```sh
cargo test                                   # unit + integration
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
```

Static assets (`static/style.css`, `static/app.js`) are bundled into the
binary at build time via `rust-embed`; rebuild after editing them.

CI runs `fmt`, `clippy`, and the test suite on every push and PR. Tagged
pushes (`v*`) trigger a release workflow that cross-builds musl
binaries for x86_64 and aarch64 and uploads them to the release.

## Contributing

Bug reports and PRs welcome. Before opening a PR:

```sh
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test
```

If you're adding a new feature, please open an issue first to discuss
the shape — this is a small project and I'd like to keep it that way.

## License

[MIT](LICENSE).
