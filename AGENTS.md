# Repository Guidelines

## Project Structure & Module Organization

Mnemark is a Tauri 2 desktop application with a TypeScript frontend and Rust backend. Frontend entry points live in `src/`: `main.ts` drives the clipboard panel, while `settings.ts` and `about.ts` support their matching root-level HTML pages. Shared styling and localization are also in `src/`; static images belong in `src/assets/`. Native code is under `src-tauri/src/`, split by responsibility (`clipboard.rs`, `history.rs`, `persistence.rs`, and `update.rs`). Tauri configuration, capabilities, and application icons live under `src-tauri/`. Architecture decisions are recorded in `docs/adr/`.

## Build, Test, and Development Commands

- `npm ci`: install the exact JavaScript dependencies from `package-lock.json`.
- `npm run tauri dev`: launch Vite and the native app with hot reload.
- `npm run build`: type-check TypeScript and build frontend assets into `dist/`.
- `npm run build:app`: create a release-mode native binary with embedded assets.
- `cargo check --manifest-path src-tauri/Cargo.toml`: quickly validate Rust changes.
- `cargo test --manifest-path src-tauri/Cargo.toml`: run the Rust test suite.

Release builds must enable Tauri's `custom-protocol` feature; use `npm run build:app` rather than an unqualified release build.

## Coding Style & Naming Conventions

Follow existing formatting: two-space indentation and double quotes in TypeScript; standard `rustfmt` output in Rust. Keep TypeScript strict and free of unused declarations. Use `camelCase` for TypeScript variables/functions, `PascalCase` for types, and Rust `snake_case` for modules/functions. Prefer small modules organized by behavior. Run `cargo fmt --manifest-path src-tauri/Cargo.toml` before submitting Rust changes.

## Testing Guidelines

Tests currently use Rust's built-in `#[test]` framework in colocated `#[cfg(test)]` modules. Name tests after observable behavior, for example `double_copy_inside_window_is_dropped`. Add regression tests beside the affected module. There is no frontend test runner; `npm run build` is the required frontend validation. No numeric coverage threshold is enforced.

## Commit & Pull Request Guidelines

History follows Conventional Commit-style subjects such as `fix: ...`, `refactor: ...`, and `chore(release): ...`. Keep commits focused and use an imperative, concise subject. Pull requests should explain the problem and solution, list validation commands, and link related issues. Include screenshots or recordings for visible UI changes. Call out changes to capabilities, persistence formats, updater behavior, or release configuration explicitly.

## Security & Configuration

Keep permissions scoped in `src-tauri/capabilities/default.json`. Never commit signing keys or passwords. Use the project data-directory helpers rather than hardcoding installed or portable storage paths.

<!-- gitnexus:start -->
# GitNexus — Code Intelligence

This project is indexed by GitNexus as **Mnemark** (728 symbols, 1993 relationships, 62 execution flows). Use the GitNexus MCP tools to understand code, assess impact, and navigate safely.

> Index stale? Run `node .gitnexus/run.cjs analyze` from the project root — it auto-selects an available runner. No `.gitnexus/run.cjs` yet? `npx gitnexus analyze` (npm 11 crash → `npm i -g gitnexus`; #1939).

## Always Do

- **MUST run impact analysis before editing any symbol.** Before modifying a function, class, or method, run `impact({target: "symbolName", direction: "upstream"})` and report the blast radius (direct callers, affected processes, risk level) to the user.
- **MUST run `detect_changes()` before committing** to verify your changes only affect expected symbols and execution flows. For regression review, compare against the default branch: `detect_changes({scope: "compare", base_ref: "main"})`.
- **MUST warn the user** if impact analysis returns HIGH or CRITICAL risk before proceeding with edits.
- When exploring unfamiliar code, use `query({search_query: "concept"})` to find execution flows instead of grepping. It returns process-grouped results ranked by relevance.
- When you need full context on a specific symbol — callers, callees, which execution flows it participates in — use `context({name: "symbolName"})`.
- For security review, `explain({target: "fileOrSymbol"})` lists taint findings (source→sink flows; needs `analyze --pdg`).

## Never Do

- NEVER edit a function, class, or method without first running `impact` on it.
- NEVER ignore HIGH or CRITICAL risk warnings from impact analysis.
- NEVER rename symbols with find-and-replace — use `rename` which understands the call graph.
- NEVER commit changes without running `detect_changes()` to check affected scope.

## Resources

| Resource | Use for |
|----------|---------|
| `gitnexus://repo/Mnemark/context` | Codebase overview, check index freshness |
| `gitnexus://repo/Mnemark/clusters` | All functional areas |
| `gitnexus://repo/Mnemark/processes` | All execution flows |
| `gitnexus://repo/Mnemark/process/{name}` | Step-by-step execution trace |

## CLI

| Task | Read this skill file |
|------|---------------------|
| Understand architecture / "How does X work?" | `.claude/skills/gitnexus/gitnexus-exploring/SKILL.md` |
| Blast radius / "What breaks if I change X?" | `.claude/skills/gitnexus/gitnexus-impact-analysis/SKILL.md` |
| Trace bugs / "Why is X failing?" | `.claude/skills/gitnexus/gitnexus-debugging/SKILL.md` |
| Rename / extract / split / refactor | `.claude/skills/gitnexus/gitnexus-refactoring/SKILL.md` |
| Tools, resources, schema reference | `.claude/skills/gitnexus/gitnexus-guide/SKILL.md` |
| Index, status, clean, wiki CLI commands | `.claude/skills/gitnexus/gitnexus-cli/SKILL.md` |

<!-- gitnexus:end -->
