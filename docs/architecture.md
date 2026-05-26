# Architecture Reference — Percolator CLI

Full reference for the upstream Percolator CLI that Ballast extends. Keep this doc updated when upstream changes. For the day-to-day briefing (conventions, critical constraints, data flow pattern) see [`CLAUDE.md`](../CLAUDE.md).

---

## Upstream: Percolator CLI

TypeScript CLI for interacting with the Percolator perpetuals protocol on Solana. Targets **v12.21+** of the on-chain program exclusively (v12.20 is deprecated and its encoders have been removed). Built with Commander.js, uses ESM modules, requires **Node 20+** and **pnpm**.

### All commands

```bash
# Dev
pnpm install          # Install dependencies
pnpm build            # Compile via tsup → dist/ (ESM, sourcemaps, shebang)
pnpm dev              # Build + run CLI
pnpm test             # Run all 5 offline unit tests (abi, pda, slab, validation, oracle)

# Run a single unit test:
npx tsx test/abi.test.ts

# Integration tests (require funded devnet wallet + RPC):
npx tsx tests/preflight.ts        # 93 checks across 3 market types
npx tsx tests/runner.ts           # Full T1-T22 integration suite

# Operational scripts:
npx tsx scripts/live-verify.ts    # 79 checks against live state
npx tsx scripts/crank-bot.ts      # Continuous keeper crank (5s intervals)
npx tsx scripts/dump-state.ts     # Full market state dump
npx tsx scripts/dump-market.ts    # Market configuration dump

# Ballast-specific scripts (in scripts/ballast/):
npx tsx scripts/ballast/setup-ballast-sol-market.ts   # Deploy SOL/USD slab
npx tsx scripts/ballast/setup-ballast-fx-market.ts    # Deploy FX slab (USDC collateral)
npx tsx scripts/ballast/ballast-crank-bot.ts          # Crank bot for Ballast slabs
npx tsx scripts/ballast/ballast-oracle-relay.ts       # Pyth → admin oracle relay
npx tsx scripts/ballast/ballast-dashboard.ts          # Market state dashboard
```

### Source layout

```
src/
├── commands/     # One file per CLI command (~33 commands).
│                 # Each exports registerXxx(program): validate → fetch → encode → build → send
├── abi/
│   ├── instructions.ts  # Instruction encoders (encodeXxx functions, IX_TAG enum)
│   ├── encode.ts        # Primitive encoders: encU8/U16/U32/U64/I64/U128/I128/Pubkey (little-endian)
│   ├── accounts.ts      # Account meta builders (buildAccountMetas, ACCOUNTS_* constants)
│   └── errors.ts        # Error parsing from transaction logs
├── solana/
│   ├── slab.ts    # Binary layout parsing: parseHeader/parseConfig/parseEngine/parseAccount
│   │              # SLAB_LEN, HEADER_LEN, CONFIG_LEN, ENGINE_OFF offset constants
│   ├── oracle.ts  # Oracle type detection (Pyth / Chainlink / Authority by account owner)
│   ├── pda.ts     # LP + vault authority PDA derivation
│   ├── ata.ts     # Associated token account helpers
│   └── wallet.ts  # Keypair loading
├── runtime/
│   ├── context.ts # createContext(config) → { connection, payer }
│   └── tx.ts      # buildInstruction, simulateOrSend, result formatting
├── config.ts      # loadConfig(flags): CLI flags → config file → defaults (Zod-validated)
└── validation.ts  # Input validators: pubkeys, indices, amounts
```

### Ballast directory structure

All Ballast code lives in dedicated directories — upstream `src/`, `scripts/`, `tests/` are never modified.

```
scripts/ballast/               # All Ballast-specific scripts
tests/ballast/                 # Ballast unit tests
tests/ballast/integration/     # Devnet integration tests
programs/                      # On-chain programs (Rust)
  └── ballast-matcher/         # Custom allowlist matcher program
config/                        # Ballast configuration
  ├── ballast-config.json      # Deployed addresses (PUBLIC keys only — never private keys)
  └── ballast-config.example.json
docs/                          # PRDs, design docs, runbook, reports
  ├── prd.md                   # Master PRD — read this first for any implementation work
  ├── architecture.md          # This file
  ├── runbook.md               # Incident response procedures
  ├── supply-chain-hardening.md
  └── reports/                 # Validation reports per phase step (phase-X-step-Y-report.md)
```

### Key data flow

Every Ballast script must follow this pattern when interacting with Percolator:

1. **Config** — `loadConfig(flags)` merges CLI flags → config file → defaults (Zod-validated)
2. **Context** — `createContext(config)` → `{ connection, payer }`
3. **State** — `fetchSlab(connection, pubkey, programId)` → raw Buffer → `parseHeader()` / `parseConfig()` / `parseEngine()` / `parseAccount()`
4. **Instruction** — `encodeXxx(args)` → Buffer (tagged discriminator prefix, little-endian fields)
5. **Accounts** — `buildAccountMetas(ACCOUNTS_XXX, [pubkeys...])` → `AccountMeta[]`
6. **Execute** — `simulateOrSend({ connection, ix, signers, simulate })`

