# Handoff — chore/supply-chain-tier1-2 onwards

> **For:** Next Claude Code session picking up the tooling hardening work.
> **Written:** 2026-05-26. Supersedes `docs/handoff-layer2.md`. Delete both files once all remaining work is merged to master.

---

## Kickoff prompt for next session

Paste this verbatim at the start of a new Claude Code session:

```
We are continuing a multi-session tooling hardening effort on the ballast-percolator-cli
repo (CargoBill's Solana devnet POC for bilateral FX/freight-rate hedging).

Read docs/handoff-layer3.md in full before doing anything else — it is the authoritative
source of what is complete, what remains, all locked decisions, and exact implementation
specs for each remaining step. Do not re-litigate any decision in that doc.

Also read:
  ~/.claude/projects/-Users-ap-Documents-GitHub-ballast-percolator-cli/memory/tooling-hardening-session-state.md

After reading both, verify git state by running:
  git log origin/dev --oneline -5   (confirm chore/layer2-tooling is merged)
  git status                         (confirm working tree is clean)
  git branch                         (confirm you are on dev or know where you are)

Then confirm with me before touching any files:
1. chore/layer2-tooling is merged to dev — it added ESLint 9, Prettier, Vitest, Husky
   pre-commit hooks, and migrated 5 test files to Vitest (68 tests pass)
2. pnpm run lint and pnpm run typecheck both pass on dev
3. docs/handoff-layer3.md is currently UNTRACKED — commit it as the first file on the
   chore/supply-chain-tier1-2 branch, not as a separate commit to dev
4. The next branch to create is chore/supply-chain-tier1-2 from dev

CRITICAL CONSTRAINTS for chore/supply-chain-tier1-2 — read before writing any file:

(a) SHA-PINNING: Every GitHub Actions step must use a commit SHA, never a mutable version
    tag like @v4 or @v3. Tags can be silently updated by the action author. Look up current
    SHAs at implementation time using gh api (commands are in the handoff doc). Do not
    guess or reuse SHAs from memory — they drift.

(b) .npmrc: The file already contains shamefully-hoist=true. Add ignore-scripts=true on a
    NEW line. Do not replace, reorder, or rewrite existing lines.

(c) esbuild exception: esbuild is the ONLY entry in pnpm.onlyBuiltDependencies. It needs
    its postinstall to download a pre-built native binary; tsup depends on it. No other
    package gets this exception without an explicit documented decision.

(d) Socket.dev: .socketrc is a config stub only. The GitHub App installation at
    https://socket.dev/install is a MANUAL step via the GitHub UI — it cannot be
    automated in code or CLI. Emit the URL prominently and tell the user explicitly.

(e) CI job name: After the workflow is committed and CI runs once, note the exact job name
    shown in the GitHub Actions UI — it is needed to update master branch protection
    required_status_checks. The handoff doc uses "ci" as the expected name; verify it.

LOCKED ESLint DECISION — do not revert:
eslint.config.js uses parserOptions.project (NOT projectService). The projectService
approach was tried and rejected in the previous session: it silently ignores test/ files
because it discovers tsconfig.json first, which only includes src/**/*. With projectService,
typed linting falls back to untyped for all test files with no warning or error. The fix is
project: './tsconfig.lint.json' which explicitly covers src/**/* + test/**/* +
vitest.config.ts.

WORKFLOW RULE — enforced by hook:
Do not run git commit, git push, gh pr create, or any other mutating git/gh command
yourself. Emit them as fenced bash blocks for the user to run. The hook at
.claude/hooks/block-mutating-commands.mjs will block and log any attempt.
All commits must include:  Co-Authored-By: Claude Code <noreply@anthropic.com>

Confirm your understanding of: (1) current git state, (2) full file list for
chore/supply-chain-tier1-2, (3) why SHA-pinning is mandatory, (4) why projectService is
not used. Ask me to confirm the layer2 PR is merged before writing any files.
```

