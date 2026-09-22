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
    Check(CheckArgs),
    /// Show new / changed / stale / untranslated counts per locale.
    #[command(after_help = STATUS_EXAMPLES)]
    Status {
        /// Only these locales (comma-separated).
        #[arg(long, value_delimiter = ',')]
        locale: Option<Vec<String>>,
        /// List the keys behind each count (everything except up-to-date), with the
        /// reason for needs-review ones. With --json, adds a `keys` map per locale.
        #[arg(short = 'k', long)]
        keys: bool,
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
    /// Add target locales to polygo.toml (e.g. `polygo add bg fr`).
    Add {
        /// BCP-47 tags: de, pt-BR, zh-Hans
        #[arg(required = true)]
        locales: Vec<String>,
    },
    /// Remove target locales from polygo.toml (files are left alone).
    Remove {
        #[arg(required = true)]
        locales: Vec<String>,
    },
    /// Pull user-facing text out of web source (JSX, HTML in template literals, .html/.vue/.svelte) into locales/en.json.
    #[command(after_help = EXTRACT_EXAMPLES)]
    Extract {
        /// Catalog to write (i18next JSON). Existing keys are kept.
        #[arg(long, default_value = "locales/en.json")]
        out: PathBuf,
        /// List what would be extracted, write nothing.
        #[arg(long)]
        dry_run: bool,
        /// Machine-readable list of hits.
        #[arg(long)]
        json: bool,
        /// Also replace the strings in .ts/.tsx/.js/.jsx files with t("key") calls and
        /// generate the i18n helper. Review with `git diff`.
        #[arg(long)]
        rewrite: bool,
        /// Where to generate the helper module (with --rewrite).
        #[arg(long, default_value = "src/i18n.ts")]
        i18n: PathBuf,
        /// Skip strings containing this word (repeatable; also `[extract] ignore` in polygo.toml).
        #[arg(long = "ignore", value_name = "WORD")]
        ignore_words: Vec<String>,
        /// Skip these paths, gitignore-style glob (repeatable; also `[extract] ignore_paths`).
        #[arg(long = "ignore-path", value_name = "GLOB")]
        ignore_paths: Vec<String>,
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
    /// Print a shell completion script (bash, zsh, fish, elvish, powershell).
    #[command(after_help = COMPLETIONS_EXAMPLES)]
    Completions { shell: clap_complete::Shell },
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
    /// Pull a missing Ollama model without asking.
    #[arg(short = 'y', long)]
    yes: bool,
}

#[derive(clap::Args)]
struct CheckArgs {
    /// A string file or a project directory to check without polygo.toml
    /// (format and locales are detected).
    path: Option<PathBuf>,
    /// Only these locales (comma-separated).
    #[arg(long, value_delimiter = ',')]
    locale: Option<Vec<String>>,
    /// Machine-readable output.
    #[arg(long)]
    json: bool,
    /// GitHub Actions output: one `::error file=…::` annotation per finding, and a
    /// table in the job summary when GITHUB_STEP_SUMMARY is set.
    #[arg(long, conflicts_with = "json")]
    github: bool,
    /// SARIF 2.1.0 on stdout, for GitHub code scanning (upload-sarif) and other tools.
    #[arg(long, conflicts_with_all = ["json", "github"])]
    sarif: bool,
    /// What a code means and what to do about it (`--explain link`); `--explain all`.
    #[arg(long, value_name = "CODE")]
    explain: Option<String>,
    /// Treat warnings as errors.
    #[arg(long)]
    strict: bool,
    /// Print every finding; by default a code with more than 20 shows 20 and a count.
    #[arg(long)]
    all: bool,
    /// Record every current finding in polygo-baseline.json; from then on only new
    /// findings are reported (and fail the exit code).
    #[arg(long, conflicts_with = "fix")]
    write_baseline: bool,
    /// Ignore polygo-baseline.json and report everything.
    #[arg(long)]
    no_baseline: bool,
    /// Re-translate keys with errors, then check again.
    #[arg(long)]
    fix: bool,
}

const EXAMPLES: &str = "\
Examples:
  polygo init                     detect the project and write polygo.toml
  polygo add bg fr                add target languages
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
  polygo translate --yes               pull a missing Ollama model without asking (CI, scripts)

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
  polygo check Localizable.xcstrings   one file, no polygo.toml needed
  polygo check app/src/main/res     a directory: format and locales are detected
  polygo check --strict             warnings (length, identical) also fail → exit 1
  polygo check --all                every finding, not the first 20 per code
  polygo check --write-baseline     accept today's findings; from now on only new ones fail
  polygo check --no-baseline        report everything, baseline or not
  polygo check --json | jq .findings
  polygo check --github             annotations + job summary in a GitHub Actions step
  polygo check --sarif > polygo.sarif   for GitHub code scanning (upload-sarif) or any SARIF viewer
  polygo check --explain punctuation    what a code means and what to do; --explain all
  polygo check --fix                re-translate the failing keys, then check again
  polygo check --locale pl,ru       only these locales

Codes: placeholders · plural · markup · escape · array (Android) · duplicate · glossary · empty · identical ·
       length · fragment · whitespace · punctuation · orphan · fuzzy (.po) · state (.xcstrings) ·
       encoding · invisible · link · brackets · entities · inconsistent
Quiet a key: polygo:ignore=code,code in its comment, or [keys] ignore in polygo.toml";

const STATUS_EXAMPLES: &str = "\
Examples:
  polygo status            counts per locale: new, stale, untranslated, edited, needs-review, up-to-date
  polygo status --keys     also list the keys behind each count, with the reason for needs-review
  polygo status -k --locale pl   one locale
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

const EXTRACT_EXAMPLES: &str = "\
Examples:
  polygo extract --dry-run        list every string it would pull out, with file:line
  polygo extract                  write locales/en.json (keys are the English text)
  polygo extract --rewrite        also replace them in .ts/.tsx with t(\"key\") and generate src/i18n.ts
  polygo extract --ignore Leafslip --ignore-path \"src/admin*\"   leave brand names and pages alone
  polygo extract --json | jq      the same list as JSON

Permanent rules go in polygo.toml:
  [extract]
  ignore = [\"Leafslip\", \"API key\"]          # strings containing these are skipped
  ignore_paths = [\"src/admin*\", \"legacy/**\"]

Looks at markup only: text between tags and placeholder/title/alt/aria-label attributes, in
JSX, HTML inside template literals, and .html/.vue/.svelte files. Skips <script>, <style>,
<svg>, <code>, tests, node_modules, dist. Interpolations become {{0}}, {{1}} placeholders.
Sentences split by inline markup (<em>, <a>, <code>) are listed as fragments and left for you.";

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

const COMPLETIONS_EXAMPLES: &str = "\
Examples:
  polygo completions zsh > ~/.zfunc/_polygo          # then `fpath+=~/.zfunc; autoload -Uz compinit; compinit` in .zshrc
  polygo completions bash > ~/.local/share/bash-completion/completions/polygo
  polygo completions fish > ~/.config/fish/completions/polygo.fish
  echo 'polygo completions powershell | Out-String | Invoke-Expression' >> $PROFILE";

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
        Commands::Check(args) => check(&cli.root, args),
        Commands::Status {
            locale,
            keys,
            json,
            markdown,
        } => status(&cli.root, locale, keys, json, markdown),
        Commands::Review { port, open } => polygo::review::serve(&cli.root, port, open),
        Commands::Add { locales } => edit_locales(&cli.root, &locales, true),
        Commands::Remove { locales } => edit_locales(&cli.root, &locales, false),
        Commands::Extract {
            out,
            dry_run,
            json,
            rewrite,
            i18n,
            ignore_words,
            ignore_paths,
        } => extract(
            &cli.root,
            ExtractArgs {
                out,
                dry_run,
                json,
                rewrite,
                i18n,
                ignore_words,
                ignore_paths,
            },
        ),
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
        Commands::Completions { shell } => {
            use clap::CommandFactory;
            clap_complete::generate(shell, &mut Cli::command(), "polygo", &mut std::io::stdout());
            Ok(())
        }
    };
    if let Err(e) = result {
        eprintln!("polygo: {e:#}");
        std::process::exit(1);
    }
}

fn translate(root: &Path, args: TranslateArgs) -> Result<()> {
    let cfg = Config::load(root)?;
    if cfg.target_locales.is_empty() {
        anyhow::bail!(
            "no target locales in polygo.toml: `polygo add de fr ja` to say which languages to translate into"
        );
    }
    if std::env::var("POLYGO_HTTP_TIMEOUT").is_err() {
        // SAFETY: single-threaded at this point; workers are spawned later.
        unsafe { std::env::set_var("POLYGO_HTTP_TIMEOUT", cfg.provider.timeout_secs.to_string()) };
    }
    if !args.dry_run {
        polygo::models::offer_pull(&cfg.provider, args.yes, &mut std::io::stderr())?;
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
        if let Some(url) = polygo::trace::ui_url() {
            println!("traces: {url}");
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

fn check(root: &Path, args: CheckArgs) -> Result<()> {
    let CheckArgs {
        path,
        locale,
        json,
        github,
        strict,
        all,
        write_baseline,
        no_baseline,
        sarif,
        explain,
        fix,
    } = args;
    if let Some(code) = explain {
        return explain_codes(&code);
    }
    if let Some(only) = &locale {
        let cfg_locales = if root.join(polygo::config::FILE_NAME).exists() && path.is_none() {
            Config::load(root)?.target_locales
        } else {
            Vec::new()
        };
        for l in only {
            if !cfg_locales.is_empty() && !cfg_locales.contains(l) {
                anyhow::bail!(
                    "locale `{l}` is not in target_locales ({})",
                    cfg_locales.join(", ")
                );
            }
        }
    }
    // With polygo.toml: the configured project. Without: whatever `path` (or the project
    // root) turns out to be, so `polygo check Localizable.xcstrings` just works. A path is
    // relative to the root (`-C`), like every other path polygo takes.
    let has_config = root.join(polygo::config::FILE_NAME).exists();
    let path = path.map(|p| if p.is_absolute() { p } else { root.join(p) });
    let (cfg, root) = match (&path, has_config) {
        (None, true) => (Config::load(root)?, root.to_path_buf()),
        (target, _) => {
            if fix {
                anyhow::bail!(if has_config {
                    "--fix works on the configured project: run `polygo check --fix` without a path"
                } else {
                    "--fix re-translates with the model from polygo.toml; run `polygo init` in the project first"
                });
            }
            if has_config && !json && !github && !sarif {
                eprintln!(
                    "note: checking {} on its own; polygo.toml settings ([keys] skip, length_ratio) do not apply",
                    target
                        .as_ref()
                        .expect("has_config implies a path")
                        .display()
                );
            }
            let (cfg, r) = polygo::init::detect_for_check(target.as_deref().unwrap_or(root))
                .map_err(|e| match target {
                    None => e.context("no polygo.toml here and nothing to check"),
                    Some(_) => e,
                })?;
            if !json && !github && !sarif {
                let what = if cfg.files.len() == 1 && target.as_ref().is_some_and(|t| t.is_file()) {
                    cfg.files[0].path.display().to_string()
                } else {
                    format!("{} detected file(s)", cfg.files.len())
                };
                eprintln!(
                    "no polygo.toml: checking {what} · locales [{}] · `polygo init` makes this permanent",
                    cfg.target_locales.join(", ")
                );
            }
            (cfg, r)
        }
    };
    // Findings name files relative to the checked root; make them relative to where the
    // user is (so GitHub annotations land on `locales/en.json`, not `en.json`), or absolute
    // when the checked tree is somewhere else entirely.
    let prefix = path.as_ref().and_then(|_| {
        let here = std::path::absolute(".").ok()?;
        let rel = root.strip_prefix(&here).unwrap_or(&root);
        (!rel.as_os_str().is_empty()).then(|| rel.to_string_lossy().replace('\\', "/"))
    });
    let root = root.as_path();
    let opts = polygo::check::run::Options {
        locales: locale.clone(),
        length_ratio: cfg.length_ratio,
    };
    let mut report = polygo::check::run::run(root, &cfg, &opts)?;
    if let Some(prefix) = &prefix {
        for f in &mut report.findings {
            f.file = format!("{prefix}/{}", f.file);
        }
    }

    if fix && report.errors > 0 {
        let keys = report.fixable_keys(&cfg.source_locale);
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

    // The baseline lives in the checked root: findings it lists are known and neither
    // shown nor counted.
    if write_baseline {
        let n = polygo::check::baseline::write(root, &report)?;
        if !json && !sarif {
            println!(
                "wrote {} with {n} finding(s); `polygo check` now reports only new ones (commit it; --no-baseline shows all)",
                root.join(polygo::check::baseline::FILE_NAME).display()
            );
        }
        return Ok(());
    }
    let applied = if no_baseline {
        None
    } else {
        polygo::check::baseline::load(root)?
            .map(|b| polygo::check::baseline::apply(&mut report, &b))
    };

    if sarif {
        println!(
            "{}",
            serde_json::to_string_pretty(&polygo::check::sarif::render(&report, strict))?
        );
    } else if json {
        let mut v = serde_json::to_value(&report)?;
        if let Some(a) = &applied {
            v["baseline"] = serde_json::json!({ "known": a.known, "stale": a.stale });
        }
        println!("{}", serde_json::to_string_pretty(&v)?);
    } else if github {
        polygo::check::github::print(&report, strict)?;
    } else if report.findings.is_empty() && report.checked == 0 {
        println!(
            "check: nothing to check yet (no translations in {}); `polygo translate` first",
            locale.unwrap_or(cfg.target_locales.clone()).join(", ")
        );
    } else if report.findings.is_empty() {
        println!(
            "check: ok ({} translation(s) in {} locale(s))",
            report.checked,
            locale.map_or(cfg.target_locales.len(), |l| l.len())
        );
    } else {
        // One code can dominate (a catalog with 3,000 strings Xcode marks needs_review):
        // show the first 20 of each code and say how many more, unless --all.
        const SHOWN: usize = 20;
        let mut seen: std::collections::BTreeMap<&str, usize> = Default::default();
        for f in &report.findings {
            let n = seen.entry(f.code.as_str()).or_default();
            *n += 1;
            if !all && *n > SHOWN {
                continue;
            }
            let key: String = f
                .key
                .chars()
                .take(60)
                .collect::<String>()
                .replace('\n', "⏎");
            let at = match f.line {
                Some(l) => format!("{}:{l}", f.file),
                None => f.file.clone(),
            };
            println!(
                "{:<7} {at}  {}  [{}]  {}: {}",
                f.severity, key, f.locale, f.code, f.message
            );
        }
        let hidden: Vec<String> = seen
            .iter()
            .filter(|(_, n)| !all && **n > SHOWN)
            .map(|(code, n)| format!("{} more {code}", n - SHOWN))
            .collect();
        if !hidden.is_empty() {
            println!(
                "… {} (first {SHOWN} of each shown; --all lists every one, --json has them all)",
                hidden.join(", ")
            );
        }
        println!("{} error(s), {} warning(s)", report.errors, report.warnings);
    }
    if !json && !github && !sarif {
        print_coverage(&report);
    }
    if let Some(a) = &applied
        && !json
        && !sarif
        && (a.known > 0 || a.stale > 0)
    {
        println!(
            "baseline: {} known finding(s) not shown{} · --no-baseline shows all, --write-baseline refreshes",
            a.known,
            if a.stale > 0 {
                format!(
                    ", {} entr{} no longer match (fixed?)",
                    a.stale,
                    if a.stale == 1 { "y" } else { "ies" }
                )
            } else {
                String::new()
            }
        );
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

fn edit_locales(root: &Path, locales: &[String], add: bool) -> Result<()> {
    let mut cfg = Config::load(root)?;
    for l in locales {
        if add {
            if l == &cfg.source_locale {
                anyhow::bail!("{l} is the source locale");
            }
            if !polygo::init::is_locale(l) {
                anyhow::bail!("{l} does not look like a locale tag (de, pt-BR, zh-Hans)");
            }
            if !cfg.target_locales.contains(l) {
                cfg.target_locales.push(l.clone());
            }
        } else if !cfg.target_locales.contains(l) {
            eprintln!("note: {l} was not a target locale");
        } else {
            cfg.target_locales.retain(|x| x != l);
        }
    }
    Config::edit(root, |doc| {
        Config::set_target_locales(doc, &cfg.target_locales)
    })?;
    println!(
        "target_locales = [{}]",
        cfg.target_locales
            .iter()
            .map(|l| format!("{l:?}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    if add {
        println!("next: `polygo translate`");
    } else if cfg.target_locales.is_empty() {
        println!("no target locales left: `polygo add <locale>` to add some");
    }
    Ok(())
}

struct ExtractArgs {
    out: PathBuf,
    dry_run: bool,
    json: bool,
    rewrite: bool,
    i18n: PathBuf,
    ignore_words: Vec<String>,
    ignore_paths: Vec<String>,
}

fn extract(root: &Path, args: ExtractArgs) -> Result<()> {
    // Rules: polygo.toml's [extract] (when there is one) plus the flags.
    let mut rules = Config::load(root).map(|c| c.extract).unwrap_or_default();
    rules.ignore.extend(args.ignore_words);
    rules.ignore_paths.extend(args.ignore_paths);
    let hits = polygo::extract::scan(root, &rules)?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&hits)?);
        return Ok(());
    }
    let mut seen = std::collections::BTreeSet::new();
    for h in &hits {
        let first = seen.insert(&h.key);
        println!(
            "{}:{}  {}{}{}",
            h.file,
            h.line,
            h.text.chars().take(90).collect::<String>(),
            if first { "" } else { "  (dup)" },
            match h.skip.as_deref() {
                Some("fragment") =>
                    "  [fragment: part of a sentence split by markup, left for you]",
                Some("markup") => "  [markup file: listed, not rewritten]",
                Some("string") => "  [inside a plain string: not rewritten]",
                _ => "",
            }
        );
    }
    if hits.is_empty() {
        println!(
            "no user-facing text found in JSX/HTML markup under {}",
            root.display()
        );
        return Ok(());
    }
    let fragments = hits
        .iter()
        .filter(|h| h.skip.as_deref() == Some("fragment"))
        .count();
    println!(
        "\n{} string(s), {} unique, {fragments} fragment(s) to handle by hand",
        hits.len(),
        seen.len()
    );
    if args.dry_run {
        println!(
            "dry run: nothing written (drop --dry-run to write {}{})",
            args.out.display(),
            if args.rewrite {
                " and rewrite the code"
            } else {
                ""
            }
        );
        return Ok(());
    }
    let (added, total) = polygo::extract::write_catalog(&root.join(&args.out), &hits)?;
    println!("wrote {} ({added} new, {total} total)", args.out.display());
    if args.rewrite {
        let r = polygo::extract::rewrite(root, &hits, &args.out, &args.i18n)?;
        println!(
            "rewrote {} string(s) in {} file(s) as t(\"key\") calls; {} left alone",
            r.replaced, r.files, r.skipped
        );
        if let Some(f) = r.i18n_file {
            println!(
                "generated {f} (t, setLocale, addCatalog; imports the catalog as JSON, so tsconfig needs resolveJsonModule)"
            );
        }
        println!(
            "review with `git diff`, then `polygo init`, `polygo add <locale>`, `polygo translate`"
        );
    } else {
        println!(
            "next: `polygo extract --rewrite` to replace them in the code with t(\"key\"), or do it by hand; then `polygo init` and `polygo translate`"
        );
    }
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
    if cfg.target_locales.is_empty() {
        println!(
            "no target locales yet: `polygo add de fr ja` (BCP-47 tags), then `polygo doctor` (checks {} {} is ready) and `polygo translate`",
            cfg.provider.kind,
            cfg.model_name()
        );
    } else {
        println!(
            "next: `polygo doctor` (checks {} {} is ready), then `polygo translate`",
            cfg.provider.kind,
            cfg.model_name()
        );
    }
    Ok(())
}

fn status(
    root: &Path,
    only: Option<Vec<String>>,
    keys: bool,
    json: bool,
    markdown: bool,
) -> Result<()> {
    let cfg = Config::load(root)?;
    let units = polygo::project::load_units(root, &cfg)?;
    let lock = Lock::load(&root.join(polygo::lockfile::FILE_NAME))?;
    if let Some(only) = &only {
        for l in only {
            if !cfg.target_locales.contains(l) {
                anyhow::bail!(
                    "locale `{l}` is not in target_locales ({})",
                    cfg.target_locales.join(", ")
                );
            }
        }
    }
    let locales: Vec<&str> = only
        .as_ref()
        .unwrap_or(&cfg.target_locales)
        .iter()
        .map(String::as_str)
        .collect();
    let status = lock.status(&units, &locales);
    // Why a key is quarantined, for the listing.
    let reason = |key: &str, locale: &str| -> Option<String> {
        lock.keys
            .get(key)
            .and_then(|r| r.review.get(locale))
            .map(|n| n.reason.clone())
    };
    let listed: Vec<State> = State::ALL
        .into_iter()
        .filter(|s| *s != State::UpToDate)
        .collect();

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
            if keys {
                let mut by_state = serde_json::Map::new();
                for s in listed.iter().copied() {
                    let ks = status.keys(l, s);
                    if ks.is_empty() {
                        continue;
                    }
                    let items: Vec<serde_json::Value> = ks
                        .iter()
                        .map(|k| match reason(k, l) {
                            Some(r) if s == State::NeedsReview => {
                                serde_json::json!({ "key": k, "reason": r })
                            }
                            _ => serde_json::Value::String(k.clone()),
                        })
                        .collect();
                    by_state.insert(s.label().replace('-', "_"), items.into());
                }
                m.insert("keys".into(), by_state.into());
            }
            per.insert((*l).to_string(), m.into());
        }
        let out = serde_json::json!({ "units": units.len(), "locales": per });
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }

    let skipped = polygo::project::skipped_keys(root, &cfg)?;
    print!("{} units · source {}", units.len(), cfg.source_locale);
    if !skipped.is_empty() {
        print!(" · {} key(s) skipped by [keys] skip", skipped.len());
    }
    println!();
    for l in &locales {
        let parts: Vec<String> = State::ALL
            .iter()
            .map(|s| format!("{}: {}", s.label(), status.count(l, *s)))
            .collect();
        println!("  {l:<8} {}", parts.join("  "));
        if !keys {
            continue;
        }
        for s in listed.iter().copied() {
            for k in status.keys(l, s) {
                let shown: String = k.chars().take(70).collect::<String>().replace('\n', "⏎");
                match reason(&k, l) {
                    Some(r) if s == State::NeedsReview => {
                        println!("    {:<13} {shown}  ·  {r}", s.label());
                    }
                    _ => println!("    {:<13} {shown}", s.label()),
                }
            }
        }
    }
    if keys
        && locales
            .iter()
            .all(|l| listed.iter().all(|s| status.count(l, *s) == 0))
    {
        println!("  everything is up to date");
    }
    Ok(())
}

/// One line when a locale is not fully translated; silence when everything is.
fn print_coverage(report: &polygo::check::run::Report) {
    let mut gaps: Vec<(&String, usize, usize)> = report
        .coverage
        .iter()
        .filter(|(_, (done, total))| done < total)
        .map(|(l, (done, total))| (l, *done, *total))
        .collect();
    if gaps.is_empty() {
        return;
    }
    // Worst first, at most six named: a 40-locale catalog must stay one line.
    gaps.sort_by_key(|(l, d, t)| (d * 100 / (*t).max(1), (*l).clone()));
    let shown: Vec<String> = gaps
        .iter()
        .take(6)
        .map(|(l, d, t)| format!("{l} {}% ({} of {t} missing)", d * 100 / (*t).max(1), t - d))
        .collect();
    let more = gaps.len().saturating_sub(6);
    println!(
        "coverage: {}{} · `polygo status --keys` lists them",
        shown.join(" · "),
        if more > 0 {
            format!(" · {more} more locale(s)")
        } else {
            String::new()
        }
    );
}

/// `polygo check --explain <code>`: the words behind a finding, from the same table the
/// SARIF rules and the README use.
fn explain_codes(which: &str) -> Result<()> {
    use polygo::check::code::Code;
    let codes: Vec<Code> = if which == "all" {
        Code::ALL.to_vec()
    } else {
        match Code::parse(which.trim().trim_start_matches("polygo/")) {
            Some(c) => vec![c],
            None => anyhow::bail!(
                "unknown code `{which}`; one of: {}",
                Code::ALL
                    .iter()
                    .map(|c| c.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    };
    for (i, c) in codes.iter().enumerate() {
        if i > 0 {
            println!();
        }
        println!("{} · {} · {} by default", c, c.title(), c.severity());
        for line in wrap(c.explain(), 88) {
            println!("  {line}");
        }
        if c.fixable() {
            println!("  `polygo check --fix` re-translates keys with this error.");
        }
    }
    Ok(())
}

fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = vec![String::new()];
    for word in text.split_whitespace() {
        let cur = lines.last_mut().unwrap();
        if !cur.is_empty() && cur.chars().count() + 1 + word.chars().count() > width {
            lines.push(word.to_string());
        } else {
            if !cur.is_empty() {
                cur.push(' ');
            }
            cur.push_str(word);
        }
    }
    lines
}
