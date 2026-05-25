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

**Output format for proposed commits:** one fenced bash block per logical commit, in execution order, copy-pasteable as-is at the repo root. Multi-line commit messages use heredocs so newlines and quoting survive. Put the `git push` and `gh pr create` blocks separately at the end. Do not add `Co-Authored-By: Claude` or "Generated with Claude Code" trailers — these are the user's commits.

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

TypeScript CLI for interacting with the Percolator perpetuals protocol on Solana. Targets **v12.21+** of the on-chain program exclusively (v12.20 is deprecated). Built with Commander.js, uses ESM modules, requires **Node 20+** and **pnpm**.

### Commands

```bash
pnpm install          # Install dependencies
pnpm build            # Compile via tsup → dist/ (ESM, sourcemaps, shebang)
pnpm dev              # Build + run CLI
pnpm test             # Run all 5 offline unit tests (abi, pda, slab, validation, oracle)

# Run a single unit test:
npx tsx test/abi.test.ts

# Integration tests (require RPC + SOL):
npx tsx tests/preflight.ts        # 93 checks across 3 market types
npx tsx tests/runner.ts           # Full T1-T22 integration suite

# Operational scripts:
npx tsx scripts/live-verify.ts    # 79 checks against live state
npx tsx scripts/crank-bot.ts      # Continuous keeper crank (5s intervals)
npx tsx scripts/dump-state.ts     # Full market state dump
npx tsx scripts/dump-market.ts    # Market configuration dump
```

### Source Layout

- **`src/commands/`** — One file per CLI command (~33 commands). Each exports a `registerXxx(program)` function following the pattern: validate inputs → fetch slab state → encode instruction → build account metas → `simulateOrSend()`.
- **`src/abi/`** — Low-level instruction encoding (`instructions.ts`), primitive encoders (`encode.ts` — u8/u16/u32/u64/i128/pubkey, all little-endian), account meta builders (`accounts.ts`), error parsing (`errors.ts`).
- **`src/solana/`** — On-chain state interaction: `slab.ts` (binary layout parsing with offset constants), `oracle.ts` (Pyth/Chainlink/Authority detection), `pda.ts` (LP + vault authority PDA derivation), `ata.ts`, `wallet.ts`.
- **`src/runtime/`** — `context.ts` (RPC connection + payer), `tx.ts` (instruction building, simulation, sending, result formatting).
- **`src/config.ts`** — Zod-validated config loading (CLI flags → config file → defaults).
- **`src/validation.ts`** — Input validators (pubkeys, indices, amounts).

### Key Data Flow

This is the pattern every Ballast script should follow when interacting with Percolator:

1. **Config:** `loadConfig(flags)` merges CLI flags → config file → defaults (Zod-validated)
2. **Context:** `createContext(config)` → `{ connection, payer }`
3. **State:** `fetchSlab(connection, pubkey, programId)` → raw Buffer → `parseHeader()` / `parseConfig()` / `parseEngine()` / `parseAccount()`
4. **Instruction:** `encodeXxx(args)` → Buffer (tagged, little-endian)
5. **Accounts:** `buildAccountMetas(ACCOUNTS_XXX, [pubkeys...])` → AccountMeta[]
6. **Execute:** `simulateOrSend({ connection, ix, signers, simulate })`

### On-Chain Layout Constants (v12.21)

```
SLAB_LEN    = 1,525,624 bytes
HEADER_LEN  = 136 bytes
CONFIG_LEN  = 384 bytes
ENGINE_OFF  = 520  (= align_up(136 + 384, 8))
ENGINE_LEN  = 1,492,176 bytes
PARAMS_SIZE = 168 bytes
```

### Upstream Conventions

- Command files: kebab-case filenames, `registerXxx` exports
- Functions: camelCase. Constants: UPPER_SNAKE_CASE. Types: PascalCase.
- **All numeric wire values use `bigint` for u64/u128/i128** — never use JS `number` for on-chain values (overflow risk)
- Slab state fetched fresh per command (stateless — no caching)
- Validation happens early, before any RPC calls
- Transaction errors parsed from logs via `parseErrorFromLogs()`
- Oracle type auto-detected from account owner (Pyth vs Chainlink vs Authority)

### Global CLI Flags

`--rpc <url>`, `--program <pubkey>`, `--wallet <path>`, `--commitment <level>`, `--config <path>`, `--json`, `--simulate`

### Important Upstream Constraints

- v12.21 wire format only — v12.20 instruction encoders have been removed
- `MAX_ACCRUAL_DT_SLOTS = 100` (~40 sec) — continuous cranker mandatory for live markets
- `max_price_move_bps_per_slot` must be > 0 at init
- Anti-spam: `new_account_fee` and/or `maintenance_fee_per_slot` must be nonzero at market init
- Chainlink oracle validation: must verify owner is Chainlink-owned program
- `fetchSlab()` verifies account owner matches `programId`

### Test Layout