---

## What was accomplished this session (chore/layer2-tooling)

Starting from the security/guardrails foundation committed to master, this session built the full local development toolchain and test infrastructure.

### Committed to chore/layer2-tooling (PR open → dev)

**ESLint 9 flat config**

- `eslint.config.js` — ESLint 9 flat config with `typescript-eslint` v8 `recommendedTypeChecked` preset. Uses `parserOptions.project: './tsconfig.lint.json'` (NOT `projectService` — `projectService` was tested and rejected because it silently ignores `test/` files: it finds `tsconfig.json` first which only includes `src/**/*`, causing typed linting to fall back to untyped for all test files with no warning. This failure mode was discovered and confirmed during the session. Use `project:` with explicit path always.)
- `tsconfig.lint.json` — extends `tsconfig.json`, adds `"noEmit": true`, explicit `include: ["src/**/*", "test/**/*", "vitest.config.ts"]`. The `vitest.config.ts` entry is required because ESLint parses it and typed linting will error if it is not in the TypeScript project.

**Prettier**

- `.prettierrc.js` — `semi: true, singleQuote: true, trailingComma: 'all', printWidth: 100, tabWidth: 2`. Identical to cargobill-dashboard config.
- `.prettierignore` — `dist/`, `node_modules/`, `target/`, `coverage/`

**Vitest**

- `vitest.config.ts` — `environment: 'node'`, `include: ['test/**/*.test.ts']` ONLY (never `tests/**` — that directory is the devnet T1-T22 integration suite requiring live RPC and funded wallets), v8 coverage provider, reporters `['text', 'html']`, output dir `./coverage/`.

**Pre-commit hooks**

- `lint-staged.config.js` — ESLint `--fix` on `*.ts` first, Prettier `--write` second. Order is intentional: ESLint fixes code quality, Prettier has final say on formatting.
- `.husky/pre-commit` — three steps in order: `pnpm exec lint-staged` → `pnpm exec tsc --noEmit` → `pnpm exec vitest run`. Fail-fast ordering: auto-fix cheap operations first, typecheck second, full test suite last.

**package.json**

- devDeps added: `eslint@^9`, `typescript-eslint@^8`, `eslint-config-prettier@^10`, `prettier@^3`, `husky@^9`, `lint-staged@^15`, `vitest@^2`, `@vitest/coverage-v8@^2`
- Scripts added/updated: `lint`, `typecheck`, `format`, `prepare` (husky), `test` changed from raw `tsx` chain to `vitest run`, `test:coverage` added

**.gitignore** — `coverage/` added

**Test migration**

Migrated all 5 test files in `test/` from raw `assert()` + `console.log` to Vitest `test()` + `expect()` blocks:

- `test/abi.test.ts`
- `test/oracle.test.ts`
- `test/pda.test.ts`
- `test/slab.test.ts` — required fixing stale constants discovered during migration:
  - `ACCOUNT_SIZE` 352 → 360
  - `ENGINE_BITMAP_OFF` 664 → 712
  - `ENGINE_ACCOUNTS_OFF` 9376 → 984 (correct value is `computeLayout(64).engineAccountsOff`)
  - `readLastThrUpdateSlot` → `readMatCounter` (function rename)
  - Mock buffer must be exactly 25216 bytes (`layoutForDataLength` rejects non-canonical sizes)
  - `CONFIG_OFFSET` moved from 72 to 136 (`HEADER_LEN` grew when `insuranceAuthority` + `insuranceOperator` fields were added to the header)
- `test/validation.test.ts`

68 tests pass. Zero failures.

### Infrastructure state

- `dev` branch: exists, tracking `origin/dev`
- Master branch protection: `enforce_admins: true`, `allow_force_pushes: false`, `allow_deletions: false`, PR required (0 approvals), **no required_status_checks yet** — to be added after CI workflow is committed and the CI job name is known
- PR target policy established: all feature branches → `dev`, then `dev` → `master`

