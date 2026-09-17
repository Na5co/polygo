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
    Translate(TranslateArgs),
    /// Validate placeholders, plurals and lengths; non-zero exit on problems.
    Check {
        /// Only these locales (comma-separated).
        #[arg(long, value_delimiter = ',')]
        locale: Option<Vec<String>>,
        /// Machine-readable output.
        #[arg(long)]
        json: bool,
        /// Treat warnings as errors.
        #[arg(long)]
        strict: bool,
        /// Re-translate keys with errors, then check again.
        #[arg(long)]
        fix: bool,
    },
    /// Show new / changed / stale / untranslated counts per locale.
    Status {
        /// Machine-readable output.
        #[arg(long)]
        json: bool,
    },
    /// Open a local review page for pending translations.
    Review,
    /// Create polygo.toml by detecting the project type.
    Init {
        /// Overwrite an existing polygo.toml.
        #[arg(long)]
        force: bool,
    },
}

#[derive(clap::Args)]
struct TranslateArgs {
    /// Only these locales (comma-separated).
    #[arg(long, value_delimiter = ',')]
    locale: Option<Vec<String>>,
    /// Strings per provider call (overrides polygo.toml).
    #[arg(long)]
    batch_size: Option<usize>,
    /// Parallel provider calls (overrides polygo.toml).
    #[arg(long)]
    jobs: Option<usize>,
    /// Show what would be translated and exit.
    #[arg(long)]
    dry_run: bool,
    /// Print every translation as it is written.
    #[arg(short, long)]
    verbose: bool,
    /// Also retry keys quarantined as needs-review on a previous run.
    #[arg(long)]
    retry_review: bool,
    /// Do not attach code-usage context or similar translations to prompts.
    #[arg(long)]
    no_context: bool,
}

fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        Commands::Translate(args) => translate(&cli.root, args),
        Commands::Check {
            locale,
            json,
            strict,
            fix,
        } => check(&cli.root, locale, json, strict, fix),
        Commands::Status { json } => status(&cli.root, json),
        Commands::Review => todo_cmd("review"),
        Commands::Init { force } => init(&cli.root, force),
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

fn translate(root: &Path, args: TranslateArgs) -> Result<()> {
    let cfg = Config::load(root)?;
    let provider = polygo::provider::from_config(&cfg.provider)?;
    let opts = polygo::engine::Options {
        locales: args.locale,
        batch_size: args.batch_size.unwrap_or(cfg.batch_size),
        jobs: args.jobs.unwrap_or(cfg.jobs),
        dry_run: args.dry_run,
        verbose: args.verbose,
        retry_review: args.retry_review,
        force_keys: None,
        context: cfg.context && !args.no_context,
    };
    let report = polygo::engine::translate(root, &cfg, provider.as_ref(), &opts)?;
    if args.dry_run {
        for (l, keys) in &report.planned {
            println!("{l:<8} {} to translate", keys.len());
            for k in keys {
                println!("  {k}");
            }
        }
        return Ok(());
    }
    if report.translated == 0 {
        println!("nothing to translate — everything is up to date");
    } else {
        println!(
            "translated {} string(s) in {} batch(es) with {} ({})",
            report.translated,
            report.batches,
            provider.name(),
            provider.model()
        );
        for (l, n) in &report.per_locale {
            println!("  {l:<8} {n}");
        }
    }
    if !report.review.is_empty() {
        eprintln!(
            "{} string(s) need review (not written; see polygo.lock, retry with --retry-review):",
            report.review.len()
        );
        for (l, k, why) in &report.review {
            eprintln!("  {l}  {k}  —  {why}");
        }
        std::process::exit(3);
    }
    Ok(())
}

fn check(
    root: &Path,
    locale: Option<Vec<String>>,
    json: bool,
    strict: bool,
    fix: bool,
) -> Result<()> {
    let cfg = Config::load(root)?;
    let opts = polygo::check::run::Options {
        locales: locale.clone(),
        length_ratio: cfg.length_ratio,
    };
    let mut report = polygo::check::run::run(root, &cfg, &opts)?;

    if fix && report.errors > 0 {
        let keys = report.error_keys();
        if !keys.is_empty() {
            let provider = polygo::provider::from_config(&cfg.provider)?;
            let n: usize = keys.values().map(Vec::len).sum();
            eprintln!("re-translating {n} key(s) with errors…");
            let topts = polygo::engine::Options {
                locales: Some(keys.keys().cloned().collect()),
                batch_size: cfg.batch_size,
                jobs: cfg.jobs,
                dry_run: false,
                verbose: false,
                retry_review: false,
                force_keys: Some(keys),
                context: cfg.context,
            };
            polygo::engine::translate(root, &cfg, provider.as_ref(), &topts)?;
            report = polygo::check::run::run(root, &cfg, &opts)?;
        }
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else if report.findings.is_empty() {
        println!("check: ok");
    } else {
        for f in &report.findings {
            let key: String = f
                .key
                .chars()
                .take(60)
                .collect::<String>()
                .replace('\n', "⏎");
            println!(
                "{:<7} {}  {}  [{}]  {}: {}",
                f.severity, f.file, key, f.locale, f.code, f.message
            );
        }
        println!("{} error(s), {} warning(s)", report.errors, report.warnings);
    }
    let failed = report.errors > 0 || (strict && report.warnings > 0);
    if failed {
        std::process::exit(1);
    }
    Ok(())
}

fn init(root: &Path, force: bool) -> Result<()> {
    let path = root.join(polygo::config::FILE_NAME);
    if path.exists() && !force {
        anyhow::bail!(
            "{} already exists (use --force to overwrite)",
            path.display()
        );
    }
    let cfg = polygo::init::detect(root)?;
    std::fs::write(&path, cfg.to_toml())?;
    println!(
        "wrote {} · source {} · targets [{}] · {} file(s):",
        path.display(),
        cfg.source_locale,
        cfg.target_locales.join(", "),
        cfg.files.len()
    );
    for f in &cfg.files {
        println!(
            "  {:<10} {}",
            format!("{:?}", f.format).to_lowercase(),
            f.path.display()
        );
    }
    Ok(())
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