- **`test/`** — Offline unit tests. No framework; raw assertions + console logs. Run individually with `npx tsx test/<name>.test.ts`.
- **`tests/`** — On-chain integration tests (T1-T22). Uses `harness.ts` utilities and `invariants.ts` assertions. Requires funded wallet + RPC.
- **`scripts/`** — Operational scripts, stress tests, verification tools.

---

## Ballast: Project-Specific

### Directory Structure

**All Ballast code lives in dedicated directories — never modify upstream files.**

```
scripts/ballast/               # All Ballast-specific scripts
tests/ballast/                 # Ballast unit tests
tests/ballast/integration/     # Devnet integration tests
programs/                      # On-chain programs (Rust)
  └── ballast-matcher/         # Custom allowlist matcher program
config/                        # Ballast configuration
  ├── ballast-config.json      # Deployed addresses (PUBLIC keys only)
  └── ballast-config.example.json
docs/                          # PRD, design docs
  ├── prd.md                   # Master PRD — read this first for any implementation work
  └── reports/                 # Validation reports per phase step
```

### Ballast Commands

```bash
# Ballast-specific scripts (in scripts/ballast/)
npx tsx scripts/ballast/setup-ballast-sol-market.ts   # Deploy SOL/USD slab
npx tsx scripts/ballast/setup-ballast-fx-market.ts    # Deploy FX slab (USDC collateral)
npx tsx scripts/ballast/ballast-crank-bot.ts          # Crank bot for Ballast slabs
npx tsx scripts/ballast/ballast-oracle-relay.ts       # Pyth → admin oracle relay
npx tsx scripts/ballast/ballast-dashboard.ts          # Market state dashboard

# Solana CLI
solana config set --url devnet
solana airdrop 2 --url devnet
solana balance --url devnet
spl-token wrap 1 --url devnet
spl-token accounts --url devnet

# Rust (for matcher program)
cd programs/ballast-matcher
cargo build-sbf
solana program deploy target/deploy/ballast_matcher.so --url devnet
```

### Percolator Protocol Concepts

- **Slab**: A single market account. One slab = one trading pair. Contains header, config, and risk engine state.
- **LP (Liquidity Provider)**: Special account that takes the other side of user trades. LP owner MUST co-sign every trade — this is the access control mechanism.
- **Matcher**: External Solana program called via CPI during `TradeCpi`. Determines pricing and can reject trades. Ballast's matcher adds an allowlist check.
- **Keeper Crank**: Permissionless transaction that updates mark price, accrues funding, processes liquidations. Must run every ~30s (constrained by `MAX_ACCRUAL_DT_SLOTS = 100` which is ~40 sec).
- **Oracle**: Price feed account. Percolator auto-detects type by account owner (Pyth, Chainlink, or admin-pushed).
- **Inverted Market**: Internal price is 1/spot. Used for SOL/USD where collateral is SOL. **LONG in inverted = SHORT SOL economically.**
- **Normal Market**: Internal price matches spot directly. Used for FX pairs with USDC collateral.

### Key Design Decisions

1. **LP-signature gating** is the primary access control — no protocol fork needed
2. **Admin-pushed oracle as Pyth relay** for FX pairs not in Pyth's sponsored push feeds
3. **SOL-collateralized slab first** (zero dependencies), then USDC-collateralized FX slab
4. **Separate slab per trading pair** — clean isolation, independent risk parameters
5. **All Ballast code in `scripts/ballast/`, `tests/ballast/`, `programs/`, `config/`, `docs/`** — upstream untouched

### Wallet Architecture

```
~/.config/ballast/
├── ballast-hedger.json            # Wallet A: CargoBill hedger (user role)
├── ballast-lp.json                # Wallet B: Counterparty LP
├── ballast-oracle-authority.json  # Oracle price pusher (FX relay, freight)
└── ballast-admin.json             # Market admin (optional, can reuse LP)
```

**CRITICAL: Devnet keypairs only. Never reuse for mainnet. Never commit to git.**

### PRD Reference

The master PRD is at `docs/prd.md`. It defines:

- **Phase 0 Step 0.0**: SOL/USD slab with Pyth oracle (technical validation)
- **Phase 0 Step 0.1**: USDC-collateralized EUR/USD slab with Pyth relay oracle (business validation)
- **Phase 0 Step 0.2**: Additional FX pairs (INR/USD, USDT/USD, etc.)
- **Phase 1**: Freight rate hedging with FBX admin oracle

Each phase has numbered success criteria (SC-0.1 through SC-0.10, SC-1.1 through SC-1.8) that must be validated and documented.

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
- See [`docs/supply-chain-hardening.md`](docs/supply-chain-hardening.md) for the npm/cargo supply-chain controls (planned)

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
- Co-locate with source: `scripts/ballast/utils.ts` → `tests/ballast/utils.test.ts`
- Test: price format transformations, config loading, allowlist validation, PnL calculations
- Don't test: Solana RPC calls, on-chain state (use integration tests)

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
