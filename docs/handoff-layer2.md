# Handoff — chore/layer2-tooling onwards

> **For:** Next Claude Code session picking up the tooling hardening work.
> **Written:** 2026-05-25. Delete this file once all remaining work is merged to master.

---

## What was accomplished this session

Starting from a bare repo with no CI, no linting, no pre-commit hooks, and `autoAllowBashIfSandboxed: true`, we built the full security and tooling foundation. Everything below is committed and on master (or in an open PR targeting master).

### Committed to master

- `.claude/hooks/block-mutating-commands.mjs` — PreToolUse hook. Blocks all mutating bash: git commit/push/tag/rebase, gh writes, solana program deploy/upgrade, spl-token writes, pnpm/npm/cargo install, publish, eval escape hatches, curl|sh, git add -A. Structured `permissionDecision: "deny"` output bypasses `--dangerously-skip-permissions`. Logs GUARDRAIL_DECISION to `~/.cache/ballast/events.jsonl`.
- `.claude/settings.json` — allowlist-first permissions (~150 allow patterns, deny block, `autoAllowBashIfSandboxed: false`, `disableBypassPermissionsMode: "disable"`).
- `.claude/settings.local.json` — local file fixed to `autoAllowBashIfSandboxed: false`.
- `.claude/settings.local.json.example` — per-developer template with explicit warnings.
- `docs/runbook.md` — 9-incident operational runbook (oracle stale, cranker lag, bad params, allowlist mistake, upgrade key, role mix-up, RPC flapping, SOL/USDC faucets).
- `docs/supply-chain-hardening.md` — tiered backlog: Tier 1-4 with rationale, implementation steps, tradeoffs.
- `CLAUDE.md` — enforcement paragraph, Permission Model section, GUARDRAIL_DECISION event type, supply-chain pointer.

### Infrastructure

- `dev` branch: created from master current tip (post-merge of security branch), pushed to origin.
- Master branch protection: `enforce_admins: true`, `allow_force_pushes: false`, `allow_deletions: false`, PR required (0 approvals — solo dev), **no required_status_checks yet** (to be added after CI workflow exists and job name is known).

### PR #5 open: chore/layer1-docs → master

- CLAUDE.md: 294 → 193 lines. Percolator reference section extracted to `docs/architecture.md`.
- `docs/architecture.md`: full source layout, all commands, protocol concepts, on-chain layout constants, design decisions, wallet architecture, PRD phases.
- `docs/PRD-TEMPLATE.md`: canonical template (problem, scope, non-goals, success criteria, technical approach, risks, rollout, validation, open questions).
- `docs/archive/.gitkeep`: directory for shipped/superseded PRDs.
- Co-Authored-By policy: changed from "do not add" to **required** on all Claude-assisted commits (audit trail for regulated financial software).

**Action needed:** Merge PR #5 to master before starting layer2 work, so dev stays ahead of master.

---

## What remains (in order)

### Step 1 — Merge PR #5

```bash
# On GitHub: review and merge PR #5 (chore/layer1-docs → master)
# Then pull master locally:
git checkout master && git pull origin master
git checkout dev && git merge master
git push origin dev
```

### Step 2 — chore/layer2-tooling

Branch from dev. All files below must land together before `pnpm install` is run.

**Files to create:**

`tsconfig.lint.json` — extends base tsconfig, adds `test/**/*` to include list. Required because base tsconfig only includes `src/**/*` and ESLint's typed linting (recommendedTypeChecked) needs to type-check test files. Without this, typed linting silently falls back to untyped for test files.

`eslint.config.js` — ESLint 9 flat config (NOT .eslintrc.json — no Next.js here so no legacy format needed). Use `typescript-eslint` v8 with `recommendedTypeChecked` preset (typed linting catches no-floating-promises, await-thenable, no-unsafe-* — critical for financial code). Add `eslint-config-prettier` last to disable formatting rules that conflict with Prettier. Use `parserOptions.projectService: true` pointing at `tsconfig.lint.json`.

`.prettierrc.js` — `semi: true, singleQuote: true, trailingComma: "all", printWidth: 100, tabWidth: 2`. Same as cargobill-dashboard.

