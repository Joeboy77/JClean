//! Dev and test harness for `jclean-core`.
//!
//! ```text
//! jclean-cli scan --dry-run --mode quick
//! jclean-cli clean --dry-run
//! jclean-cli fixture /tmp/jclean-fixture
//! jclean-cli scan --root /tmp/jclean-fixture/root --home /tmp/jclean-fixture/root/Users/tester
//! ```

mod output;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use clap::{Args, Parser, Subcommand, ValueEnum};
use jclean_core::cancel::CancelToken;
use jclean_core::cleaner::{self, CleanContext, SystemTrash};
use jclean_core::env::{Env, Os};
use jclean_core::history::History;
use jclean_core::planner::{Selection, build_plan};
use jclean_core::platform;
use jclean_core::rules::{self, Audience, RuleSet};
use jclean_core::safety::{SafetyGuard, SystemProcesses};
use jclean_core::scanner::{ScanMode, ScanOptions, ScanResult, Scanner};
use jclean_core::testing::Fixture;
use jclean_core::tools::{CommandError, CommandOutput, CommandRunner, SystemRunner};

#[derive(Parser)]
#[command(name = "jclean-cli", version = jclean_core::VERSION, about = "JClean engine harness")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Scan and list what could be cleaned. Never changes anything.
    Scan {
        #[command(flatten)]
        scan: ScanArgs,
        /// Accepted for symmetry with `clean`; scanning never deletes.
        #[arg(long)]
        dry_run: bool,
    },
    /// Scan, build a clean plan, and carry it out (or just print it with --dry-run).
    Clean {
        #[command(flatten)]
        scan: ScanArgs,
        /// Check and log everything without changing anything.
        #[arg(long)]
        dry_run: bool,
        /// Required to actually clean.
        #[arg(long)]
        yes: bool,
        /// `preselected` (default), `safe`, or item IDs from `scan --json`.
        #[arg(long, value_delimiter = ',', default_value = "preselected")]
        select: Vec<String>,
        /// History database. Defaults to the app's own.
        #[arg(long)]
        history: Option<PathBuf>,
    },
    /// List the built-in rules, or check a rule pack.
    Rules {
        /// Validate a user rule pack as the app would import it.
        #[arg(long)]
        check: Option<PathBuf>,
    },
    /// Build a demo fixture home to scan and clean safely.
    Fixture { dir: PathBuf },
}

#[derive(Args)]
struct ScanArgs {
    #[arg(long, value_enum, default_value_t = Mode::Quick)]
    mode: Mode,
    /// Show Everyday mode (default is Developer).
    #[arg(long)]
    everyday: bool,
    /// Treat this folder as the filesystem root (for fixtures).
    #[arg(long, requires = "home")]
    root: Option<PathBuf>,
    /// Home folder to scan instead of your own.
    #[arg(long)]
    home: Option<PathBuf>,
    /// Don't ask tools like Docker or tmutil.
    #[arg(long)]
    no_probes: bool,
    /// Print JSON instead of a table.
    #[arg(long)]
    json: bool,
}

#[derive(Clone, Copy, ValueEnum)]
enum Mode {
    Quick,
    Full,
}

/// Used with `--root`: a fixture has no tools, and the real ones must not
/// run against it.
struct NoTools;

impl CommandRunner for NoTools {
    fn run(
        &self,
        program: &Path,
        _args: &[String],
        _timeout: Duration,
    ) -> Result<CommandOutput, CommandError> {
        Err(CommandError::Spawn {
            program: program.display().to_string(),
            source: std::io::Error::other("tools are disabled for fixture roots"),
        })
    }

    fn find_tool(&self, _env: &Env, _name: &str) -> Option<PathBuf> {
        None
    }
}

struct Session {
    env: Env,
    rules: RuleSet,
    runner: Box<dyn CommandRunner>,
    audience: Audience,
    fixture: bool,
}

impl Session {
    fn new(args: &ScanArgs) -> anyhow::Result<Self> {
        let (env, fixture) = match (&args.root, &args.home) {
            (Some(root), Some(home)) => (
                Env::new(absolute(home)?, absolute(root)?, Os::current()),
                true,
            ),
            (None, Some(home)) => {
                let system = Env::from_system()?;
                (Env::new(absolute(home)?, system.root(), system.os()), false)
            }
            _ => (Env::from_system()?, false),
        };
        Ok(Self {
            rules: RuleSet::builtin(env.os())?,
            runner: if fixture {
                Box::new(NoTools)
            } else {
                Box::new(SystemRunner)
            },
            audience: if args.everyday {
                Audience::Everyday
            } else {
                Audience::Developer
            },
            env,
            fixture,
        })
    }