---

## What remains (in order)

### Step 1 — Verify dev is at layer2 level

Before starting any new branch, confirm the layer2 PR has merged and dev is current:

```bash
git fetch origin
git log origin/dev --oneline -5
```

If layer2 is not yet merged, wait. Do not branch from a stale dev.

```bash
git checkout dev
git pull origin dev
```

### Step 2 — chore/supply-chain-tier1-2

Branch from dev:

```bash
git checkout -b chore/supply-chain-tier1-2
```

#### File: `.npmrc`

The file already contains `shamefully-hoist=true`. Add `ignore-scripts=true` on a new line. Do NOT replace the existing line or reorder lines. Final file must contain both:

```
shamefully-hoist=true
ignore-scripts=true
```

This closes the postinstall RCE class (ua-parser-js, node-ipc attack pattern). All postinstall scripts are blocked except the explicit allowlist below.

#### File: `package.json`

Add a `"pnpm"` key at the top level:

```json
"pnpm": {
  "onlyBuiltDependencies": ["esbuild"]
}
```

`esbuild` is the sole exception: it uses postinstall to download its pre-built native binary, and `tsup` (the build tool) depends on it. No other package gets this exception. If future dependencies need postinstall, they must be explicitly added here and the decision documented.

#### File: `.github/workflows/ci.yml`

**Before writing this file**, look up the current commit SHAs for the following actions. Do NOT use mutable version tags (`@v4`, `@v3`, etc.) — they can be silently updated by the action author. Use the SHA-pinned form `uses: actions/checkout@<sha>`:

- `actions/checkout` (v4 family)
- `actions/setup-node` (v4 family)
- `actions/cache` (v4 family)
- `dtolnay/rust-toolchain` (stable) — for Rust setup

To look up current SHAs:

```bash
gh api repos/actions/checkout/git/ref/heads/v4 --jq .object.sha
gh api repos/actions/setup-node/git/ref/heads/v4 --jq .object.sha
gh api repos/actions/cache/git/ref/heads/v4 --jq .object.sha
gh api repos/dtolnay/rust-toolchain/git/ref/heads/master --jq .object.sha
```

Workflow structure:

```yaml
name: CI

on:
  pull_request:
    branches: [dev, master]
  push:
    branches: [dev, master]

jobs:
  ci:
    name: ci          # <-- NOTE THIS NAME. Needed for branch protection step below.
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@<SHA>

      - uses: actions/setup-node@<SHA>
        with:
          node-version: '20'

      - uses: actions/cache@<SHA>
        with:
          path: ~/.pnpm-store
          key: ${{ runner.os }}-pnpm-${{ hashFiles('**/pnpm-lock.yaml') }}
          restore-keys: ${{ runner.os }}-pnpm-

      - name: Install pnpm
        run: npm install -g pnpm

      - name: Install dependencies
        run: pnpm install --frozen-lockfile

      - name: Audit dependencies
        run: pnpm audit --audit-level=high

      - name: Lint
        run: pnpm run lint

      - name: Typecheck
        run: pnpm run typecheck

      - name: Test with coverage
        run: pnpm exec vitest run --coverage
        # Do NOT also run bare `vitest run` — that is a double-run.
        # Do NOT use `pnpm run test:coverage` if it adds flags that conflict.
        # Use pnpm exec vitest run --coverage directly.

      - name: Build
        run: pnpm build

      - name: Setup Rust toolchain
        uses: dtolnay/rust-toolchain@<SHA>
        with:
          toolchain: stable

      - name: Cache cargo registry
        uses: actions/cache@<SHA>
        with:
          path: |
            ~/.cargo/registry
            ~/.cargo/git
            ~/.cargo/bin
          key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}
          restore-keys: ${{ runner.os }}-cargo-

      - name: Install cargo-audit
        run: cargo install cargo-audit --locked
        # Option A: direct install. Shorter trust chain than rustsec/audit-check action.
        # Cache above means this is a no-op on repeat runs if binary is cached.

      - name: Audit Rust dependencies
        run: cargo audit
        working-directory: programs/ballast-matcher
```

