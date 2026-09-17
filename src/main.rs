use anyhow::Result;
use clap::{Parser, Subcommand};
use polygo::config::Config;
use polygo::lockfile::{Lock, State};
use std::path::{Path, PathBuf};

/// Lokalise for one person: local-first, git-native localization.
#[derive(Parser)]
#[command(name = "polygo", version, about, long_about = None)]
struct Cli {
    /// Project root (directory containing polygo.toml). Defaults to the current directory.
    #[arg(short = 'C', long, global = true, default_value = ".")]
    root: PathBuf,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Translate new or changed strings into every target locale.
    Translate,
    /// Validate placeholders, plurals and lengths; non-zero exit on problems.
    Check,
    /// Show new / changed / stale / untranslated counts per locale.
    Status {
        /// Machine-readable output.
        #[arg(long)]
        json: bool,
    },
    /// Open a local review page for pending translations.
    Review,
    /// Create polygo.toml by detecting the project type.
    Init,
}

fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        Commands::Translate => todo_cmd("translate"),
        Commands::Check => todo_cmd("check"),
        Commands::Status { json } => status(&cli.root, json),
        Commands::Review => todo_cmd("review"),
        Commands::Init => todo_cmd("init"),
    };
    if let Err(e) = result {
        eprintln!("polygo: {e:#}");
        std::process::exit(1);
    }
}

fn todo_cmd(name: &str) -> Result<()> {
    eprintln!("polygo {name}: not implemented yet");
    std::process::exit(2);
}

fn status(root: &Path, json: bool) -> Result<()> {
    let cfg = Config::load(root)?;
    let units = polygo::project::load_units(root, &cfg)?;
    let lock = Lock::load(&root.join(polygo::lockfile::FILE_NAME))?;
    let locales: Vec<&str> = cfg.target_locales.iter().map(String::as_str).collect();
    let status = lock.status(&units, &locales);

    if json {
        let mut per = serde_json::Map::new();
        for l in &locales {
            let mut m = serde_json::Map::new();
            for s in State::ALL {
                m.insert(s.label().replace('-', "_"), status.count(l, s).into());
            }
            per.insert((*l).to_string(), m.into());
        }
        let out = serde_json::json!({ "units": units.len(), "locales": per });
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }

    println!("{} units · source {}", units.len(), cfg.source_locale);
    for l in &locales {
        let parts: Vec<String> = State::ALL
            .iter()
            .map(|s| format!("{}: {}", s.label(), status.count(l, *s)))
            .collect();
        println!("  {l:<8} {}", parts.join("  "));
    }
    Ok(())
}
