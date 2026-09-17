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
    /// A second model grades existing translations (meaning, grammar, register) and says why.
    #[command(after_help = AUDIT_EXAMPLES)]
    Audit {
        /// Only these locales (comma-separated).
        #[arg(long, value_delimiter = ',')]
        locale: Option<Vec<String>>,
        /// Judge model: `gemma4`, `qwen3:14b`, `openai/gpt-4o-mini`, `anthropic/claude-sonnet-5`…
        #[arg(long, default_value = "gemma4")]
        judge: String,
        /// Report translations scored at or below this (1 wrong … 5 native).
        #[arg(long, default_value_t = 3)]
        threshold: u8,
        /// Strings per judge call.
        #[arg(long, default_value_t = 10)]
        batch_size: usize,
        /// Only the first N translated strings per locale (0 = all).
        #[arg(long, default_value_t = 0)]
        limit: usize,
        /// Machine-readable output.
        #[arg(long)]
        json: bool,
        /// Re-translate every flagged string with the judge model, then write.
        #[arg(long)]
        fix: bool,
    },
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
        /// A coverage table for READMEs and PR comments.
        #[arg(long, conflicts_with = "json")]
        markdown: bool,
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
    /// Write a pseudo-locale ([Šéţţíñĝš ~~~]) to catch hardcoded strings and truncation.
    #[command(after_help = PSEUDO_EXAMPLES)]
    Pseudo {
        /// Locale to write (Android's pseudolocale by default).
        #[arg(long, default_value = polygo::pseudo::DEFAULT_LOCALE)]
        locale: String,
    },
    /// Translation memory: what polygo remembers across projects.
    #[command(after_help = MEMORY_EXAMPLES)]
    Memory {
        /// Forget everything (or one locale with --locale).
        #[arg(long)]
        forget: bool,
        #[arg(long)]
        locale: Option<String>,
    },
    /// List recommended models and which are pulled / active.
    Models,
    /// Pick a model: pull it through Ollama if needed, or set an API provider; writes polygo.toml.
    #[command(after_help = USE_EXAMPLES)]
    Use {
        /// `gemma4`, `qwen3:14b` (Ollama), `openai/gpt-4o-mini`, `anthropic/claude-sonnet-5`, or `openai/<model> --base-url URL`.
        spec: String,
        /// OpenAI-compatible endpoint (llama.cpp, vLLM, LM Studio, OpenRouter…).
        #[arg(long)]
        base_url: Option<String>,
        /// Store an API key in ~/.config/polygo/credentials.toml (mode 0600). Env vars still win.
        #[arg(long)]
        api_key: Option<String>,
        /// Also make this the default provider for future `polygo init`.
        #[arg(long)]
        global: bool,
        /// Fail instead of pulling a missing Ollama model.
        #[arg(long)]
        no_pull: bool,
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
  polygo use gemma4               pick a model: pulls it through Ollama, or set an API key
  polygo doctor                   is the model reachable? what's missing?
  polygo translate                translate what changed since the last run
  polygo check --strict           fail CI on any placeholder, plural or length problem
  polygo audit --locale bg        a second model grades the translations and says why
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

const AUDIT_EXAMPLES: &str = "\
Examples:
  polygo audit --locale bg                a second model (gemma4) grades every Bulgarian string
  polygo audit --judge anthropic/claude-sonnet-5 --threshold 2
  polygo audit --locale bg --fix          re-translate the flagged strings with the judge model
  polygo audit --json | jq '.[] | select(.score < 3)'

Structural checks (`polygo check`) cannot see a wrong word; this can. Exit 1 when anything is flagged.
Model-made translations that a human later edits are never touched by --fix.";

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
  polygo status --json     same as JSON, e.g. for a dashboard or a pre-push hook
  polygo status --markdown coverage table with flags, for a README or a PR comment";

const REVIEW_EXAMPLES: &str = "\
Examples:
  polygo review --open          serve http://127.0.0.1:4133 and open the browser
  polygo review --port 0        pick a free port

Approve writes the translation into your files and records it as human-made;
reject leaves the key untranslated for the next `polygo translate --retry-review`.
The server binds to 127.0.0.1 only and rejects non-localhost Host headers.";

const MEMORY_EXAMPLES: &str = "\
Examples:
  polygo memory                  entries per locale (human-confirmed vs model-made)
  polygo memory --forget         clear it
  polygo memory --forget --locale de

Stored at ~/.config/polygo/memory.toml (or $POLYGO_CONFIG_DIR). Disable per project with
`memory = false` in polygo.toml.";

const PSEUDO_EXAMPLES: &str = "\
Examples:
  polygo pseudo                  write en-XA: [Šáṽé çĥáñĝéš ~~~~] for every string, placeholders untouched
  polygo pseudo --locale qps-ploc  the .NET / Windows pseudo-locale name

Not recorded in polygo.lock and not a target locale: delete the file or the locale entry when done.";

const USE_EXAMPLES: &str = "\
Examples:
  polygo use gemma4                              pull gemma4 (~9.6 GB) and use it here
  polygo use qwen3:14b --global                  …and for every future `polygo init`
  polygo use openai/gpt-4o-mini --api-key sk-…   API model; key stored with mode 0600
  polygo use anthropic/claude-sonnet-5           uses ANTHROPIC_API_KEY from the environment
  polygo use qwen3:8b --base-url http://gpu-box:11434
  polygo use openai/llama-3.3-70b --base-url https://api.groq.com/openai/v1 --api-key gsk_…

`polygo models` lists the local models with sizes and what they are good at.";

const DOCTOR_EXAMPLES: &str = "\
Examples:
  polygo doctor            config parses, files load, Ollama reachable, model pulled
  polygo doctor --json     the same as JSON (exit 1 when anything fails)";

const INIT_EXAMPLES: &str = "\
Examples:
  polygo init              detect .xcstrings / Android / i18next / ARB / .po / .resx layouts
  polygo init --force      overwrite an existing polygo.toml
  polygo -C ios/App init   another project root

Then edit target_locales in polygo.toml, and `polygo use <model>` to pick the model
(default: Ollama qwen3:8b; `polygo models` lists the options).";

fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        Commands::Translate(args) => translate(&cli.root, args),
        Commands::Audit {
            locale,
            judge,
            threshold,
            batch_size,
            limit,
            json,
            fix,
        } => audit(
            &cli.root,
            AuditArgs {
                locale,
                judge,
                threshold,
                batch_size,
                limit,
                json,
                fix,
            },
        ),
        Commands::Check {
            locale,
            json,
            strict,
            fix,
        } => check(&cli.root, locale, json, strict, fix),
        Commands::Status { json, markdown } => status(&cli.root, json, markdown),
        Commands::Review { port, open } => polygo::review::serve(&cli.root, port, open),
        Commands::Pseudo { locale } => pseudo(&cli.root, &locale),
        Commands::Memory { forget, locale } => memory(forget, locale.as_deref()),
        Commands::Models => polygo::models::list(&cli.root, &mut std::io::stdout()),
        Commands::Use {
            spec,
            base_url,
            api_key,
            global,
            no_pull,
        } => polygo::models::use_model(
            &cli.root,
            &polygo::models::UseArgs {
                spec,
                base_url,
                api_key,
                global,
                no_pull,
            },
            &mut std::io::stdout(),
        )
        .map(|_| println!("next: `polygo doctor`, then `polygo translate`")),
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
    if report.reused > 0 {
        println!(
            "reused {} string(s) from translation memory (human-confirmed, no model call)",
            report.reused
        );
    }
    if report.translated == 0 && report.reused == 0 {
        println!("nothing to translate: everything is up to date");
    } else if report.translated > 0 {
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
            eprintln!("  {l}  {k} :  {why}");
        }
        std::process::exit(3);
    }
    Ok(())
}