`.prettierignore` — `dist/`, `node_modules/`, `target/`, `coverage/`

`vitest.config.ts` — critical details:
- Environment: `node` (no jsdom — this is a CLI, no browser)
- Include: `['test/**/*.test.ts']` ONLY. Do NOT use `**/*.test.ts` — that would pick up files in `tests/` (T1-T22 devnet integration suite that requires RPC and funded wallets; running those in CI would fail)
- Coverage: v8 provider, reporters: `['text', 'html']`, output dir: `./coverage/`
- No coverage threshold yet (add once suite matures)

**Migrate 5 test files** in `test/` only — NOT `tests/`:
- `test/abi.test.ts`
- `test/oracle.test.ts`
- `test/pda.test.ts`
- `test/slab.test.ts`
- `test/validation.test.ts`

Current tests use raw `assert()` function and `console.log`. Convert to Vitest `test()` + `expect()` blocks. Do not force factory-function pattern during migration — just convert assertions. No snapshots (financial data must use explicit value assertions).

`lint-staged.config.js` — ESLint `--fix` on `*.ts` first, Prettier `--write` second. Order matters: ESLint fixes code quality issues first, Prettier gets final say on formatting.

`.husky/pre-commit` — three steps in order: `lint-staged` → `tsc --noEmit` → `vitest run`. Fail-fast: lint/format auto-fixes happen first (cheapest), typecheck next, tests last.

`package.json` changes:
- devDeps to add: `eslint@^9`, `typescript-eslint@^8`, `eslint-config-prettier@^10`, `prettier@^3`, `husky@^9`, `lint-staged@^15`, `vitest@^2`, `@vitest/coverage-v8@^2`
- Scripts to add/update:
  - `"lint": "eslint src test"` 
  - `"typecheck": "tsc --noEmit"`
  - `"format": "prettier --write ."`
  - `"prepare": "husky"`
  - `"test"`: change from raw `tsx` chain to `"vitest run"`
  - `"test:coverage": "vitest run --coverage"`
- No `test:watch` needed (not in scope)

`.gitignore` — add `coverage/` line

**After files land, user runs:**
```bash
pnpm install
```
This bootstraps Husky (installs the pre-commit hook) and installs all new devDeps.

### Step 3 — chore/supply-chain-tier1-2

Branch from dev.

`.npmrc` — add `ignore-scripts=true` on a new line. Keep existing `shamefully-hoist=true`. This single line closes the postinstall RCE class (Shai-Hulud, ua-parser-js attack pattern).

`package.json` — add `"pnpm": { "onlyBuiltDependencies": ["esbuild"] }`. esbuild needs postinstall to download its pre-built binary; tsup uses esbuild under the hood. This is the allowlist exception for `ignore-scripts=true`.

