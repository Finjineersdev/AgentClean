//! AgentClean engine.
//!
//! Two things live here so far:
//!
//! * [`rules`] — the declarative rule catalogue and its linter.
//! * [`guard`] — the deletion safety guard. Every delete, from any front end,
//!   must hold a [`guard::Approved`] token issued by [`guard::Guard::check`],
//!   and the guard only accepts a [`rules::Candidate`] produced by a rule.
//!
//! The invariants the guard enforces are specified in `docs/SAFETY-SPEC.md`.

pub mod activity;
pub mod deny;
pub mod discover;
pub mod guard;
pub mod quarantine;
pub mod rules;
pub mod size;

#[cfg(test)]
mod activity_tests;
#[cfg(test)]
mod discover_tests;
#[cfg(test)]
mod guard_tests;
#[cfg(test)]
mod quarantine_tests;