struct AuditArgs {
    locale: Option<Vec<String>>,
    judge: String,
    threshold: u8,
    batch_size: usize,
    limit: usize,
    json: bool,
    fix: bool,
}

fn audit(root: &Path, args: AuditArgs) -> Result<()> {
    let cfg = Config::load(root)?;
    if std::env::var("POLYGO_HTTP_TIMEOUT").is_err() {
        // SAFETY: single-threaded here.
        unsafe { std::env::set_var("POLYGO_HTTP_TIMEOUT", cfg.provider.timeout_secs.to_string()) };
    }
    let judge_cfg = if args.judge == "project" {
        cfg.provider.clone()
    } else {
        polygo::models::parse_spec(&args.judge, None)?
    };
    let judge = polygo::provider::from_config(&judge_cfg)?;
    if judge.name() == cfg.provider.kind && judge.model() == cfg.model_name() && !args.json {
        eprintln!(
            "note: the judge is the same model that translated ({}); pick another with --judge",
            judge.model()
        );
    }
    let opts = polygo::audit::Options {
        locales: args.locale,
        threshold: args.threshold,
        batch_size: args.batch_size,
        limit: args.limit,
        context: cfg.context,
    };
    let verdicts = polygo::audit::run(root, &cfg, judge.as_ref(), &opts, |l, i, n| {
        if !args.json {
            eprint!("\r  auditing {l} {i}/{n}   ");
        }
    })?;
    if !args.json {
        eprintln!();
    }
    if args.json {
        println!("{}", serde_json::to_string_pretty(&verdicts)?);
    } else if verdicts.is_empty() {
        println!(
            "audit: nothing scored ≤ {} by {}",
            args.threshold,
            judge.model()
        );
    } else {
        for v in &verdicts {
            println!(
                "{}/5  [{}]  {}\n      {}  →  {}\n      {}",
                v.score,
                v.locale,
                v.key
                    .chars()
                    .take(70)
                    .collect::<String>()
                    .replace('\n', "⏎"),
                v.source
                    .chars()
                    .take(80)
                    .collect::<String>()
                    .replace('\n', "⏎"),
                v.translation
                    .chars()
                    .take(80)
                    .collect::<String>()
                    .replace('\n', "⏎"),
                v.issue
            );
        }
        println!(
            "{} translation(s) scored ≤ {} by {}",
            verdicts.len(),
            args.threshold,
            judge.model()
        );
    }
    if args.fix && !verdicts.is_empty() {
        // Never rewrite what a human wrote or corrected: report it, leave it.
        let units = polygo::project::load_units(root, &cfg)?;
        let lock = Lock::load(&root.join(polygo::lockfile::FILE_NAME))?;
        let locales: Vec<&str> = cfg.target_locales.iter().map(String::as_str).collect();
        let status = lock.status(&units, &locales);
        let mut keys = polygo::audit::keys_by_locale(&verdicts);
        let mut kept = 0;
        for (l, ks) in keys.iter_mut() {
            let human = status.keys(l, State::Edited);
            let before = ks.len();
            ks.retain(|k| !human.contains(k));
            kept += before - ks.len();
        }
        keys.retain(|_, ks| !ks.is_empty());
        if kept > 0 {
            eprintln!("{kept} flagged string(s) are human-edited: left alone");
        }
        let n: usize = keys.values().map(Vec::len).sum();
        eprintln!("re-translating {n} string(s) with {}…", judge.model());
        let topts = polygo::engine::Options {
            locales: Some(keys.keys().cloned().collect()),
            batch_size: cfg.batch_size,
            jobs: cfg.jobs,
            dry_run: false,
            verbose: !args.json,
            retry_review: false,
            force_keys: Some(keys),
            context: cfg.context,
        };
        let report = polygo::engine::translate(root, &cfg, judge.as_ref(), &topts)?;
        eprintln!("rewrote {} string(s)", report.translated);
    }
    if !verdicts.is_empty() && !args.fix {
        std::process::exit(1);
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

fn memory(forget: bool, locale: Option<&str>) -> Result<()> {
    let mut mem = polygo::memory::Memory::load();
    if forget {
        let n = mem.forget(locale);
        mem.save()?;
        println!("forgot {n} entr{}", if n == 1 { "y" } else { "ies" });
        return Ok(());
    }
    let counts = mem.counts();
    println!("{}", polygo::memory::path().display());
    if counts.is_empty() {
        println!(
            "empty: polygo remembers every translation it writes and every hand-edited one it sees"
        );
        return Ok(());
    }
    println!("{:<8} {:>6} {:>6}", "locale", "human", "model");
    for (l, h, m) in counts {
        if locale.is_none_or(|x| x == l) {
            println!("{l:<8} {h:>6} {m:>6}");
        }
    }
    println!(
        "human entries are reused verbatim for identical strings; model entries only as examples"
    );
    Ok(())
}

fn pseudo(root: &Path, locale: &str) -> Result<()> {
    let cfg = Config::load(root)?;
    let n = polygo::pseudo::write(root, &cfg, locale)?;
    println!("wrote {n} pseudo-localized string(s) as {locale}");
    println!("run the app in that locale: hardcoded text stays plain, tight layouts truncate.");
    if locale == polygo::pseudo::DEFAULT_LOCALE {
        println!("  iOS/macOS: scheme → Options → App Language, or `-AppleLanguages (en-XA)`");
        println!(
            "  Android:   Settings → System → Languages → English (XA), or `adb shell setprop persist.sys.locale en-XA`"
        );
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

fn status(root: &Path, json: bool, markdown: bool) -> Result<()> {
    let cfg = Config::load(root)?;
    let units = polygo::project::load_units(root, &cfg)?;
    let lock = Lock::load(&root.join(polygo::lockfile::FILE_NAME))?;
    let locales: Vec<&str> = cfg.target_locales.iter().map(String::as_str).collect();
    let status = lock.status(&units, &locales);

    if markdown {
        print!("{}", polygo::coverage::markdown(&units, &locales, &status));
        return Ok(());
    }

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