`.github/workflows/ci.yml` — triggers: `pull_request` targeting `dev` or `master`, `push` to `dev` or `master`. **All actions must be pinned to commit SHAs** (look up current SHAs at implementation time — do not use mutable tags like `@v4`). Job sequence:
1. `pnpm install --frozen-lockfile` (lockfile gate — Tier 2)
2. `pnpm audit --audit-level=high` (Tier 1)
3. `pnpm run lint` (Layer 2)
4. `pnpm run typecheck` (Layer 2)
5. `vitest run --coverage` (Layer 2 — do NOT also run bare `vitest run`, that's the double-run issue)
6. `pnpm build` (verify TypeScript compiles to dist/)
7. `cargo audit` in `programs/ballast-matcher/` using `cargo install cargo-audit --locked` with cargo cache (Option A — shorter trust chain than rustsec/audit-check action)

**Note the CI job name.** After this file is committed and CI runs, update master branch protection to require that job:
```bash
gh api repos/adrian-petrykiw/ballast-percolator-cli/branches/master/protection \
  --method PUT \
  --input - <<'EOF'
{
  "required_status_checks": { "strict": true, "contexts": ["<JOB_NAME>"] },
  "enforce_admins": true,
  "required_pull_request_reviews": { "dismiss_stale_reviews": true, "require_code_owner_reviews": false, "required_approving_review_count": 0 },
  "restrictions": null,
  "allow_force_pushes": false,
  "allow_deletions": false
}
EOF
```

`.github/dependabot.yml` — three ecosystems: npm (directory: `/`, weekly), cargo (directory: `/programs/ballast-matcher`, weekly), github-actions (directory: `/`, weekly).

`.socketrc` — Socket.dev config stub. **User must also manually install the Socket.dev GitHub App** at https://socket.dev/install (this cannot be done in code — it's a GitHub App installation via GitHub UI/OAuth).

### Step 4 — Memory updates

Add reference memories for:
- Devnet RPC: `api.devnet.solana.com`
- Pyth Hermes: `hermes.pyth.network`
- Deployed program IDs: placeholder (to be filled when programs are deployed)

### Step 5 — docs/rubric

Branch from dev. Create `docs/claude-setup-rubric.md`.

Structure: Option C — general 6-layer checklist up front (usable for any repo), then a Ballast implementation status section. Based on the cargobill-dashboard audit format (the 6-layer doc shown in the previous session). Each checklist item has a status indicator: ✅ implemented / 🔲 planned / — N/A. Layers: CLAUDE.md, Repo tooling guardrails, .claude/ directory, Memory, Documentation, Workflow hygiene.

### Step 6 — Final: dev → master PR + branch protection update

Once all branches are merged to dev and CI is green end-to-end: open a single PR from `dev` → `master`. Then update branch protection to require the CI job name (emit the `gh api` command).

---

## Key decisions — do not re-litigate

| Decision | Choice | Reason |
|---|---|---|
| ESLint version | v9 flat config (`eslint.config.js`) | No Next.js here; legacy format (.eslintrc.json) is in maintenance-only mode |
| TypeScript ESLint preset | `recommendedTypeChecked` | Typed linting catches no-floating-promises etc — critical for financial code |
| Prettier | Yes, same config as cargobill-dashboard | Consistency; ESLint-config-prettier prevents conflicts |
| Test framework | Vitest | ESM-native, v8 coverage, replaces raw tsx assertions |
| Test migration scope | `test/` only | `tests/` is devnet integration suite — never run in CI without RPC |
| cargo-audit approach | Option A: `cargo install --locked` + cache | Shorter trust chain than rustsec/audit-check action |
| Co-Authored-By | Required | Audit trail for regulated financial software |
| enforce_admins | true | Even Adrian cannot push directly to master |
| PR target | All branches → dev, then dev → master | PR #5 went to master directly (acceptable) but future ones use `--base dev` |
| Rubric format | Option C | General checklist + Ballast-specific status |
| Socket.dev | .socketrc stub + manual GitHub App install | Can't automate the App installation |
| Coverage threshold | None yet | Add once suite matures |

---

## Critical file paths

```
.claude/hooks/block-mutating-commands.mjs  # The hook — do not modify without understanding it
.claude/settings.json                       # Allowlist-first permissions
.claude/settings.local.json                 # Local override (gitignored) — autoAllowBashIfSandboxed: false
docs/architecture.md                        # Full Percolator reference (extracted from CLAUDE.md)
docs/supply-chain-hardening.md             # Tiered supply-chain backlog
docs/runbook.md                            # Incident response
docs/PRD-TEMPLATE.md                       # Canonical PRD template
programs/ballast-matcher/                  # Rust program — cargo audit runs here
config/ballast-config.json                 # Public keys only — never private keys
~/.config/ballast/                         # Devnet keypairs — never in repo
~/.cache/ballast/events.jsonl              # Audit log (GUARDRAIL_DECISION, INCIDENT, etc.)
```

---

## Git state at handoff

```
master    — protected (enforce_admins, no force push, PR required)
dev       — tracking origin/dev, up to date
PR #5     — chore/layer1-docs → master (open, needs merge before layer2 starts)
```

Current branch at session end: `chore/layer1-docs`
Next branch to create: `chore/layer2-tooling` from `dev` (after PR #5 merged)