**After committing this file and CI runs once**, note the exact job name from the GitHub Actions UI. It should be `ci` (matching `name: ci` in the job definition). Then proceed to the branch protection update in Step 5.

#### File: `.github/dependabot.yml`

```yaml
version: 2
updates:
  - package-ecosystem: npm
    directory: /
    schedule:
      interval: weekly

  - package-ecosystem: cargo
    directory: /programs/ballast-matcher
    schedule:
      interval: weekly

  - package-ecosystem: github-actions
    directory: /
    schedule:
      interval: weekly
```

#### File: `.socketrc`

Socket.dev config stub. Thresholds for behavioral risk flags. Exact schema: see https://docs.socket.dev/docs/socket-config-file. A minimal stub:

```json
{
  "version": 2,
  "issueRules": {
    "installScripts": "error",
    "obfuscatedFiles": "error",
    "suspiciousPackage": "error",
    "networkAccess": "warn",
    "environmentVariableAccess": "warn",
    "filesystemAccess": "warn"
  }
}
```

**IMPORTANT — manual step required:** After committing `.socketrc`, the user must manually install the Socket.dev GitHub App at https://socket.dev/install. This is a GitHub App installation via the GitHub UI/OAuth flow. It cannot be automated in code or CLI. Emit this URL prominently and explicitly tell the user this step cannot be skipped or automated.

#### Commit and push

Emit the following bash block for the user to run (do not run it yourself):

```bash
git add .npmrc package.json .github/workflows/ci.yml .github/dependabot.yml .socketrc
git commit -m "$(cat <<'EOF'
chore(supply-chain): add CI workflow, dependabot, ignore-scripts, and Socket.dev stub

- ignore-scripts=true in .npmrc with esbuild as sole onlyBuiltDependencies exception
- GitHub Actions CI: pnpm audit, lint, typecheck, vitest --coverage, build, cargo audit
- All action steps SHA-pinned (no mutable version tags)
- Dependabot: npm + cargo + github-actions weekly updates
- .socketrc: Socket.dev behavioral risk thresholds stub
- Manual step required: install Socket.dev GitHub App at https://socket.dev/install

Co-Authored-By: Claude Code <noreply@anthropic.com>
EOF
)"
git push -u origin chore/supply-chain-tier1-2
```

Then open a PR targeting `dev`:

```bash
gh pr create --base dev --title "chore(supply-chain): add CI workflow, dependabot, ignore-scripts, Socket.dev stub" --body "$(cat <<'EOF'
## Summary

- `.npmrc`: adds `ignore-scripts=true` to close postinstall RCE class; `esbuild` is the sole allowed exception via `pnpm.onlyBuiltDependencies`
- `.github/workflows/ci.yml`: full CI pipeline (pnpm audit → lint → typecheck → vitest --coverage → build → cargo audit); all actions SHA-pinned
- `.github/dependabot.yml`: weekly updates for npm, cargo, and github-actions
- `.socketrc`: Socket.dev behavioral risk config stub

## Manual step required

Install the Socket.dev GitHub App at https://socket.dev/install — this cannot be automated.

## Test plan

- [ ] `pnpm install` succeeds with `ignore-scripts=true` and esbuild exception
- [ ] `pnpm build` succeeds (esbuild postinstall ran via exception)
- [ ] CI green on this PR
- [ ] Dependabot PRs appear within one week of merge
EOF
)"
```

### Step 3 — Update master branch protection (after CI runs)

After the supply-chain PR is merged to dev and CI has run at least once successfully, update master branch protection to require the CI job. The CI job name is `ci` (the `name:` field under `jobs:` in the workflow file).

Emit this command for the user to run:

