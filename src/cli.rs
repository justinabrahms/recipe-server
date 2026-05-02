use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "recipes", version, about = "Cooklang recipe server")]
pub struct Cli {
    #[arg(
        long,
        env = "RECIPES_DIR",
        global = true,
        help = "Directory containing .cook files (recursive)"
    )]
    pub recipes_dir: Option<PathBuf>,

    #[arg(long, global = true, default_value = "info")]
    pub log_level: String,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Run the HTTP server
    Serve(ServeArgs),
    /// Parse and check files; non-zero exit on errors
    Validate(ValidateArgs),
    /// Print all recipe families and their current versions
    List,
    /// Print version table for a single recipe family
    Versions(VersionsArgs),
    /// Generate a shopping list for one or more recipes
    Shopping(ShoppingArgs),
    /// Print binary version
    Version,
}

#[derive(Debug, Args)]
pub struct ServeArgs {
    #[arg(long, default_value = "127.0.0.1:8080")]
    pub bind: String,

    #[arg(long, default_value = "/")]
    pub base_path: String,

    /// Absolute public URL of the server, e.g. `https://recipes.example.com`.
    /// Used to embed clickable links in the shopping list output. When unset,
    /// the URL is derived from request headers (`X-Forwarded-Proto`,
    /// `X-Forwarded-Host`, then `Host`).
    #[arg(long, env = "RECIPES_PUBLIC_URL")]
    pub public_url: Option<String>,
}

#[derive(Debug, Args)]
pub struct ValidateArgs {
    /// File or directory to validate (recursive for directories)
    pub path: PathBuf,
}

#[derive(Debug, Args)]
pub struct VersionsArgs {
    /// Recipe family slug
    pub slug: String,
}

#[derive(Debug, Args)]
pub struct ShoppingArgs {
    /// One or more recipes: `slug` (current version) or `slug@v1-2`
    #[arg(required = true)]
    pub recipes: Vec<String>,

    /// Output format
    #[arg(long, default_value = "text", value_parser = ["text", "html"])]
    pub format: String,

    /// Absolute base URL to embed in source-recipe links (e.g.
    /// `https://recipes.example.com`). When omitted, the text format
    /// emits source titles without URLs.
    #[arg(long)]
    pub link_base: Option<String>,

    /// Servings overrides (absolute), e.g. `--servings carbonara=4,risotto=2`
    #[arg(long)]
    pub servings: Option<String>,

    /// Repeat-batch multipliers, e.g. `--multiplier overnight-oats=3` adds
    /// three batches of overnight oats. Multiplied with the recipe's declared
    /// servings; ignored if --servings sets an absolute count for the same slug.
    #[arg(long)]
    pub multiplier: Option<String>,
}