### On-chain layout constants (v12.21)

```
SLAB_LEN    = 1,525,624 bytes
HEADER_LEN  = 136 bytes
CONFIG_LEN  = 384 bytes
ENGINE_OFF  = 520  (= align_up(136 + 384, 8))
ENGINE_LEN  = 1,492,176 bytes
PARAMS_SIZE = 168 bytes
```

Breaking any of these means corrupt reads from the binary layout. Always cross-reference against the deployed program version before changing offset math.

### Upstream conventions

- Command files: kebab-case filenames, `registerXxx(program)` export pattern
- Functions: camelCase. Constants: UPPER_SNAKE_CASE. Types: PascalCase.
- **All numeric wire values use `bigint`** — `u64`/`u128`/`i128` silently overflow as JS `number`
- Slab state fetched fresh per command — stateless, no caching
- Validation happens before any RPC call
- Transaction errors parsed from logs via `parseErrorFromLogs()`
- Oracle type auto-detected from account owner (Pyth vs Chainlink vs Authority)

### Global CLI flags

`--rpc <url>`, `--program <pubkey>`, `--wallet <path>`, `--commitment <level>`, `--config <path>`, `--json`, `--simulate`

### Important upstream constraints

| Constraint | Detail |
|---|---|
| v12.21 wire format only | v12.20 instruction encoders removed — do not use |
| `MAX_ACCRUAL_DT_SLOTS = 100` | ~40 sec; continuous cranker mandatory; prepend `KeeperCrank` to any one-shot engine op |
| `max_price_move_bps_per_slot` | Must be > 0 at market init |
| Anti-spam | `new_account_fee` and/or `maintenance_fee_per_slot` must be nonzero at init |
| Chainlink oracle | Must verify account owner is a Chainlink-owned program |
| `fetchSlab()` | Verifies account owner matches `programId` — wrong program ID = silent failure |

### Test layout

| Directory | Purpose | Runner | RPC required |
|---|---|---|---|
| `test/` | 5 offline unit tests (abi, pda, slab, validation, oracle) | `pnpm test` (Vitest) | No |
| `tests/` | T1-T22 on-chain integration suite + harness + invariants | `npx tsx tests/runner.ts` | Yes (devnet) |
| `tests/ballast/` | Ballast-specific unit and integration tests | Vitest / tsx | No / Yes |
| `scripts/` | Operational scripts, stress tests, live verification | `npx tsx scripts/<name>.ts` | Yes (devnet) |

### Percolator protocol concepts

| Term | Definition |
|---|---|
| **Slab** | Single market account. One slab = one trading pair. Contains header, config, risk engine. |
| **LP** | Liquidity Provider. Takes the other side of user trades. LP owner must co-sign every trade — this is the access control mechanism. |
| **Matcher** | External Solana program called via CPI during `TradeCpi`. Determines pricing, can reject trades. Ballast adds an allowlist check. |
| **Keeper Crank** | Permissionless tx: updates mark price, accrues funding, processes liquidations. Must run every ~30s. |
| **Oracle** | Price feed account. Auto-detected by account owner: Pyth, Chainlink, or admin-pushed. |
| **Inverted Market** | Internal price = 1/spot. SOL/USD uses this (collateral is SOL). LONG in inverted = SHORT SOL economically. |
| **Normal Market** | Internal price matches spot directly. FX pairs with USDC collateral. |

### Key design decisions

1. **LP-signature gating** is the primary access control — no protocol fork needed
2. **Admin-pushed oracle as Pyth relay** for FX pairs not in Pyth's sponsored push feeds
3. **SOL-collateralized slab first** (zero dependencies), then USDC-collateralized FX slab
4. **Separate slab per trading pair** — clean isolation, independent risk parameters
5. **All Ballast code in dedicated directories** — upstream source is never modified

### Wallet architecture

```
~/.config/ballast/
├── ballast-hedger.json            # Wallet A: CargoBill hedger (user role)
├── ballast-lp.json                # Wallet B: Counterparty LP
├── ballast-oracle-authority.json  # Oracle price pusher (FX relay, freight)
└── ballast-admin.json             # Market admin (optional, can reuse LP)
```

**CRITICAL: Devnet keypairs only. Never reuse for mainnet. Never commit to git.**

### PRD phases

The master PRD at `docs/prd.md` defines:

| Phase | Step | Description |
|---|---|---|
| Phase 0 | 0.0 | SOL/USD slab with Pyth oracle (technical validation) |
| Phase 0 | 0.1 | USDC-collateralized EUR/USD slab with Pyth relay oracle |
| Phase 0 | 0.2 | Additional FX pairs (INR/USD, USDT/USD, etc.) |
| Phase 1 | — | Freight rate hedging with FBX admin oracle |

Each phase has numbered success criteria (SC-0.1 through SC-0.10, SC-1.1 through SC-1.8) validated in `docs/reports/`.
