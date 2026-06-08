# CLAUDE.md

## Project Overview

**Ballast** is a proof-of-concept for compliant on-chain bilateral derivatives (FX and freight rate hedging) built on Anatoly Yakovenko's Percolator perpetual futures protocol on Solana devnet. It is a research project by CargoBill — a stablecoin payments platform for supply chain/logistics companies.

This repo is a fork of `aeyakovenko/percolator-cli` extended with CargoBill-specific scripts, a custom allowlist matcher program, and supporting infrastructure for two use cases: FX hedging (EUR/USD, INR/USD, etc.) and freight rate hedging.

**This is a devnet-only POC. No real funds. No mainnet deployment.**

---

## Workflow Rule — Claude Does Not Run Commits, Pushes, or PRs

**Claude (Code) does not execute commands that mutate shared state.** When a step calls for a commit, push, tag, PR, or deploy / on-chain-write action, Claude emits a single fenced bash block with the exact command(s) — heredoc'd commit message included — for the user to run in the VS Code terminal at the repo root. The user reviews the staged diff, runs the block, and reports back. Claude does **not** retry the same command after a denial; it produces the terminal block and waits.

**Applies to:**

- `git commit`, `git push`, `git tag`, `git push --tags`
- Any history-rewriting op: `git rebase`, `git reset --hard`, `git revert`, `git cherry-pick`, `git commit --amend`, `git branch -D`, `git stash drop`
- `gh pr create | merge | close`, `gh issue create | close`, `gh release create`, `gh workflow run`
- The `/create-pr` skill — Claude must NOT invoke it; emit the equivalent `gh pr create` command instead
- `solana program deploy | close | upgrade`, any `solana` or RPC call submitting a transaction signed by a Ballast keypair, `spl-token` writes (mint, burn, transfer, approve)
- Any `cargo publish`, `npm publish`, `pnpm publish`

**Free without confirmation (read-only):** `git status | diff | log | show | branch | check-ignore | ls-files`, `gh pr view | list | checks`, `gh issue view`, `solana account | program show | balance`, `getAccountInfo`-style RPC queries.

**Staging is fine** — Claude may run `git add <specific paths>` and `git restore --staged <paths>` to prepare a clean index for the user to inspect. Never `git add -A` or `git add .`. If the index already contains files Claude did not stage, leave them alone or flag them.

**Output format for proposed commits:** one fenced bash block per logical commit, in execution order, copy-pasteable as-is at the repo root. Multi-line commit messages use heredocs so newlines and quoting survive. Put the `git push` and `gh pr create` blocks separately at the end. Add a `Co-Authored-By: Claude Code <noreply@anthropic.com>` trailer on all Claude-assisted commits — required for audit trail in regulated financial software.

**Why:** Claude runs in a sandbox without credentials for git pushes / `gh` API / on-chain signers, and the user wants to review every state-changing action before it lands. Don't ask permission per command — emit the block, wait for the user to run it, move on.

**Enforcement:** This rule is enforced by `.claude/hooks/block-mutating-commands.mjs` (PreToolUse hook on the Bash tool). Mutating commands return a structured `permissionDecision: "deny"` to Claude Code, which bypasses `--dangerously-skip-permissions` mode by design. The hook recurses into `bash -c`, `sh -c`, `env`, `nice`, `xargs`, `sudo` wrappers and quote-aware-splits pipelines so `cd subdir && git push` and `bash -c "git push"` are also caught. Every denial is logged to `~/.cache/ballast/events.jsonl` as `event_type: "GUARDRAIL_DECISION"`. There is no override flag at this layer.

---

## Permission Model

This repo operates under **allowlist-first** for bash commands. Concretely:

- `.claude/settings.json` `permissions.allow` lists ~150 patterns covering normal dev work (filesystem read, git read + targeted staging, cargo build/test, pnpm test/build, npx tsx, solana read-only queries, gh read-only, common system tools). These auto-execute without prompting.
- `.claude/settings.json` `permissions.deny` hard-blocks specific catastrophic forms (`solana program deploy*`, `spl-token transfer*`, `git push --force*`, install commands, `mainnet` substring, etc.) — these cannot be approved even at a prompt.
- `.claude/hooks/block-mutating-commands.mjs` (PreToolUse) handles complex policy that can't be expressed as globs (tokenization, recursion, conditional matching, eval-escape-hatch detection, `rm -rf <root>` protection, `git -c <dangerous-key>=`).
- Anything not matching `allow`, `deny`, or the hook **prompts the user**. This is the allowlist-first default.
- `sandbox.autoAllowBashIfSandboxed: false` in committed settings enforces this. Any developer who flips it `true` in their local `settings.local.json` reverts to the pre-Phase-0 permission model and is on their own.

