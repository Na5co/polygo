use anyhow::Result;
use clap::{Parser, Subcommand};
use polygo::config::Config;
use polygo::lockfile::{Lock, State};
use std::path::{Path, PathBuf};

/// Lokalise for one person: local-first, git-native localization.
#[derive(Parser)]
#[command(name = "polygo", version, about, long_about = None, after_help = EXAMPLES)]
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
    #[command(after_help = TRANSLATE_EXAMPLES)]
    Translate(TranslateArgs),
    /// Validate placeholders, plurals and lengths; non-zero exit on problems.
    #[command(after_help = CHECK_EXAMPLES)]
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
    #[command(after_help = STATUS_EXAMPLES)]
    Status {
        /// Machine-readable output.
        #[arg(long)]
        json: bool,
    },
    /// Open a local review page for pending translations.
    #[command(after_help = REVIEW_EXAMPLES)]
    Review {
        /// Port on 127.0.0.1 (0 = pick a free one).
        #[arg(long, default_value_t = 4133)]
        port: u16,
        /// Open the page in the default browser.
        #[arg(long)]
        open: bool,
    },
    /// Check config, files, provider and model; say exactly what to fix.
    #[command(after_help = DOCTOR_EXAMPLES)]
    Doctor {
        /// Machine-readable output.
        #[arg(long)]
        json: bool,
    },
    /// Create polygo.toml by detecting the project type.
    #[command(after_help = INIT_EXAMPLES)]
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

const EXAMPLES: &str = "\
Examples:
  polygo init                     detect the project and write polygo.toml
  polygo doctor                   is the model reachable? what's missing?
  polygo translate                translate what changed since the last run
  polygo check --strict           fail CI on any placeholder, plural or length problem
  polygo status --json            machine-readable per-locale counts
  polygo review --open            approve or reject quarantined translations

Exit codes: 0 ok · 1 error (or check found problems) · 3 some strings need review
Docs: https://github.com/Na5co/polygo/tree/main/docs";

const TRANSLATE_EXAMPLES: &str = "\
Examples:
  polygo translate                     every target locale, new/changed strings only
  polygo translate --locale de,fr -v   two locales, print each translation as it lands
  polygo translate --dry-run           list the strings that would be sent, send nothing
  polygo translate --retry-review      retry strings quarantined as needs-review
  polygo translate --jobs 4            four provider calls in parallel (API providers)
  polygo translate --no-context        plain prompts without code usage / similar strings

Human-edited translations are never overwritten (state `edited` in polygo.lock).
Exit code 3 means some strings were quarantined; run `polygo review` to decide.";

const CHECK_EXAMPLES: &str = "\
Examples:
  polygo check                      placeholders, plural categories, empty/identical/length
  polygo check --strict             warnings (length, identical) also fail → exit 1
  polygo check --json | jq .findings
  polygo check --fix                re-translate the failing keys, then check again
  polygo check --locale pl,ru       only these locales

Codes: placeholders · plural · empty · identical · length · fragment";

const STATUS_EXAMPLES: &str = "\
Examples:
  polygo status            counts per locale: new, stale, untranslated, edited, needs-review, up-to-date
  polygo status --json     same as JSON, e.g. for a dashboard or a pre-push hook";

const REVIEW_EXAMPLES: &str = "\
Examples:
  polygo review --open          serve http://127.0.0.1:4133 and open the browser
  polygo review --port 0        pick a free port

Approve writes the translation into your files and records it as human-made;
reject leaves the key untranslated for the next `polygo translate --retry-review`.
The server binds to 127.0.0.1 only and rejects non-localhost Host headers.";

const DOCTOR_EXAMPLES: &str = "\
Examples:
  polygo doctor            config parses, files load, Ollama reachable, model pulled
  polygo doctor --json     the same as JSON (exit 1 when anything fails)";

const INIT_EXAMPLES: &str = "\
Examples:
  polygo init              detect .xcstrings / Android / i18next / ARB / .po / .resx layouts
  polygo init --force      overwrite an existing polygo.toml
  polygo -C ios/App init   another project root

Then edit polygo.toml to set target_locales and the provider (default: Ollama, qwen3:8b).";

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
        Commands::Review { port, open } => polygo::review::serve(&cli.root, port, open),
        Commands::Doctor { json } => {
            let checks = polygo::doctor::run(&cli.root);
            match polygo::doctor::print(&checks, json) {
                Ok(true) => Ok(()),
                Ok(false) => std::process::exit(1),
                Err(e) => Err(e),
            }
        }
        Commands::Init { force } => init(&cli.root, force),
    };
    if let Err(e) = result {
        eprintln!("polygo: {e:#}");
        std::process::exit(1);
    }
}

fn translate(root: &Path, args: TranslateArgs) -> Result<()> {
    let cfg = Config::load(root)?;
    if std::env::var("POLYGO_HTTP_TIMEOUT").is_err() {
        // SAFETY: single-threaded at this point; workers are spawned later.
        unsafe { std::env::set_var("POLYGO_HTTP_TIMEOUT", cfg.provider.timeout_secs.to_string()) };
    }
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
    println!(
        "next: `polygo doctor` (checks {} {} is ready), then `polygo translate`",
        cfg.provider.kind,
        cfg.model_name()
    );
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