```bash
gh api repos/adrian-petrykiw/ballast-percolator-cli/branches/master/protection \
  --method PUT \
  --input - <<'EOF'
{
  "required_status_checks": {
    "strict": true,
    "contexts": ["ci"]
  },
  "enforce_admins": true,
  "required_pull_request_reviews": {
    "dismiss_stale_reviews": true,
    "require_code_owner_reviews": false,
    "required_approving_review_count": 0
  },
  "restrictions": null,
  "allow_force_pushes": false,
  "allow_deletions": false
}
EOF
```

If the GitHub Actions job name differs from `ci` (verify in the Actions UI), substitute the actual name in the `contexts` array.

### Step 4 — Memory updates (no branch needed)

Two updates to existing memory files in `~/.claude/projects/-Users-ap-Documents-GitHub-ballast-percolator-cli/memory/`:

**Update `tooling-hardening-session-state.md`** to reflect:
- chore/layer2-tooling: done and merged to dev
- chore/supply-chain-tier1-2: current step
- What remains: supply-chain branch, rubric branch, dev → master PR

**Add reference memory** (new file or append to MEMORY.md):
- Devnet RPC: `api.devnet.solana.com`
- Pyth Hermes: `hermes.pyth.network`

These are allowlisted network hosts in `.claude/settings.json` and referenced throughout Ballast scripts. Useful for future sessions to have them in quick-access memory.

### Step 5 — chore/rubric

Branch from dev:

```bash
git checkout dev && git pull origin dev
git checkout -b chore/rubric
```

Create `docs/claude-setup-rubric.md`. Use Option C structure:

1. **General 6-layer checklist** at the top (repo-agnostic, usable for any project). Six layers:
   - Layer 1: CLAUDE.md (project overview, workflow rules, coding conventions, security rules)
   - Layer 2: Repo tooling guardrails (ESLint, Prettier, Vitest, Husky, lint-staged, tsconfig.lint.json)
   - Layer 3: .claude/ directory (settings.json allowlist-first, hooks/block-mutating-commands.mjs, settings.local.json template)
   - Layer 4: Memory (MEMORY.md, per-topic memory files, session state tracking)
   - Layer 5: Documentation (architecture.md, runbook.md, supply-chain-hardening.md, PRD template, handoff docs)
   - Layer 6: Workflow hygiene (branch naming, commit conventions, Co-Authored-By trailer, PR targets, no direct master push)

2. **Ballast implementation status** section below the general checklist. Each item: ✅ implemented / 🔲 planned / — N/A. Based on the cargobill-dashboard audit format.

Emit a commit block for the user to run; open PR targeting `dev`.

### Step 6 — Final: dev → master PR + verify branch protection

After all branches (supply-chain, rubric) are merged to dev and CI is green end-to-end:

```bash
gh pr create --base master --head dev \
  --title "chore: complete tooling hardening (ESLint, Vitest, CI, supply-chain, rubric)" \
  --body "$(cat <<'EOF'
## Summary

Completes the multi-session tooling hardening effort:

- Layer 2: ESLint 9 flat config, Prettier, Vitest, Husky pre-commit, test migration (68 tests)
- Supply chain: ignore-scripts, esbuild exception, SHA-pinned CI workflow, Dependabot, Socket.dev stub
- Rubric: docs/claude-setup-rubric.md (6-layer general checklist + Ballast status)

## Test plan

- [ ] CI green on this PR
- [ ] `pnpm test` passes locally
- [ ] `pnpm build` succeeds
- [ ] Pre-commit hook fires and passes on a test commit
EOF
)"
```

After merge, verify branch protection has `required_status_checks` set (from Step 3). If not already done, run the `gh api` command from Step 3.

---

## Key decisions — do not re-litigate