This model is non-negotiable for any repo touching mainnet keys, mainnet RPC, or production data. It also fits the devnet POC because the same patterns travel to mainnet without changes.

When the agent needs a new command pattern that isn't allowlisted, the prompt will surface it. Promote frequently-used patterns to committed `permissions.allow`; keep per-developer experimental patterns in `settings.local.json`.

Supply-chain controls (npm/cargo dep trust, postinstall script blocking, SBOM generation, Socket.dev) are tracked separately in [`docs/supply-chain-hardening.md`](docs/supply-chain-hardening.md).

---

## Upstream: Percolator CLI

TypeScript CLI for Percolator perpetuals on Solana. Targets **v12.21+** exclusively (v12.20 encoders removed). Node 22+, pnpm, ESM, Commander.js. Full source layout, all commands, protocol concepts, and PRD phases: [`docs/architecture.md`](docs/architecture.md).

### Common commands

```bash
pnpm build                          # tsup → dist/ (ESM + sourcemaps)
pnpm test                           # 5 offline unit tests via Vitest
npx tsx tests/preflight.ts          # 93 preflight checks (devnet)
npx tsx tests/runner.ts             # Full T1-T22 integration suite (devnet)
cargo build-sbf                     # Build matcher program (run in programs/ballast-matcher/)
```

### Key data flow — every Ballast script must follow this pattern

1. **Config** — `loadConfig(flags)` → Zod-validated merge of CLI flags → config file → defaults
2. **Context** — `createContext(config)` → `{ connection, payer }`
3. **State** — `fetchSlab(connection, pubkey, programId)` → Buffer → `parseHeader()` / `parseConfig()` / `parseEngine()` / `parseAccount()`
4. **Instruction** — `encodeXxx(args)` → Buffer (tagged discriminator, little-endian fields)
5. **Accounts** — `buildAccountMetas(ACCOUNTS_XXX, [pubkeys...])` → `AccountMeta[]`
6. **Execute** — `simulateOrSend({ connection, ix, signers, simulate })`

### On-chain layout constants (v12.21)

```
SLAB_LEN = 1,525,624 | HEADER_LEN = 136 | CONFIG_LEN = 384
ENGINE_OFF = 520      | ENGINE_LEN = 1,492,176 | PARAMS_SIZE = 168
```

### Critical constraints

- **`bigint` for all on-chain numerics** — `u64`/`u128`/`i128` silently overflow as JS `number`
- **`MAX_ACCRUAL_DT_SLOTS = 100`** (~40 sec) — prepend `KeeperCrank` to any one-shot engine op; see memory note `percolator-oracle-stale-gate.md`
- **Stateless** — fetch slab fresh per command, no caching
- **v12.21 wire format only** — do not reference removed v12.20 encoders
- **Anti-spam** — `new_account_fee` and/or `maintenance_fee_per_slot` must be nonzero at init
- **`fetchSlab()`** verifies account owner matches `programId` — wrong program ID silently fails

---

## Ballast: Project-Specific

**All Ballast code lives in dedicated directories — never modify upstream `src/`, `scripts/`, or `tests/` files.**

```
scripts/ballast/          # Ballast scripts (crank bot, oracle relay, market setup, dashboard)
tests/ballast/            # Ballast unit + integration tests
programs/ballast-matcher/ # On-chain allowlist matcher program (Rust)
config/                   # ballast-config.json (PUBLIC keys only) + .example.json
docs/                     # prd.md, architecture.md, runbook.md, supply-chain-hardening.md, reports/
```

### Wallet architecture

```
~/.config/ballast/
├── ballast-hedger.json            # Wallet A: CargoBill hedger (user role)
├── ballast-lp.json                # Wallet B: Counterparty LP
├── ballast-oracle-authority.json  # Oracle price pusher (FX relay, freight)
└── ballast-admin.json             # Market admin (optional, can reuse LP)
```

