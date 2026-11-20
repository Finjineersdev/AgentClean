# ADR 0002: One deletion path

Status: accepted, 19 Nov 2026

## Decision

Only `opensweep-core` may move or delete user files, and only for a path
holding an `Approved` token that has just passed `reverify`. `Candidate` and
`Approved` cannot be constructed outside the crate.

By the end of M3, no Swift code calls a file-removal or move API on user
paths. A CI check greps the app and `CleanCore` for those APIs and fails on
any use outside an allowlist.

## Why

Two engines mean two safety guards, and the weaker one sets the product's real
safety level. Upstream's guard is prefix-based and lightly tested; the Rust
guard is specified, property-tested and type-enforced.
