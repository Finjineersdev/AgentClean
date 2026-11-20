# ADR 0003: Developer-only scope

Status: accepted, 19 Nov 2026

## Decision

OpenSweep cleans developer projects and developer tool caches. The inherited
system-junk, malware, uninstaller, fan and sensor, and speed-test modules are
removed in M3. Projects, developer caches, Docker, simulators and the disk map
stay.

## Why

Each removed module is attack surface and maintenance with no bearing on the
product's purpose, and several need broader permissions than a project
cleaner should ask for.
