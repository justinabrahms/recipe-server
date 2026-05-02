use anyhow::Result;
use clap::Parser;
use recipes::cli::{Cli, Command};
use recipes::commands;

fn main() -> Result<()> {
    let cli = Cli::parse();
    init_tracing(&cli.log_level);

    match cli.command {
        Command::Serve(args) => commands::serve::run(cli.recipes_dir, args),
        Command::Validate(args) => commands::validate::run(args),
        Command::List => commands::list::run(cli.recipes_dir),
        Command::Versions(args) => commands::versions::run(cli.recipes_dir, args),
        Command::Shopping(args) => commands::shopping::run(cli.recipes_dir, args),
        Command::Version => {
            println!("{}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
    }
}

fn init_tracing(level: &str) {
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(format!("recipes={level},tower_http=info")));

    let json = std::env::var("LOG_FORMAT").ok().as_deref() == Some("json");
    let registry = tracing_subscriber::registry().with(filter);
    if json {
        registry.with(fmt::layer().json()).init();
    } else {
        registry.with(fmt::layer().with_target(false)).init();
    }
}
