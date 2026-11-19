# ADR 0001: Fork Zouuabi/CleanMyDev and add a Rust engine

Status: accepted, 19 Nov 2026

## Context

The owner chose to build on Zouuabi/CleanMyDev (MIT, Swift 6, macOS 26) and
also chose a Rust engine with a SwiftUI app. The upstream engine is Swift.

## Decision

Fork upstream for the Mac app. Add a Rust workspace in `engine/` that owns
rules, the safety guard, scanning, cleaning and restore, with a CLI. Swift
modules worth keeping (Docker, simulators, activity signals) become read-only
scanners that hand candidates to the Rust engine.

## Consequences

- Two languages and a bridge (UniFFI, M3) to maintain.
- A CLI on macOS and Linux, and a Windows path later.
- Until M3 the inherited Swift engine still contains its own deletion code;
  ADR 0002 removes it.
- New work is Apache-2.0; inherited code keeps its MIT and BSD-3-Clause notices.
