# Project Overview : youwhatknow
Claude Code hook server that injects file summaries before reads and tracks repeat reads per session.

## Tech Stack
Async runtime = Tokio

## Local development
Nix, direnv and flake to manage local dev environment
just to run often used commands

## Context Loading
Before exploring the codebase (reading files, checking structure, dispatching exploration agents):
1. Read `.claude/summaries/project-summary.toml` — full directory/module map
2. Read the specific `.claude/summaries/<area>.toml` for the area you're working in
3. Only explore files directly if the summaries don't answer your question

## Work Structure
Always create a plan, then review, then implement.
Always create a git branch for the work.
Create atomic commits for coherent work done.

## Planning Style
- Small milestones - never more than 5-10 tasks per milestone.
- use `bd` for task tracking

## Code Style
- Idiomatic rust code
- Optimized for readability first
- Avoid long format!() chains and other allocations. Be memory efficient.
- Write tests immediately after a feature.
- Do not write "ceremony" tests that actually just test the library being used.
- Do not use unwrap or expect unless its an invariant.

## Repository Structure
youwhatknow/
├── Cargo.toml
├── CLAUDE.md
├── rust-toolchain.toml
├── flake.nix
├── .envrc
├── .gitignore
├── justfile
├── docs/superpowers/
│   ├── specs/          — Design specs
│   └── plans/          — Implementation plans
└── src/
    ├── main.rs         — Entry point, server startup, signal handling
    ├── config.rs       — Figment-based configuration
    ├── types.rs        — Shared types: summaries, hook request/response
    ├── storage.rs      — TOML read/write, atomic file operations
    ├── session.rs      — Per-session read count tracking
    ├── server.rs       — Axum router, endpoints
    ├── hooks.rs        — Hook handler logic, response formatting
    └── indexer/
        ├── mod.rs      — Index orchestration, full/incremental indexing
        ├── discovery.rs — File discovery via git, filtering
        ├── symbols.rs  — Tree-sitter symbol extraction
        └── describe.rs — Haiku CLI description generation, fallback

## Available commands
The just file has available commands. Be mindful of commands that you run often, add it to the justfile.
