//! `agentclean` command-line interface.
//!
//! Every command that changes anything is a dry run unless `--execute` is
//! given. Cleaning moves folders into a quarantine; nothing is deleted until a
//! run is purged.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use agentclean_core::guard::{Approved, Guard};
use agentclean_core::quarantine::{self, Manifest, State, Store};
use agentclean_core::rules::{self, Class, RuleFile};
use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "agentclean",
    version,
    about = "Reclaim disk space from developer projects, safely"
)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Args)]
struct StoreArg {
    /// Quarantine folder [default: the platform data folder]
    #[arg(long, value_name = "DIR")]
    store: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Validate a directory of rule files
    RulesCheck {
        /// Directory containing *.toml rule files
        dir: PathBuf,
    },
    /// Show what the rules and the safety guard would allow in one project (read-only)
    Plan {
        /// Directory containing *.toml rule files
        #[arg(long)]
        rules: PathBuf,
        /// Project root
        project: PathBuf,
    },
    /// Move rebuildable folders into quarantine (dry run unless --execute)
    Clean {
        /// Directory containing *.toml rule files
        #[arg(long)]
        rules: PathBuf,
        /// Also move Caution folders, which need a reinstall (node_modules, .venv)
        #[arg(long)]
        include_caution: bool,
        /// Actually move the folders
        #[arg(long)]
        execute: bool,
        #[command(flatten)]
        store: StoreArg,
        /// Project roots
        #[arg(required = true)]
        projects: Vec<PathBuf>,
    },
    /// List clean runs, newest first
    History {
        #[command(flatten)]
        store: StoreArg,
    },
    /// Move a run's folders back where they came from
    Restore {
        /// Run id, as shown by `history`
        run: String,
        /// Restore only this item number
        #[arg(long)]
        item: Option<usize>,
        #[command(flatten)]
        store: StoreArg,
    },
    /// Permanently delete quarantined folders to free the space (dry run unless --execute)
    Purge {
        /// Run id, as shown by `history`
        #[arg(
            required_unless_present = "older_than_days",
            conflicts_with = "older_than_days"
        )]
        run: Option<String>,
        /// Purge every run at least this many days old
        #[arg(long, value_name = "DAYS")]
        older_than_days: Option<u64>,
        /// Actually delete
        #[arg(long)]
        execute: bool,
        #[command(flatten)]
        store: StoreArg,
    },
}

type CliResult = Result<ExitCode, String>;

fn main() -> ExitCode {
    let result = match Cli::parse().command {
        Cmd::RulesCheck { dir } => rules_check(&dir),
        Cmd::Plan { rules, project } => plan(&rules, &project),
        Cmd::Clean {
            rules,
            include_caution,
            execute,
            store,
            projects,
        } => clean(&rules, &projects, include_caution, execute, &store),
        Cmd::History { store } => history(&store),
        Cmd::Restore { run, item, store } => restore(&run, item, &store),
        Cmd::Purge {
            run,
            older_than_days,
            execute,
            store,
        } => purge(run.as_deref(), older_than_days, execute, &store),
    };
    result.unwrap_or_else(|e| {
        eprintln!("error: {e}");
        ExitCode::FAILURE
    })
}

fn load_rules(dir: &Path) -> Result<Vec<RuleFile>, String> {
    rules::load_dir(dir).map_err(|e| e.to_string())
}

fn open_store(arg: &StoreArg) -> Result<Store, String> {
    let root = match &arg.store {
        Some(p) => p.clone(),
        None => quarantine::default_location()
            .ok_or("cannot find a home folder for the quarantine; pass --store")?,
    };
    Store::open(&root).map_err(|e| e.to_string())
}

fn rules_check(dir: &Path) -> CliResult {
    let loaded = load_rules(dir)?;
    let artifacts: usize = loaded.iter().map(|r| r.artifacts.len()).sum();
    println!("{} rule files, {} artifacts: ok", loaded.len(), artifacts);
    Ok(ExitCode::SUCCESS)
}

/// One candidate's outcome in one project.
enum Verdict {
    Allowed(Approved),
    Refused(String),
    CautionHeld,
}

fn assess(
    rules: &[RuleFile],
    guard: &Guard,
    project: &Path,
    caution: bool,
) -> Vec<(String, Verdict)> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for rule in rules {
        for candidate in rule.candidates(project) {
            // Two rules can claim one folder (Cargo and Maven both use target).
            if !seen.insert(candidate.rel().to_path_buf()) {
                continue;
            }
            let label = format!("{}  [{}]", candidate.rel().display(), candidate.rule_id());
            let verdict = match guard.check(project, &candidate) {
                Err(why) => Verdict::Refused(why.to_string()),
                Ok(_) if candidate.class() == Class::Caution && !caution => Verdict::CautionHeld,
                Ok(ok) => Verdict::Allowed(ok),
            };
            out.push((label, verdict));
        }
    }
    out
}

