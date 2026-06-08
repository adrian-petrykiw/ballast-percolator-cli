# Claude Code Project Setup Rubric

A reusable, six-layer checklist for hardening a new repository that will be
worked on with Claude Code (or any agentic coding assistant). The layers are
ordered by trust boundary: start with what stops an agent or a dependency from
doing damage, end with the documentation that lets the next human (or agent)
operate the repo safely.

This document is **general-purpose** — use it as the starting checklist for any
new CargoBill repo. Each layer pairs the _what/why_ with the **Ballast worked
example**: the concrete decision made in `ballast-percolator-cli`, so you can
copy the pattern and see the trade-offs that were already settled.

This rubric covers **security** — what stops an agent or a dependency from doing
harm. Its companion, [`claude-repo-scaffold.md`](claude-repo-scaffold.md),
covers the **operating model** — how a repo is _structured_ so a human and
Claude Code can plan → build → validate → ship → archive across many sessions
(the CLAUDE.md skeleton, the docs information architecture, the `.claude/`
workflow commands, the PRD lifecycle, and the kickoff/handoff continuity
pattern). Together they are the full baseline: **security + operating model.**
Layer 6 below names the doc set; the scaffold doc is its expansion.

**How to use it:** walk the six layers in order on a new repo. For each, copy
the checklist, then read the Ballast example for the non-obvious decisions
(marked ⚠️) that are easy to get wrong. A layer is "done" when every checklist
box is ticked _and_ the gate is enforced in CI, not just configured locally.

> **Rolling this out to other repos?** See the companion
> [`security-baseline-starter-kit.md`](security-baseline-starter-kit.md) — the
> per-stack translation table (Node / Python / Go / Rust) and the file-by-file
> extraction plan for turning this blueprint into a copyable baseline.

> Scope note: Ballast is a **devnet-only POC** handling no real funds. Some
> "accepted risk" decisions below (e.g. allowlisted advisories) are calibrated
> to that context. The risk calculus tightens for any repo touching mainnet
> keys, mainnet RPC, or production data — those are flagged inline.

---

## Layer overview