    fn scan(&self, args: &ScanArgs) -> ScanResult {
        let mode = match args.mode {
            Mode::Quick => ScanMode::Quick,
            Mode::Full => ScanMode::Full,
        };
        let mut opts = ScanOptions::new(mode, self.audience);
        opts.run_probes = !args.no_probes && !self.fixture;
        let scanner = Scanner {
            env: &self.env,
            rules: &self.rules,
            runner: self.runner.as_ref(),
        };
        let show_progress = !args.json;
        scanner.scan(&opts, &CancelToken::new(), &|event| {
            if show_progress {
                output::progress(&event);
            }
        })
    }
}

fn absolute(path: &Path) -> anyhow::Result<PathBuf> {
    std::fs::canonicalize(path).with_context(|| format!("{} doesn't exist", path.display()))
}

fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        Command::Scan { scan, dry_run: _ } => {
            let session = Session::new(&scan)?;
            let started = Instant::now();
            let result = session.scan(&scan);
            if scan.json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                output::scan(
                    &result,
                    &session.rules,
                    &session.env,
                    session.audience,
                    started.elapsed(),
                );
            }
        }
        Command::Clean {
            scan,
            dry_run,
            yes,
            select,
            history,
        } => {
            if !dry_run && !yes {
                bail!("cleaning changes files. Add --dry-run to preview, or --yes to clean");
            }
            let session = Session::new(&scan)?;
            let result = session.scan(&scan);
            let selection = match select.as_slice() {
                [one] if one == "preselected" => Selection::Preselected,
                [one] if one == "safe" => Selection::AllSafe,
                ids => Selection::Ids(ids.to_vec()),
            };
            let plan = build_plan(
                &result,
                &session.rules,
                &selection.resolve(&result),
                &session.env,
                session.runner.as_ref(),
            );

            let history_path = history
                .unwrap_or_else(|| platform::app_data_dir(&session.env).join("history.sqlite"));
            let history = History::open(&history_path)
                .with_context(|| format!("opening {}", history_path.display()))?;
            let scan_id = history
                .record_scan(
                    result.started_at,
                    result.finished_at,
                    if result.mode == ScanMode::Full {
                        "full"
                    } else {
                        "quick"
                    },
                    result.items.iter().map(|i| i.bytes).sum(),
                    result.reclaimable(),
                    result.items.len(),
                )
                .ok();
            let guard = SafetyGuard::new(&session.env);
            let ctx = CleanContext {
                guard: &guard,
                runner: session.runner.as_ref(),
                trasher: &SystemTrash,
                processes: &SystemProcesses,
                history: Some(&history),
                scan_id,
                dry_run,
            };
            let json = scan.json;
            if !json {
                output::plan(&plan, &session.env, dry_run);
            }
            let report = cleaner::execute(&plan, &ctx, &CancelToken::new(), &|event| {
                if !json {
                    output::clean_progress(&event);
                }
            });
            if json {
                println!("{}", serde_json::json!({ "plan": plan, "report": report }));
            } else {
                output::report(&report, &history_path);
            }
        }
        Command::Rules { check } => match check {
            Some(path) => {
                let json = std::fs::read_to_string(&path)
                    .with_context(|| format!("reading {}", path.display()))?;
                let env = Env::from_system()?;
                let loaded = rules::load_custom(&path.display().to_string(), &json, &env)?;
                println!("{} rules are valid.", loaded.len());
            }
            None => output::rules(&RuleSet::builtin(Os::current())?),
        },
        Command::Fixture { dir } => {
            std::fs::create_dir_all(&dir)?;
            let f = Fixture::standard(&absolute(&dir)?)?;
            println!("Fixture home created.\n");
            println!(
                "  jclean-cli scan --root {} --home {}",
                f.root.display(),
                f.home.display()
            );
            println!(
                "  jclean-cli clean --dry-run --root {} --home {}",
                f.root.display(),
                f.home.display()
            );
        }
    }
    Ok(())
}
