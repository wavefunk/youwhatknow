# Project Overview : youwhatknow
Hooks for Claude Code that inject project file info and summaries, track repeat reads, and monitor token usage.

## Tech Stack
Async runtime = Tokio

## Local development
Nix, direnv and flake to manage local dev environment
just to run often used commands

## Context Loading
Before exploring the codebase (reading files, checking structure, dispatching exploration agents):
1. Read `.claude/summaries/project-summary.md` — full directory/module map
2. Read the specific `.claude/summaries/<area>.md` for the area you're working in
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
└── src/
    └── main.rs

## Available commands
The just file has available commands. Be mindful of commands that you run often, add it to the justfile.