| Decision | Choice | Reason |
|---|---|---|
| ESLint version | v9 flat config (`eslint.config.js`) | No Next.js; legacy .eslintrc format is maintenance-only |
| ESLint TypeScript preset | `recommendedTypeChecked` | Typed linting: no-floating-promises, await-thenable, no-unsafe-* — critical for financial code |
| ESLint `parserOptions` | `project: './tsconfig.lint.json'` | `projectService` silently ignores `test/` files (finds tsconfig.json first, which excludes test/); confirmed broken during this session |
| `tsconfig.lint.json` includes | `src/**/*`, `test/**/*`, `vitest.config.ts` | vitest.config.ts must be included or typed linting errors on it |
| Prettier | Yes, same config as cargobill-dashboard | Consistency; eslint-config-prettier disables conflicting rules |
| Vitest include pattern | `test/**/*.test.ts` ONLY | `tests/` is devnet integration suite — never run in CI without live RPC |
| cargo-audit approach | Option A: `cargo install --locked` + cargo cache | Shorter trust chain than rustsec/audit-check GitHub Action |
| `ignore-scripts` exception | `esbuild` only | tsup dependency; all other postinstall scripts are blocked |
| CI action pinning | SHA pins only, never mutable version tags | Version tags can be silently updated; SHA pins are immutable |
| Co-Authored-By trailer | Required on all Claude-assisted commits | Audit trail for regulated financial software |
| `enforce_admins` | true | Even repo owner cannot bypass branch protection |
| PR target | All branches → dev, then dev → master | Prevents half-baked work landing on master |
| Rubric format | Option C | General checklist + Ballast-specific status in same doc |
| Socket.dev | `.socketrc` stub + manual GitHub App install | App installation cannot be automated — must be done via GitHub UI |
| Coverage threshold | None yet | Add once test suite matures |

---

## Critical file paths

```
# Layer 2 (done — chore/layer2-tooling)
eslint.config.js                            # ESLint 9 flat config — use project:, not projectService:
tsconfig.lint.json                          # Extends tsconfig.json, adds test/**/* + vitest.config.ts
.prettierrc.js                              # Formatting config
.prettierignore                             # Excludes dist/, node_modules/, target/, coverage/
vitest.config.ts                            # include: test/**/*.test.ts ONLY
lint-staged.config.js                       # ESLint --fix first, Prettier --write second
.husky/pre-commit                           # lint-staged → tsc --noEmit → vitest run
test/abi.test.ts                            # Migrated to Vitest
test/oracle.test.ts                         # Migrated to Vitest
test/pda.test.ts                            # Migrated to Vitest
test/slab.test.ts                           # Migrated — required fixing 5 stale constants
test/validation.test.ts                     # Migrated to Vitest

# Layer 3 (next — chore/supply-chain-tier1-2)
.npmrc                                      # Add ignore-scripts=true; keep shamefully-hoist=true
.github/workflows/ci.yml                    # SHA-pinned; job name: ci
.github/dependabot.yml                      # npm + cargo + github-actions, weekly
.socketrc                                   # Socket.dev stub; App install is manual

# Security foundation (done — on master)
.claude/hooks/block-mutating-commands.mjs   # PreToolUse hook — do not modify without understanding
.claude/settings.json                       # Allowlist-first permissions
docs/architecture.md                        # Full Percolator reference
docs/supply-chain-hardening.md             # Tiered supply-chain backlog
docs/runbook.md                            # Incident response
programs/ballast-matcher/                  # Rust program — cargo audit runs here
config/ballast-config.json                 # Public keys only
~/.config/ballast/                         # Devnet keypairs — never in repo
~/.cache/ballast/events.jsonl              # Audit log
```

---

## Git state at handoff

```
master    — protected (enforce_admins, no force push, PR required, no required_status_checks yet)
dev       — should be at chore/layer2-tooling level after PR merge
chore/layer2-tooling   — PR open → dev, being merged as this doc is written
```

Next branch to create: `chore/supply-chain-tier1-2` from `dev` (after layer2 PR merged and `git pull origin dev` run locally).

Remaining branches after that (in order):
1. `chore/rubric` — from dev
2. Final PR: `dev` → `master`
