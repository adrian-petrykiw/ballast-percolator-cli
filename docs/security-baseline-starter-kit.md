# Security Baseline — Starter Kit & Per-Stack Translation

Companion to [`claude-setup-rubric.md`](claude-setup-rubric.md). The rubric is the
*blueprint* (the six layers, the threat model, the gotchas). **This doc is the
operational kit for rolling that blueprint out across other CargoBill repos** —
what to copy verbatim, what to template, and how each Layer 2–5 control maps onto
a non-Node/Rust stack.

> **Where this should eventually live:** in a dedicated `cargobill-security-baseline`
> (or org `.github`) repo, alongside the copyable files. It lives in
> `ballast-percolator-cli` for now because that's where the reference
> implementation was built. Move it when the baseline repo exists.

> **What this is NOT gated on:** you can roll the baseline out to other repos
> today. The Ballast Dependabot triage (repo-specific upkeep) and Tiers 3–4
> (additional depth on Ballast) are **not** prerequisites — Tier 1+2 is the
> security floor, and it's fully captured here.

---

## Part A — Starter-kit extraction plan

### A.1 What's portable, and how much editing each needs

| File / artifact | Reuse mode | Per-repo edits |
|---|---|---|
| `.claude/hooks/block-mutating-commands.mjs` | **Copy ~verbatim** | Swap the on-chain/dangerous-CLI block list (Ballast: `solana`, `spl-token`) for *this* repo's destructive commands (e.g. `kubectl`, `terraform apply`, `aws`, `gcloud`, `flyctl`, `wrangler`, `psql -c "DROP`). The git/gh/publish/install/eval/`curl\|sh`/`rm -rf` core is universal. |
| `.claude/settings.json` | **Template** | Keep the allowlist-first structure + `sandbox.autoAllowBashIfSandboxed:false`. Rewrite `permissions.allow` build/test commands and `permissions.deny` dangerous-CLI/`*mainnet*`/secret globs for the stack + product. |
| `.claude/settings.local.json.example` | Copy ~verbatim | Path/comment tweaks only. |
| `CLAUDE.md` | **Template** | Keep the workflow rule (agent doesn't commit/push/deploy) + permission-model section. Rewrite project overview, stack commands, data-flow, conventions. |
| `docs/runbook.md` | **Template** | Keep incident-response structure + event schema; rewrite the incident catalog per system. |
| `.github/workflows/ci.yml` | **Skeleton** | Keep job shape (frozen install → audit gate → lint → typecheck → test+coverage → build) and **SHA-pinned actions**. Swap every command for the stack's equivalent (Part B). |
| `.github/dependabot.yml` | **Template** | Swap `package-ecosystem` keys (Part B); keep weekly + cooldown structure. |
| `.socketrc` | **Copy verbatim** | Same `issueRules`; ensure Socket supports the stack's ecosystem (Part B notes). |
| `audit-allowlist.json` + the CI jq gate | **Pattern, not file** | The *mechanism* (allowlist by advisory ID + rationale + resolution trigger, filtered in CI) transfers; the entries are per-repo. For non-npm stacks, the equivalent is the audit tool's own ignore mechanism (Part B). |
| `pnpm-workspace.yaml minimumReleaseAge` | **pnpm-only** | See Part B for the per-stack release-age story (mostly Dependabot cooldown / Renovate elsewhere). |
| `docs/claude-setup-rubric.md` | **Copy verbatim** | It's already general; the worked example stays as a reference. |
| `docs/supply-chain-hardening.md` | **Template** | Keep the Tier 1–4 ladder + threat model; rewrite the accepted-vuln entries (repo-specific). |

### A.2 Proposed baseline-repo layout

```
cargobill-security-baseline/
├── README.md                       # how to bootstrap a repo from this kit
├── rubric.md                       # = claude-setup-rubric.md (the blueprint)
├── per-stack.md                    # = Part B of this doc (the translation table)
├── claude/                         # drop-in .claude/ contents
│   ├── hooks/block-mutating-commands.mjs
│   ├── settings.json.template
│   └── settings.local.json.example
├── ci/
│   ├── node-pnpm.ci.yml            # one CI skeleton per stack
│   ├── python-uv.ci.yml
│   ├── go.ci.yml
│   └── rust.ci.yml
├── dependabot/                     # one dependabot.yml per ecosystem mix
├── socketrc                        # .socketrc
└── docs-templates/
    ├── CLAUDE.md.template
    ├── runbook.md.template
    └── supply-chain-hardening.md.template
```

### A.3 Per-repo rollout order (proven on Ballast)

For each target repo, ship **as separate PRs** so each gate is reviewed alone, and
target `dev` then `dev → master`, exactly as Ballast did:

1. **Layer 1 first** — hook + `settings.json`, before any other agent-assisted work, so the agent is constrained from commit zero.
2. **Layers 3 + 4** — linter/formatter/type-check + test runner, before code volume grows.
3. **Layer 2** — install-script block, audit gate + allowlist, lockfile-frozen install, registry pin, Dependabot, Socket, SHA-pinned actions.
4. **Layer 5** — wire all the above into `ci.yml`; enable branch protection with `ci` **required** on `master` (strict/up-to-date + include-admins) and on `dev` (lighter: required `ci`, admins may bypass).
5. **Layer 6** — `CLAUDE.md` + architecture + runbook from the start; fill reports as work ships.

A layer is "done" only when its check is **required in CI**, not merely configured — configured-but-not-enforced is the failure mode the whole exercise exists to prevent.

### A.4 Bootstrap checklist (copy per repo)

- [ ] Drop in `.claude/` (hook + settings); edit dangerous-CLI denies for this repo
- [ ] Pick the stack column in Part B; create `.npmrc`/`pip.conf`/`.cargo/config.toml`/env as listed
- [ ] Add install-script block + lockfile-frozen install
- [ ] Add audit gate + allowlist mechanism
- [ ] Add lint + format + type-check + test runner (+ pre-commit)
- [ ] Add `ci.yml` (SHA-pinned) + `dependabot.yml` + `.socketrc`
- [ ] Branch protection: required `ci` on master (strict + admins) and dev (lighter)
- [ ] `CLAUDE.md` + `runbook.md` + `supply-chain-hardening.md`
- [ ] Verify every gate **fails** on a deliberate violation, not just passes clean

---

## Part B — Per-stack translation table

**Layer 1 (Guardrails) and Layer 6 (Docs) are stack-agnostic** — the hook,
permission model, and doc set transfer unchanged (only the dangerous-CLI deny list
and project specifics differ). The table below covers **Layers 2–5**, which are
stack-specific. The Node/pnpm column is the Ballast reference.

> `(verify)` = confirm against the current tool version when you implement; these
> evolve. Footnotes cover yarn/bun and uv-vs-poetry differences.

### Layer 2 — Supply chain

| Control | Node / pnpm (reference) | Node / npm | Python | Go | Rust / cargo |
|---|---|---|---|---|---|
| **Block install-time code exec** | `.npmrc ignore-scripts=true` + `pnpm.onlyBuiltDependencies` allowlist | `.npmrc ignore-scripts=true` (no granular allowlist; `npm rebuild <pkg>` manually if needed) | Prefer wheels: `--only-binary=:all:` (pip) / uv defaults to wheels. *No npm-style postinstall, but `setup.py` in sdists runs code at build* | **None needed** — Go runs no install-time code (`go build` is hermetic; cgo/`go generate` are explicit & local) ✅ | ⚠️ **Residual gap** — `build.rs` runs arbitrary code at build; cargo has no `ignore-scripts`. Mitigate with sandboxed CI builds + cargo-deny/vet |
| **Audit (advisories)** | `pnpm audit` | `npm audit` | `pip-audit` (works against pip/poetry/uv-exported reqs) | `govulncheck ./...` | `cargo audit` |
| **Frozen / locked install** | `pnpm install --frozen-lockfile` | `npm ci` | `uv sync --locked` / `poetry install` (+ `poetry check --lock`) / `pip install --require-hashes` | `go mod download && go mod verify`; build `-mod=readonly` | `cargo build --locked` |
| **Lockfile hash verification** | integrity hashes in `pnpm-lock.yaml`, enforced by `--frozen-lockfile` | `package-lock.json` integrity, enforced by `npm ci` | `--require-hashes` (pip); uv/poetry lock hashes | `go.sum` + `go mod verify` + `GOSUMDB` | `Cargo.lock` checksums + `--locked` |
| **Registry / source pin** | `.npmrc registry=https://registry.npmjs.org/` | same | `index-url` in `pip.conf` / `[tool.uv]` = PyPI | `GOPROXY=https://proxy.golang.org,direct`, `GOSUMDB=sum.golang.org`, `GOPRIVATE` | `.cargo/config.toml [source.crates-io]` |
| **Version cooldown / min release age** | `pnpm minimumReleaseAge` (native) **+** Dependabot `cooldown` | Dependabot `cooldown` only (no native) | Dependabot `cooldown` (or Renovate `minimumReleaseAge`) | Dependabot `cooldown` | Dependabot `cooldown` |
| **Behavioral scan** | Socket.dev (npm) | Socket.dev (npm) | Socket.dev (PyPI) | Socket.dev (Go) `(verify)` | Socket.dev (cargo) `(verify)`; else cargo-deny/vet |
| **Dependabot ecosystem key** | `npm` | `npm` | `pip` (poetry supported; uv limited `(verify)` → Renovate) | `gomod` | `cargo` |
| **SHA-pin CI actions** | identical — all GitHub Actions pinned to commit SHA; Dependabot `github-actions` ecosystem upgrades them | ← same | ← same | ← same | ← same |
| **Optional advisory/license/source policy** | — | — | — | — | `cargo deny` (`deny.toml`) — Tier 3 |

### Layer 3 — Code quality gate

| Control | Node / TS | Python | Go | Rust |
|---|---|---|---|---|
| **Lint** | ESLint (typescript-eslint, type-checked rules) | Ruff | golangci-lint (+ `go vet`) | `cargo clippy -- -D warnings` |
| **Format** | Prettier (+ `eslint-config-prettier`) | Ruff format (or Black) | `gofmt` / `goimports` | `cargo fmt --check` |
| **Type check** | `tsc --noEmit` | mypy or pyright | (compiler) | (compiler) |
| **Pre-commit** | Husky + lint-staged | `pre-commit` framework | `pre-commit` / lefthook | `pre-commit` / lefthook |

> The [`pre-commit`](https://pre-commit.com) framework is language-agnostic and is
> the natural choice for any non-Node repo (Husky/lint-staged is Node-centric).

### Layer 4 — Test infrastructure

| Control | Node / TS | Python | Go | Rust |
|---|---|---|---|---|
| **Runner + coverage** | Vitest (or Jest) + v8/istanbul coverage | pytest + pytest-cov | `go test ./... -race -coverprofile=...` | `cargo test` + `cargo-llvm-cov` (or tarpaulin) |
| **Fast/offline vs slow/networked split** | dir split + runner `include` (Ballast: `test/` vs `tests/`) | markers (`@pytest.mark.integration`) + `-m "not integration"` | build tags (`//go:build integration`) | `#[ignore]` + `--ignored`, or a separate `tests/` integration dir |
| **No snapshots for money math** | explicit assertions only | explicit assertions only | explicit assertions only | explicit assertions only |

### Layer 5 — CI

Same job **shape** every stack — frozen install → audit gate → lint → type-check →
test+coverage → build — with commands swapped from Layers 2–4 above, all actions
**SHA-pinned**, and the `ci` job set as a **required status check** on protected
branches. The Ballast `ci.yml` is the skeleton; only the `run:` lines change.

---

## Part C — Quick reference: what transfers vs. what you rewrite

- **Transfers 1:1 (copy):** the six-layer framework, threat model, trust-boundary
  ordering, the PreToolUse hook core, the permission-model prose, the runbook
  structure, SHA-pinning, Dependabot/Socket/cooldown concepts, the branch→dev→master
  + required-`ci` flow, "enforce in CI not just locally."
- **Rewrite per stack (Part B):** every Layer 2–4 *tool* — install-script block,
  audit tool, lockfile-frozen install, registry pin, linter/formatter/type-checker,
  test runner — plus the hook's dangerous-CLI deny list and the CI `run:` lines.

### Stack security-posture notes
- **Go** is the easiest to harden: no install-time code execution, `go.sum`+`GOSUMDB`
  give strong default integrity. Layer 2 is mostly "turn on what's already there."
- **Rust** has the one structural gap (`build.rs` runs at build with no
  `ignore-scripts`); compensate with sandboxed CI + cargo-deny/cargo-vet. This is
  why Tiers 3–4 lean Rust-heavy.
- **Python**'s risk is sdists running `setup.py` at build → prefer wheels +
  `--require-hashes`. uv is the modern default (fast, lockfile-native); poetry is
  fine but its Dependabot/tooling story differs slightly.
- **Node** carries the largest postinstall attack surface (the reason
  `ignore-scripts` is Tier-1 item #1) — but also the most mature tooling to close it.

---

_Footnotes_
- **yarn (berry):** install-script block = `enableScripts: false` + `dependenciesMeta.<pkg>.built: true`; frozen install = `yarn install --immutable`; audit = `yarn npm audit`.
- **bun:** lifecycle scripts blocked by default except `trustedDependencies`; frozen = `bun install --frozen-lockfile`; `bun audit` `(verify)`; **not yet a Dependabot ecosystem → use Renovate.**
- **uv vs poetry:** uv = `uv sync --locked`, `[tool.uv] index-url`, lockfile-native; poetry = `poetry install` + `poetry check --lock`, `[[tool.poetry.source]]`. Both export to `pip-audit`.