| #   | Layer                                                | Stops                                                        | Enforced by                                                                   |
| --- | ---------------------------------------------------- | ------------------------------------------------------------ | ----------------------------------------------------------------------------- |
| 1   | [Guardrails](#layer-1--guardrails)                   | The agent from mutating shared state or exfiltrating secrets | PreToolUse hook + `permissions` in `settings.json`                            |
| 2   | [Supply chain](#layer-2--supply-chain)               | Malicious / vulnerable dependencies                          | `.npmrc`, pnpm config, audit gate, Dependabot, Socket.dev, SHA-pinned Actions |
| 3   | [Code quality gate](#layer-3--code-quality-gate)     | Type-unsafe / unformatted / lint-failing code                | ESLint, Prettier, `tsc --noEmit`, Husky pre-commit                            |
| 4   | [Test infrastructure](#layer-4--test-infrastructure) | Regressions; untested financial math                         | Vitest + coverage; offline-unit vs devnet-integration split                   |
| 5   | [CI](#layer-5--ci)                                   | Any of the above being bypassed before merge                 | GitHub Actions `ci` job, required status check on protected branches          |
| 6   | [Documentation](#layer-6--documentation)             | Knowledge loss; unsafe operation                             | `CLAUDE.md`, `architecture.md`, `runbook.md`, supply-chain doc, PRD + reports |

---

## Layer 1 — Guardrails

**Goal:** the agent cannot, even with `--dangerously-skip-permissions`, run a
command that mutates shared state (push, deploy, on-chain write, publish) or
read a secret. Three mechanisms, in increasing order of expressiveness:

1. **`permissions.allow`** — allowlist-first. Only listed command patterns
   auto-run; everything else prompts the user. This is the default posture.
2. **`permissions.deny`** — hard blocks for catastrophic forms that must _never_
   run, not even at a prompt (deploy, transfer, force-push, secret reads).
3. **PreToolUse hook** — a script for policy that globs can't express:
   tokenization, recursion into wrapper commands, conditional matching.

### Checklist

- [ ] `.claude/settings.json` exists with `permissions.allow` covering normal
      dev work (file read, git read + _targeted_ staging, build, test, read-only
      CLI queries) so the agent isn't prompt-spammed.
- [ ] `permissions.deny` hard-blocks: deploy/upgrade/close, value transfers,
      `publish`, `git push --force*`, `*--no-verify*`, secret/keypair reads,
      and any `*mainnet*` substring.
- [ ] Secret files denied for **Read, Edit, and Bash-cat** (all three — globs
      differ per tool): `.env*`, `**/secrets/**`, `*keypair*`, signer key dirs.
- [ ] A PreToolUse hook on Bash enforces the "agent does not mutate shared
      state" rule and **recurses into wrapper commands** (`bash -c`, `sh -c`,
      `env`, `nice`, `xargs`, `sudo`) so `bash -c "git push"` is also caught.
- [ ] Hook returns a structured `permissionDecision: "deny"` (this bypasses
      `--dangerously-skip-permissions` by design) and **logs every denial** to an
      audit trail.
- [ ] `sandbox.autoAllowBashIfSandboxed: false` committed — flipping it `true`
      reverts to the pre-hardening permission model.
- [ ] Sandbox network egress restricted to an explicit `allowedDomains` list.
- [ ] A `settings.local.json.example` template documents per-developer overrides
      without committing anyone's local state.

### Ballast worked example ✅

- **`.claude/hooks/block-mutating-commands.mjs`** — PreToolUse hook. Blocks
  git write ops (commit/push/tag/rebase/reset --hard/branch -D/stash drop),
  `gh` writes, `solana` deploy + value transfers, `spl-token` writes,
  `*publish`, all installs (supply-chain gate — _the user_ runs installs),
  language-eval escape hatches (`node -e`, `python -c`, …), `curl|sh` pipes,
  `rm -rf` on dangerous roots, and `git -c <dangerous-key>=`. ⚠️ It recurses
  into `bash -c`/`env`/`xargs`/`sudo` wrappers and quote-aware-splits pipelines
  so `cd subdir && git push` is caught too. Every block is logged to
  `~/.cache/ballast/events.jsonl` as `event_type: "GUARDRAIL_DECISION"`.
- **`.claude/settings.json`** — 222 `allow` patterns; a `deny` block covering
  every mutating form above plus `*mainnet*`/`*mainnet-beta*` substrings and
  secret reads across Read/Edit/Bash; `sandbox.autoAllowBashIfSandboxed: false`;
  network egress limited to GitHub, npm/yarn registries, devnet RPC, Pyth
  Hermes, and crates.io.
- **The workflow rule it enforces:** Claude never runs commits/pushes/PRs/
  deploys — it emits a copy-pasteable fenced bash block (heredoc'd commit
  message, `Co-Authored-By` trailer for audit) and waits for the user to run it.

**Key decisions:**

1. ⚠️ **Hook over globs for anything stateful.** Globs (`permissions.deny`)
   can't tokenize or recurse; the hook is the authoritative layer and has _no
   override flag_. Globs are the fast first line; the hook is the backstop.
2. **Allowlist-first, not denylist-first.** Unknown commands prompt rather than
   run. This travels to mainnet unchanged — the whole point is that the same
   patterns are safe when real keys land.
3. **Installs are a guardrail, not just a supply-chain concern** — the agent is
   blocked from `pnpm add`/`cargo add`; humans run installs so a compromised
   transitive dep can't be pulled in by an agent mid-task.

---

## Layer 2 — Supply chain

**Goal:** a malicious or vulnerable dependency cannot execute on install, slip
in silently, or merge without review. Defense in depth from install-time
execution down to advisory triage.

### Checklist

- [ ] **`ignore-scripts=true`** in `.npmrc` — no package runs postinstall
      scripts by default (the npm supply-chain attack vector).
- [ ] An **explicit exception list** for the few packages that legitimately need
      a build step (native binaries), and nothing else.
- [ ] **`pnpm.overrides`** pin a floor for known-vulnerable transitive deps you
      _can_ move.
- [ ] An **audit gate** in CI that fails on high/critical advisories, with a
      reviewed **allowlist** for advisories you've accepted — each entry carrying
      a path, a _why-not-exploitable_ rationale, and a _resolution trigger_.
- [ ] **`cargo audit`** for any Rust crates, with the same explicit-ignore model.
- [ ] **`minimumReleaseAge`** (version cooldown) so a freshly-published
      compromised version isn't installed the day it lands.
- [ ] **Dependabot** with semver-tiered cooldowns for npm + cargo + actions.
- [ ] **Socket.dev** (or equivalent) flagging install scripts / obfuscation /
      suspicious packages at PR time.
- [ ] **GitHub Actions pinned to commit SHAs**, never mutable tags.
- [ ] **`pnpm install --frozen-lockfile`** in CI — lockfile is the source of
      truth; the tracked lockfile must not drift.

### Ballast worked example ✅

- **`.npmrc`** — `ignore-scripts=true`. **`package.json`
  `pnpm.onlyBuiltDependencies: ["esbuild"]`** — the _only_ package allowed a
  postinstall (tsup's native binary). `pnpm.overrides`: `protobufjs ^7.5.6`,
  `rollup ^4.59.0`.
- **`audit-allowlist.json`** — 7 accepted high-severity GHSAs (bigint-buffer,
  picomatch, 5× axios), each with full path, non-exploitability rationale, and a
  named Dependabot resolution trigger.
- **`.github/workflows/ci.yml`** — `pnpm audit --json` piped through `jq` that
  drops allowlisted GHSAs and fails on any remaining high/critical.
  `cargo audit` with 8 explicit `--ignore RUSTSEC-*` flags.
- **`pnpm-workspace.yaml`** — `minimumReleaseAge: 10080` (7 days).
- **`.github/dependabot.yml`** — npm (major 14d / minor 7d / patch 3d cooldown),
  cargo + actions (7d). **`.socketrc`** — install scripts/obfuscation/suspicious
  = error; network/fs/env access + recently-published = warn.
- All Actions SHA-pinned (e.g. `actions/checkout@34e1148…`).

**Key decisions:**

1. ⚠️ **`audit-ci` was rejected** — it depends on `event-stream`, a historically
   _compromised_ package. A hand-rolled `jq` filter over `pnpm audit --json` is
   the gate instead; no new attack surface to add an audit tool.
2. ⚠️ **Peer-dep vulnerabilities can't be `overrides`-fixed.** axios@1.13.2
   arrives via `@zodios/core` as a _peer_ dep; pnpm does not substitute overrides
   into peer slots. The 5 axios GHSAs are allowlisted (devnet, trusted Pyth
   endpoints only) with the resolution trigger being a Dependabot bump of
   `@pythnetwork/hermes-client` / `@zodios/core`. **On a mainnet repo, revisit:**
   accepting an HTTP-client advisory is only defensible because axios here never
   touches untrusted input.
3. ⚠️ **picomatch is _not_ overridden** — `micromatch@4` declares
   `picomatch@^2.3.1`; a blanket override forces micromatch off its range and
   breaks lint-staged + vitest watching. Allowlisted instead, resolution gated on
   an upstream tsup/tinyglobby bump.
4. **Every allowlist entry is self-expiring** — the rationale names the exact
   condition under which the entry should be removed ("when advisory no longer
   appears in `pnpm audit`"), so the allowlist doesn't rot into a permanent
   blind spot.

> Full supply-chain backlog and the tiered roadmap (Tier 3–4 deferred until
> mainnet) live in [`supply-chain-hardening.md`](supply-chain-hardening.md).

---

## Layer 3 — Code quality gate

**Goal:** type-unsafe, unformatted, or lint-failing code can't be committed.
Catch it at the keystroke (editor), the commit (Husky), and the merge (CI) —
three chances, same ruleset.

### Checklist

- [ ] **ESLint** with `typescript-eslint` type-checked rules (not just syntactic).
- [ ] ⚠️ Linting uses **`parserOptions.project`** pointed at a tsconfig that
      _explicitly includes test files and config files_ — not `projectService`,
      which silently skips files outside the first tsconfig it finds.
- [ ] **Prettier** + `eslint-config-prettier` so formatting and linting don't
      fight; a `.prettierignore` for generated/vendored paths.
- [ ] **`tsc --noEmit`** as a separate typecheck step (lint ≠ full typecheck).
- [ ] **Husky `pre-commit`** running **lint-staged** on changed files only.
- [ ] A documented policy for **suppressing rules on code you don't own**
      (vendored/upstream), scoped narrowly rather than disabled globally.

### Ballast worked example ✅

- **`eslint.config.js`** (flat config) — `eslint.configs.recommended` +
  `tseslint.configs.recommendedTypeChecked` + `prettierConfig`.
  `parserOptions.project: './tsconfig.lint.json'`.
- **`tsconfig.lint.json`** — extends `tsconfig.json`, `noEmit: true`, and
  explicitly includes `src/**`, `test/**`, _and_ `vitest.config.ts` (omitting the
  config file makes typed linting error on it).
- **`.prettierrc.js`** + **`.prettierignore`**; **`.husky/pre-commit`** runs
  lint-staged (`lint-staged.config.js`).
- Scripts: `lint: "eslint src test"`, `typecheck: "tsc --noEmit"`,
  `format: "prettier --write ."`, `prepare: "husky"`.
- **Upstream-suppression block:** `src/**/*.ts` disables `no-unsafe-*` /
  `no-explicit-any` — the upstream percolator-cli uses Commander's `.opts()`
  which returns `any`, and CLAUDE.md forbids modifying upstream `src/`. Scoped to
  `src/` only; all _new_ Ballast code in `scripts/ballast/` is held to the full
  ruleset.

**Key decisions:**

1. ⚠️ **`project` not `projectService`.** `projectService` finds `tsconfig.json`
   first (which only includes `src/**`) and _silently_ type-checks nothing in
   `test/`. The dedicated `tsconfig.lint.json` is the fix — this was the single
   most error-prone setting in the whole effort.
2. **Suppress narrowly, by path.** Upstream `any` is tolerated only under
   `src/`; new code gets no exemption. The rule disable is a documented comment,
   not a silent config tweak.
3. **Three enforcement points, one ruleset** — editor, pre-commit, CI all run
   the same ESLint/Prettier/tsc so there's no "passes locally, fails in CI" gap.

---

## Layer 4 — Test infrastructure

**Goal:** regressions are caught automatically, and the _fast, deterministic_
tests are cleanly separated from slow networked ones so CI stays green and quick.

### Checklist

- [ ] A single test runner (**Vitest**) with a coverage command.
- [ ] ⚠️ **Offline unit tests vs. networked integration tests are physically
      separated** by directory, and the runner config includes **only** the
      offline set. Integration tests never run in CI by accident.
- [ ] Integration tests are **idempotent** (re-runnable without manual cleanup)
      and document their preconditions (funded wallets, deployed state, …).
- [ ] **No snapshot tests for financial data** — explicit value assertions only;
      money math must be exact and reviewed, not auto-captured.
- [ ] Tests asserting on-chain layout pin **exact byte sizes / offsets** so a
      wire-format drift fails loudly.

### Ballast worked example ✅

- **Vitest** — `test: "vitest run"`, `test:coverage: "vitest run --coverage"`.
- **`vitest.config.ts`** — `include: ['test/**/*.test.ts']` **only**.
  ⚠️ The `tests/` directory (note plural) holds the devnet integration suite
  (`preflight.ts`, `runner.ts`, T1–T22) and is _never_ in the Vitest include —
  it would hit live RPC in CI. Two directories, one letter apart, deliberately.
- Offline unit tests cover price-format transforms, config loading, allowlist
  validation, PnL math. RPC calls and on-chain state are integration-only.
- **No snapshots** — financial assertions are explicit literal comparisons.
- Layout tests pin exact constants (e.g. canonical slab buffer size; engine
  bitmap/accounts offsets) so a v12.x wire-format change fails the test rather
  than silently passing.

**Key decisions:**

1. ⚠️ **`test/` (unit, in CI) vs `tests/` (integration, devnet, never in CI).**
   The naming is intentional and the Vitest `include` enforces it. This is the
   second-most error-prone setting after the ESLint `project` choice.
2. **No snapshots for money.** Snapshots silently bless whatever the code
   currently outputs; financial correctness needs a human-chosen expected value.
3. **Coverage runs in CI** (`vitest run --coverage`) so the gate is visible, not
   just a local convenience.

---

## Layer 5 — CI

**Goal:** every layer above is _enforced at the merge boundary_, not merely
configured locally. A PR cannot merge to a protected branch without a green run.

### Checklist

- [ ] One `ci` job triggered on PR + push to protected branches.
- [ ] **`pnpm install --frozen-lockfile`** — no lockfile drift permitted.
- [ ] Steps, in order: **audit gate → lint → typecheck → test+coverage → build**,
      plus **Rust toolchain + `cargo audit` + build** if the repo has crates.
- [ ] All `uses:` **pinned to commit SHAs**.
- [ ] Dependency caching (pnpm store + cargo registry) keyed on lockfile hashes.
- [ ] **Branch protection** with the `ci` check **required** on the protected
      branch, `enforce_admins: true`, and force-pushes disabled.
- [ ] A clear **branch/PR flow** (e.g. feature → `dev` → `master`) so CI runs at
      every hop.

### Ballast worked example ✅

- **`.github/workflows/ci.yml`** — triggers on PR + push to `dev`/`master`.
  Node 22, `pnpm@10`, `--frozen-lockfile`. Steps: jq audit gate → `pnpm run
lint` → `pnpm run typecheck` → `vitest run --coverage` → `pnpm build` → Rust
  toolchain → `cargo install cargo-audit --locked` → `cargo audit` (8 explicit
  `--ignore`s) in `programs/ballast-matcher`.
- All Actions SHA-pinned; pnpm store + cargo registry cached on lockfile hashes.
- **Branch protection on `master`:** `enforce_admins: true`,
  `allow_force_pushes: false`. ⚠️ Add the **`ci` required status check** in
  GitHub → Settings → Branches once the first run is green (a one-time manual
  step; can't be set via `gh` CLI without an admin token).
- **Flow:** all feature branches → `dev`, then `dev` → `master`.

**Key decisions:**

1. ⚠️ **`cargo audit` config via explicit `--ignore` flags in CI**, with
   `.cargo/audit.toml` as a local-dev fallback. `audit.toml` in the crate root
   is _not_ reliably auto-discovered by cargo-audit 0.22.x — the CLI flags are
   the authoritative gate.
2. **`cargo install cargo-audit --locked`** rather than a third-party
   `audit-check` Action — fewer unpinned action dependencies in the trust chain.
3. **SHA pins only.** A mutable `@v4` tag can be re-pointed at malicious code;
   the SHA can't.

---

## Layer 6 — Documentation

**Goal:** the next operator — human or agent — can work the repo safely without
re-discovering the rules, the architecture, or the incident response. Docs are
part of the security boundary: an undocumented guardrail gets disabled by
someone who didn't know why it was there.

### Checklist

- [ ] **`CLAUDE.md`** — project overview, the agent **workflow rule** (what the
      agent may/may not run), the **permission model**, coding conventions,
      security rules, event-logging schema.
- [ ] **`architecture.md`** — system/protocol reference extracted out of
      `CLAUDE.md` so the latter stays a lean instruction file.
- [ ] **`runbook.md`** — incident response: detection → diagnosis → remediation
      per failure mode, with the event-log schema for incident records.
- [ ] **`supply-chain-hardening.md`** — what's implemented, what's accepted (with
      rationale), what's deferred and until when.
- [ ] **PRD + `reports/`** — a per-step validation report (success criteria,
      tx signatures, state snapshots, discrepancies) so claims are auditable.
- [ ] A **PRD template** so every phase/step is specified consistently.
- [ ] **Session/handoff docs** for multi-session efforts so context survives a
      new conversation.

### Ballast worked example ✅

- **`CLAUDE.md`** — project overview, the "Claude does not run commits/pushes/
  PRs" workflow rule, the allowlist-first permission model, data-flow pattern,
  on-chain layout constants, coding conventions, security rules, and the
  `events.jsonl` schema (incl. `GUARDRAIL_DECISION`).
- **`docs/architecture.md`** — full Percolator reference extracted from
  `CLAUDE.md` to keep the instruction file lean.
- **`docs/runbook.md`** — 9-incident operational runbook; `INCIDENT` events
  appended during the response loop.
- **`docs/supply-chain-hardening.md`** — implemented table, 7 accepted npm vulns
  - 8 Rust advisories with rationale, Tier 3–4 deferred until mainnet.
- **`docs/PRD-TEMPLATE.md`** + **`docs/prd.md`** + **`docs/reports/`** —
  per-step validation reports keyed to success criteria SC-0.1 … SC-1.8.
- **`docs/handoff-layer{2,3,4}.md`** — per-session handoffs that carried this
  multi-session hardening effort across context windows. **This rubric is the
  capstone of that effort.**

**Key decisions:**

1. **`CLAUDE.md` is instructions, `architecture.md` is reference.** Splitting
   them keeps the file the agent loads every session short and actionable.
2. **Documentation is enforcement.** The workflow rule and permission model are
   written down _and_ enforced by Layer 1 — the doc explains the _why_ so nobody
   disables the hook out of confusion.
3. **Handoff docs + memory notes** make multi-session work resumable: every
   locked decision is recorded once so it's never re-litigated.

---

## Quick-start order for a new repo

1. **Layer 1 first** — land the hook + `settings.json` before doing any other
   agent-assisted work, so the agent is constrained from commit zero.
2. **Layer 3 + 4** — ESLint/Prettier/tsc + Vitest, so quality gates exist before
   code volume grows.
3. **Layer 2** — `.npmrc`, pnpm config, audit allowlist, Dependabot, Socket.dev.
4. **Layer 5** — wire all of the above into `ci.yml`; turn on branch protection
   with `ci` required.
5. **Layer 6** — `CLAUDE.md` + architecture + runbook from the start; fill
   `reports/` as work ships.

Each layer is independently valuable; ship them as separate PRs so each gate is
reviewed on its own. A layer isn't "done" until its check is **required in CI** —
configured-but-not-enforced is the failure mode this rubric exists to prevent.