**CRITICAL: Devnet keypairs only. Never reuse for mainnet. Never commit to git.**

### PRD reference

Master PRD at `docs/prd.md`. Phase 0: SOL/USD slab (step 0.0) → EUR/USD USDC slab (step 0.1) → additional FX pairs (step 0.2). Phase 1: freight rate hedging with FBX oracle. Success criteria SC-0.1 through SC-1.8 validated in `docs/reports/`.

---

## Coding Conventions

- TypeScript strict mode for all new code
- **All Ballast scripts go in `scripts/ballast/`** — never modify upstream scripts in `scripts/`
- **All Ballast tests go in `tests/ballast/`**
- Follow upstream conventions: camelCase functions, UPPER_SNAKE_CASE constants, PascalCase types
- **Use `bigint` for all on-chain numeric values** (u64, u128, i128) — never JS `number`
- Follow upstream data flow pattern: Config → Context → State → Instruction → Accounts → Execute
- Every script must have a header comment: purpose, prerequisites, PRD step reference
- Use `config/ballast-config.json` for all pubkeys — never hardcode addresses
- Conventional commits: `feat:`, `fix:`, `chore:`, `refactor:`, `docs:`, `test:`
- Branch naming: `feat/`, `fix/`, or `chore/` prefix + short kebab-case (e.g., `feat/phase-0-sol-slab`)
- PR titles use conventional commit format

## Security Rules

- NEVER read, access, or reference .env, .env.local, or any .env.* files
- NEVER hardcode private keys, keypair data, or seed phrases in code
- NEVER commit keypair JSON files — they must be in .gitignore
- NEVER log full keypair data, private keys, or oracle authority keys
- All keypair paths use `~/.config/ballast/` — never relative paths in the repo
- Oracle authority, LP, admin, and user keypairs MUST be separate files — no key reuse
- `ballast-config.json` stores PUBLIC addresses only — never private keys
- Matcher allowlist contains PUBLIC keys only
- NEVER reference mainnet RPC URLs or deploy to mainnet
- See [`docs/supply-chain-hardening.md`](docs/supply-chain-hardening.md) for npm/cargo supply-chain controls (Tier 1+2 implemented; Tier 3-4 planned)

## Event Logging

All state-changing operations log to `~/.cache/ballast/events.jsonl`:
```json
{
  "timestamp": "ISO-8601",
  "event_type": "MARKET_DEPLOY | USER_INIT | DEPOSIT | WITHDRAW | TRADE | CRANK | LIQUIDATION | ORACLE_PUSH | STATE_SNAPSHOT | INCIDENT | GUARDRAIL_DECISION",
  "tx_signature": "base58 signature",
  "slab": "slab pubkey",
  "actor": "wallet pubkey",
  "details": { ... }
}
```

`INCIDENT` events are appended during the response loop documented in [`docs/runbook.md`](docs/runbook.md). `GUARDRAIL_DECISION` events are appended automatically by `.claude/hooks/block-mutating-commands.mjs` on every block.

## Testing

### Unit Tests
- Runner: **Vitest** (`pnpm test` / `pnpm test:coverage`). Test files live in `test/` (offline, no RPC).
- Ballast unit tests: `tests/ballast/utils.test.ts` co-located with source
- Test: price format transformations, config loading, allowlist validation, PnL calculations
- Don't test: Solana RPC calls, on-chain state (use integration tests)
- No snapshots — explicit value assertions only (financial data must be exact)

### Integration Tests
- Shell scripts or tsx scripts against devnet, in `tests/ballast/integration/`
- Must be idempotent — can run repeatedly without manual cleanup
- Document required pre-conditions (funded wallets, deployed slab, etc.)

### Validation Reports
- After completing each PRD step, create `docs/reports/phase-X-step-Y-report.md`
- Include: success criteria results, transaction signatures, state snapshots, discrepancies

## Important Notes

- UNAUDITED, EXPERIMENTAL protocol — devnet only, no real funds
- Pyth push feeds update every ~1 minute on devnet — crank bot must account for staleness
- Circle devnet USDC faucet: 20 USDC per wallet every 2 hours
- Devnet SOL faucet: 2 SOL per request with cooldown — accumulate aggressively early
