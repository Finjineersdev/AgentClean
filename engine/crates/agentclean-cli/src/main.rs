//! `agentclean` command-line interface.
//!
//! M0 exposes two read-only commands. Neither moves or deletes anything;
//! cleaning arrives with M1 (see `docs/REQUIREMENTS.md`).

use std::path::PathBuf;
use std::process::ExitCode;

use agentclean_core::{guard::Guard, rules};
use clap::{Parser, Subcommand};

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

#[derive(Subcommand)]
enum Cmd {
    /// Validate a directory of rule files
    RulesCheck {
        /// Directory containing *.toml rule files
        dir: PathBuf,
    },
    /// Show what the rules and the safety guard would allow in one project (deletes nothing)
    Plan {
        /// Directory containing *.toml rule files
        #[arg(long)]
        rules: PathBuf,
        /// Project root
        project: PathBuf,
    },
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Cmd::RulesCheck { dir } => match rules::load_dir(&dir) {
            Ok(loaded) => {
                let artifacts: usize = loaded.iter().map(|r| r.artifacts.len()).sum();
                println!("{} rule files, {} artifacts: ok", loaded.len(), artifacts);
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("error: {e}");
                ExitCode::FAILURE
            }
        },
        Cmd::Plan { rules, project } => {
            let loaded = match rules::load_dir(&rules) {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("error: {e}");
                    return ExitCode::FAILURE;
                }
            };
            let guard = Guard::from_env();
            let mut any = false;
            for rule in &loaded {
                for candidate in rule.candidates(&project) {
                    if !project.join(candidate.rel()).exists() {
                        continue;
                    }
                    any = true;
                    let rel = candidate.rel().display();
                    match guard.check(&project, &candidate) {
                        Ok(_) => println!(
                            "allow   {rel}  [{} / {:?}]",
                            candidate.rule_id(),
                            candidate.class()
                        ),
                        Err(why) => println!("refuse  {rel}  [{}]  {why}", candidate.rule_id()),
                    }
                }
            }
            if !any {
                println!("nothing matched: no rule detects this folder, or it is already clean");
            }
            ExitCode::SUCCESS
        }
    }
}
