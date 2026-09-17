use clap::{Parser, Subcommand};

/// Lokalise for one person: local-first, git-native localization.
#[derive(Parser)]
#[command(name = "polygo", version, about, long_about = None)]
struct Cli {
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
    Status,
    /// Open a local review page for pending translations.
    Review,
    /// Create polygo.toml by detecting the project type.
    Init,
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Translate => todo_cmd("translate"),
        Commands::Check => todo_cmd("check"),
        Commands::Status => todo_cmd("status"),
        Commands::Review => todo_cmd("review"),
        Commands::Init => todo_cmd("init"),
    }
}

fn todo_cmd(name: &str) {
    eprintln!("polygo {name}: not implemented yet");
    std::process::exit(2);
}
