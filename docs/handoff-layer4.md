# Handoff: Tooling Hardening — Layer 4 (Rubric + Final PRs)

**Branch at handoff:** `chore/supply-chain-tier1-2` — **merged to dev** (PR #7, commit `2b174b6`)
**Date:** 2026-05-26
**Preceding session summary:** supply-chain-tier1-2 branch complete and merged to dev. Cargo-audit CI required a follow-up fix — config auto-discovery was not applying `audit.toml`; resolved by (a) moving config to `.cargo/audit.toml` (canonical location) and (b) passing explicit `--ignore` flags in `ci.yml`. All CI checks green on merge.

---

## What was done in the session that produced this doc

All 9 files for `chore/supply-chain-tier1-2` are written and verified:

| File | Change |
|---|---|
| `.npmrc` | `ignore-scripts=true` appended |
| `package.json` | `pnpm.onlyBuiltDependencies: ["esbuild"]`, overrides for protobufjs/rollup (axios override removed — peer dep mechanics prevent it from working) |
| `pnpm-workspace.yaml` | `minimumReleaseAge: 10080` (7-day hold for new versions) |
| `audit-allowlist.json` | 7 allowlisted GHSAs: bigint-buffer, picomatch, 5× axios (all with resolution triggers) |
| `.github/workflows/ci.yml` | SHA-pinned actions, frozen-lockfile install, jq audit gate, lint/typecheck/test/build, cargo-audit |
| `.github/dependabot.yml` | npm/cargo/github-actions with semver cooldowns |
| `.socketrc` | Socket.dev config (GitHub App must be installed in repo settings) |
| `docs/supply-chain-hardening.md` | Status updated, implemented table, 7 accepted vulns documented |
| `pnpm-lock.yaml` | protobufjs 7.5.4→7.6.0, rollup 4.54.0→4.60.4; axios still 1.13.2 (acceptable, allowlisted) |

**Audit gate verified locally:** 0 violations after filter.
**`pnpm build` + `pnpm test` (68/68):** green.

Additional fixes landed in follow-up commits on the same PR (cargo-audit CI failure):

| File | Change |
|---|---|
| `.github/workflows/ci.yml` | Added explicit `--ignore RUSTSEC-*` flags to `cargo audit` step |
| `programs/ballast-matcher/.cargo/audit.toml` | Renamed from `audit.toml` → `.cargo/audit.toml` (cargo-audit canonical location); content unchanged |
| `docs/supply-chain-hardening.md` | Added Rust vulnerability subsection with all 8 RUSTSEC advisory paths |

---

## What the next session needs to do

### Step 1 — DONE ✅

`chore/supply-chain-tier1-2` merged to **dev** as PR #7 (commit `2b174b6`). CI green.

- **GitHub → Settings → Branches → master protection → Required status checks → add `ci`** (one-time manual step if not yet done, cannot be done via gh CLI without admin token)

### Step 2 — Write rubric doc on chore/tooling-rubric (30 min)

Branch `chore/tooling-rubric` already exists and is checked out. Skip branch creation.

Create `docs/claude-setup-rubric.md` — **Option C (chosen):** a reusable 6-layer checklist for setting up a new Claude Code project, with the Ballast implementation as the worked example for each layer. Not Ballast-specific — general enough to use on the next CargoBill repo.

**Six layers:**

1. **Guardrails** — PreToolUse hook (block-mutating-commands), permissions.deny hard-blocks, allowlist-first settings.json
2. **Supply chain** — ignore-scripts, onlyBuiltDependencies, pnpm.overrides floor, audit gate + allowlist, cargo-audit, minimumReleaseAge, Dependabot cooldowns, Socket.dev, SHA-pinned Actions
3. **Code quality gate** — ESLint (typescript-eslint strict), Prettier, tsc --noEmit, Husky pre-commit (lint-staged)
4. **Test infrastructure** — Vitest, coverage gate, offline unit tests vs devnet integration tests, no snapshot tests
5. **CI** — frozen-lockfile install, audit gate, lint, typecheck, test+coverage, build, rust steps if applicable
6. **Documentation** — CLAUDE.md (project overview + workflow rule + permission model + coding conventions), architecture.md, runbook.md, supply-chain-hardening.md, PRD + reports/

For each layer: ✅ Ballast status + the 2-3 most important decisions made and why.

Then commit + PR → dev:

```bash
git add docs/claude-setup-rubric.md
git commit -m "$(cat <<'EOF'
docs: add Claude Code project setup rubric (6-layer checklist)

General-purpose checklist for hardening a new Claude Code repo, with
Ballast implementation as worked example. Covers guardrails, supply
chain, code quality, tests, CI, and documentation layers.

Co-Authored-By: Claude Code <noreply@anthropic.com>
EOF
)"
git push -u origin chore/tooling-rubric
gh pr create --base dev --title "docs: add Claude Code project setup rubric (6-layer checklist)" ...
```

### Step 3 — Final dev → master PR

After both PRs above are merged to dev:

```bash
git checkout dev && git pull
gh pr create --base master --title "chore: tooling hardening — Phase 0 complete (Tiers 1-2 supply chain + rubric)"
```

Include in PR body: link to supply-chain-hardening.md, summary of all layers complete, note on Tier 3-4 deferred until mainnet.

### Step 4 — Update memory file

Update `~/.claude/projects/-Users-ap-Documents-GitHub-ballast-percolator-cli/memory/tooling-hardening-session-state.md` to reflect:
- chore/supply-chain-tier1-2: DONE
- chore/tooling-rubric: DONE (after it's merged)
- dev→master final PR: DONE

---

## Key decisions locked (do not re-litigate)

- **pnpm v10** — stays on v10.33.2; v11 has breaking changes (.npmrc settings ignored)
- **axios peer dep** — allowlisted in audit-allowlist.json; pnpm.overrides cannot fix it; resolution trigger is Dependabot PR for @pythnetwork/hermes-client or @zodios/core
- **audit-ci rejected** — depends on event-stream (compromised package); use jq filter approach
- **picomatch NOT overridden** — micromatch@4 uses picomatch@^2.3.1; blanket override would break lint-staged and vitest
- **minimumReleaseAge in pnpm-workspace.yaml** — v10.16+ feature; 10080 minutes = 7 days; only affects `pnpm install`, not `--frozen-lockfile`
- **ESLint parserOptions.project** (NOT projectService) — locked in prior session; projectService is v8.x only
- **onlyBuiltDependencies: ["esbuild"] only** — esbuild needs postinstall for tsup native binary; no other packages in this list
- **cargo-audit config: use `--ignore` flags in CI + `.cargo/audit.toml`** — `audit.toml` in the crate root is not reliably auto-discovered by cargo-audit 0.22.x; explicit `--ignore` flags in `ci.yml` are the authoritative gate; `.cargo/audit.toml` serves as local dev fallback and documentation

---

## Kickoff prompt for next session

Paste this verbatim at the start of the next Claude Code conversation in this repo:

---

We are continuing a multi-session tooling hardening effort on ballast-percolator-cli. Read the memory file at `~/.claude/projects/-Users-ap-Documents-GitHub-ballast-percolator-cli/memory/tooling-hardening-session-state.md` and `docs/handoff-layer4.md` before doing anything.

Current state:
- Branch `chore/supply-chain-tier1-2` is merged to `dev` as PR #7 (commit `2b174b6`) — Step 1 DONE
- Branch `chore/tooling-rubric` already exists and is checked out — do NOT try to create it
- Next work: write `docs/claude-setup-rubric.md` on `chore/tooling-rubric` (Option C: 6-layer general checklist + Ballast worked example), PR → dev, then final dev → master PR
- See `docs/handoff-layer4.md` Step 2 for the rubric structure

Do not ask clarifying questions. Confirm what's merged, then proceed with the rubric.

---