fn plan(rules: &Path, project: &Path) -> CliResult {
    let rules = load_rules(rules)?;
    let found = assess(&rules, &Guard::from_env(), project, true);
    if found.is_empty() {
        println!("nothing matched: no rule detects this folder, or it is already clean");
    }
    for (label, verdict) in found {
        match verdict {
            Verdict::Allowed(ok) => {
                let bytes = agentclean_core::size::usage(ok.path()).bytes;
                println!("allow   {label}  {:?}  {}", ok.class(), human(bytes));
            }
            Verdict::Refused(why) => println!("refuse  {label}  {why}"),
            Verdict::CautionHeld => {}
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn clean(
    rules: &Path,
    projects: &[PathBuf],
    caution: bool,
    execute: bool,
    store: &StoreArg,
) -> CliResult {
    let rules = load_rules(rules)?;
    let guard = Guard::from_env();
    let mut approved = Vec::new();
    let mut held = 0usize;
    for project in projects {
        println!("{}", project.display());
        for (label, verdict) in assess(&rules, &guard, project, caution) {
            match verdict {
                Verdict::Allowed(ok) => {
                    let bytes = agentclean_core::size::usage(ok.path()).bytes;
                    println!("  move    {label}  {}", human(bytes));
                    approved.push((ok, bytes));
                }
                Verdict::Refused(why) => println!("  keep    {label}  {why}"),
                Verdict::CautionHeld => {
                    held += 1;
                    println!("  keep    {label}  Caution: pass --include-caution to move it");
                }
            }
        }
    }
    let total: u64 = approved.iter().map(|(_, b)| b).sum();
    if approved.is_empty() {
        println!("Nothing to clean.");
        return Ok(ExitCode::SUCCESS);
    }
    if !execute {
        println!(
            "Dry run: {}, {} would move to quarantine. Nothing was changed. Add --execute to move them.",
            count(approved.len(), "folder"),
            human(total)
        );
        if held > 0 {
            println!("{} left out (Caution).", count(held, "folder"));
        }
        return Ok(ExitCode::SUCCESS);
    }
    let store = open_store(store)?;
    let run = store
        .clean(approved.into_iter().map(|(a, _)| a).collect())
        .map_err(|e| e.to_string())?;
    print_run(&run);
    println!("run: {}", run.run_id);
    println!("Undo:        agentclean restore {}", run.run_id);
    println!("Free space:  agentclean purge {} --execute", run.run_id);
    let failed = run.entries.iter().any(|e| e.state == State::Failed);
    Ok(if failed {
        ExitCode::from(3)
    } else {
        ExitCode::SUCCESS
    })
}

fn history(store: &StoreArg) -> CliResult {
    let store = open_store(store)?;
    let runs = store.runs().map_err(|e| e.to_string())?;
    if runs.is_empty() {
        println!("No clean runs yet.");
    }
    for run in runs {
        println!(
            "{}  {}  cleaned {}  held in quarantine {}",
            run.run_id,
            count(run.entries.len(), "item"),
            human(run.bytes_cleaned()),
            human(run.bytes_held())
        );
    }
    Ok(ExitCode::SUCCESS)
}

fn restore(run: &str, item: Option<usize>, store: &StoreArg) -> CliResult {
    let store = open_store(store)?;
    let manifest = store.restore(run, item).map_err(|e| e.to_string())?;
    print_run(&manifest);
    let stuck = match item {
        Some(i) => manifest
            .entries
            .get(i)
            .is_some_and(|e| e.state == State::Moved),
        None => manifest.entries.iter().any(|e| e.state == State::Moved),
    };
    Ok(if stuck {
        ExitCode::from(3)
    } else {
        ExitCode::SUCCESS
    })
}

fn purge(run: Option<&str>, days: Option<u64>, execute: bool, store: &StoreArg) -> CliResult {
    let store = open_store(store)?;
    let targets: Vec<Manifest> = match (run, days) {
        (Some(id), _) => vec![store.load(id).map_err(|e| e.to_string())?],
        (None, Some(d)) => {
            let cutoff = now_unix().saturating_sub(d.saturating_mul(86_400));
            let runs = store.runs().map_err(|e| e.to_string())?;
            runs.into_iter()
                .filter(|r| r.created_unix <= cutoff && r.bytes_held() > 0)
                .collect()
        }
        (None, None) => return Err("give a run id or --older-than-days".to_owned()),
    };
    let total: u64 = targets.iter().map(Manifest::bytes_held).sum();
    if total == 0 {
        println!("Nothing held in quarantine for that selection.");
        return Ok(ExitCode::SUCCESS);
    }
    if !execute {
        for r in &targets {
            println!("{}  {}", r.run_id, human(r.bytes_held()));
        }
        println!(
            "Dry run: {} would be permanently deleted from quarantine. This cannot be undone. Add --execute to delete.",
            human(total)
        );
        return Ok(ExitCode::SUCCESS);
    }
    let mut freed = 0u64;
    for r in &targets {
        let after = store.purge(&r.run_id).map_err(|e| e.to_string())?;
        freed += r.bytes_held().saturating_sub(after.bytes_held());
        print_run(&after);
    }
    println!("Freed {}.", human(freed));
    Ok(ExitCode::SUCCESS)
}

fn print_run(run: &Manifest) {
    for (i, e) in run.entries.iter().enumerate() {
        let state = format!("{:?}", e.state).to_lowercase();
        let note = e
            .note
            .as_deref()
            .map(|n| format!("  ({n})"))
            .unwrap_or_default();
        println!(
            "  {i:>3}  {state:<8}  {}  {}{note}",
            e.original.display(),
            human(e.bytes)
        );
    }
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn count(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

/// Decimal units, as Finder reports sizes.
fn human(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}
