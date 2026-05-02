# Pyth Oracle Compatibility — Devnet Findings

**Status:** Phase 0 scaffolding research (PRD Phase 0 Setup Step 3)
**Date:** 2026-05-01
**Audience:** CargoBill engineering

This document answers the kickoff question: *can Ballast use Pyth instead of Chainlink for the SOL/USD slab on devnet, and what is the exact wire-up?*

## TL;DR

**Yes — Pyth is the supported primary oracle for v12.21+ Percolator.** The upstream `init-market` command is documented as *"Pyth Pull oracle; Hyperp when index feed is zero"* and accepts a Pyth `PriceUpdateV2` account directly via `--oracle`. No client-side preprocessing is needed; the Percolator program auto-detects oracle type by account owner. For the Ballast SOL/USD slab, pass:

| Flag | Value |
|---|---|
| `--oracle` | `7UVimffxr9ow1uXYxsr4LHAcV58mLzhmwaeKvJ1pjLiE` (sponsored devnet push feed, shard 0) |
| `--index-feed-id` | `ef0d8b6fda2ceba41da15d4095d1da392a0d2f8ed0c6c7bc0f4cfac8c280b56d` |

## 1. Pyth deployments on Solana devnet (verified)

All addresses below were confirmed live via `getAccountInfo` against `https://api.devnet.solana.com` on 2026-05-01.

| Component | Address | Notes |
|---|---|---|
| Pyth Solana Receiver program | `rec5EKMGg6MxZYaMdyBfgwp4d5rB9T1VQH5pJv5LtFJ` | Owns every `PriceUpdateV2` account; same program ID on mainnet and devnet |
| Pyth helper program | `pythWSnswVUd12oZpeFP8e9CVaEqJg25g1Vtc2biRsT` | Listed as the "price feed program" in Pyth docs |
| Sponsored SOL/USD push feed (shard 0) | `7UVimffxr9ow1uXYxsr4LHAcV58mLzhmwaeKvJ1pjLiE` | 134-byte account, owner = receiver, ~5.25 SOL rent-exempt; live updates |

Pyth sponsors ~43 feeds on Solana (mainnet + devnet) at a 1-minute heartbeat / 0.5% deviation cadence. Crypto majors (SOL/USD, BTC/USD, ETH/USD, USDC/USD, USDT/USD) are sponsored. **FX feeds (EUR/USD, USD/INR, GBP/USD) are NOT in the sponsored set on Solana** — their feed IDs exist on the cross-chain price service (Hermes), but no on-chain push feed account is maintained on devnet. This drives the Phase 0.1 design decision below.

## 2. Pyth feed IDs (cross-chain, populated in `config/ballast-config.example.json`)

| Symbol | Feed ID (hex, 64 chars, no `0x`) | Sponsored on Solana devnet? |
|---|---|---|
| Crypto.SOL/USD | `ef0d8b6fda2ceba41da15d4095d1da392a0d2f8ed0c6c7bc0f4cfac8c280b56d` | ✅ shard 0 = `7UVimffxr9ow1uXYxsr4LHAcV58mLzhmwaeKvJ1pjLiE` |
| FX.EUR/USD | `a995d00bb36a63cef7fd2c287dc105fc8f3d93779f062f09551b0af3e81ec30b` | ❌ Hermes only |
| FX.USD/INR | `0ac0f9a2886fc2dd708bc66cc2cea359052ce89d324f45d95fadbc6c4fcf1809` | ❌ Hermes only (note: USD/INR — invert in software for INR/USD displays) |
| FX.GBP/USD | `84c2dde9633d93d1bcad84e7dc41c9d56578b7ec52fabedc1f335d673df0a7c1` | ❌ Hermes only |
| Crypto.USDC/USD | `eaa020c61cc479712813461ce153894a96a6c00b21ed0cfc2798d1f9a9e9c94a` | ✅ (account address not yet confirmed; derive via shard 0 PDA) |
| Crypto.USDT/USD | `2b89b9dc8fdf9f34709a5b106b472f0f39bb6ca9ce04b0fd7f2e971688e2e53b` | ✅ (account address not yet confirmed; derive via shard 0 PDA) |

`PriceUpdateV2` PDAs are derived as `find_program_address([shard_id_le_2_bytes, feed_id_32_bytes], rec5EK…)`. Shard 0 is the canonical sponsored shard. Use `@pythnetwork/pyth-solana-receiver` to derive these in TypeScript (already a dependency in `package.json`).

## 3. Upstream CLI / on-chain support

Findings from the upstream `percolator-cli` source (read-only — no upstream files modified):

- **`src/commands/init-market.ts`**: `--oracle <pubkey>` is described as *"Oracle account (Pyth PriceUpdateV2 / Chainlink aggregator; for Hyperp pass any pubkey)"*. `--index-feed-id <hex>` takes the 64-hex-char Pyth feed ID and is required (zeros for Hyperp). The instruction encoder (`encodeInitMarket`) ships the feed-id hash into the slab config.
- **`src/commands/keeper-crank.ts`** and **`src/commands/trade-cpi.ts`**: pass the oracle account pubkey through unmodified — no client-side Pyth decoding. The on-chain program reads and verifies the account itself.
- **`src/solana/oracle.ts`**: only contains a Chainlink aggregator parser (`parseChainlinkPrice`) used by setup scripts for sanity logging (e.g., `scripts/setup-devnet-market.ts` decodes Chainlink before calling `init-market`). There is no Pyth parser in TypeScript today; we'll need a small `parsePythPrice` helper for log-time price display in `scripts/ballast/setup-ballast-sol-market.ts`. (`@pythnetwork/pyth-solana-receiver` provides the layout — minor work.)
- **`src/abi/errors.ts`**: contains a Pyth-specific hint string ("Check the oracle account is a valid Pyth price feed"), confirming the program path.
- **`package.json`**: already depends on `@pythnetwork/hermes-client` and `@pythnetwork/pyth-solana-receiver`. No new deps needed.

The existing devnet helper script `scripts/setup-devnet-market.ts` is hard-coded to Chainlink (`99B2bTijsU6f1GCT73HmdR7HCFFjGMBcPZY6jZ96ynrR`) — this is just an example; we will fork its structure into `scripts/ballast/setup-ballast-sol-market.ts` and swap in the Pyth oracle.

## 4. Existing devnet test market — gone

The PRD §2.4 references an existing devnet slab at `A7wQtRT9DhFqYho8wTVqQCDc7kYPTUXGPATiyVbZKVFs`. Querying devnet today returns `null` for that account — it has been closed or reset. The Percolator program at `2SSnp35m7FQ7cRLNKGdW5UzjYFF6RBUNq7d3m5mqNByp` is still live, so this does not block Ballast (we deploy fresh slabs anyway, per PRD §3.1). It does mean the kickoff plan's "verify existing market is accessible" check cannot pass — flag for the team and update PRD §2.4 when convenient.

## 5. Phase 0 oracle plan

| Slab | Mode | Oracle account | Feed ID | Notes |
|---|---|---|---|---|
| `solUsd` (Phase 0 Step 0.0) | Pyth Pull (primary) | `7UVimffxr9ow1uXYxsr4LHAcV58mLzhmwaeKvJ1pjLiE` | SOL/USD | Replace PRD §4.4 "Chainlink primary" with Pyth; admin-pushed remains as fallback for SC-0.4 / SC-0.9 controlled scenarios |
| `eurUsd` (Phase 0 Step 0.1) | Admin-pushed (Hermes relay) | (slab itself, Hyperp-style) | EUR/USD via Hermes | Use `oracle_authority` + `PushOraclePrice` on a 15-min cadence; no sponsored Pyth account exists on devnet |
| Future FX (INR, GBP, etc.) | Same Hermes-relay pattern | — | per table above | One slab per pair |

**Oracle authority key** (`~/.config/ballast/ballast-oracle-authority.json`) is a **separate keypair** from LP / hedger / admin per CLAUDE.md security rules. The Hermes relay script (`scripts/ballast/ballast-oracle-relay.ts`, to be written in PRD Step 0.5) will be the only process that uses it.

## 6. Verification commands

Once a wallet is configured, the SOL/USD oracle wire-up can be sanity-checked with:

```bash
# Confirm the Pyth account is alive and owned by the receiver program
solana account 7UVimffxr9ow1uXYxsr4LHAcV58mLzhmwaeKvJ1pjLiE --url devnet

# Confirm the receiver program is executable
solana program show rec5EKMGg6MxZYaMdyBfgwp4d5rB9T1VQH5pJv5LtFJ --url devnet
```

For runtime price display, `@pythnetwork/pyth-solana-receiver`'s `PythSolanaReceiver` class exposes `getPriceFromAccount(pubkey)` which decodes `PriceUpdateV2` and returns `{ price, conf, exponent, publishTime }` — this is what we'll wrap into a small `scripts/ballast/utils/pyth.ts` helper alongside `setup-ballast-sol-market.ts`.

## 7. Open items for Phase 0 execution

1. Derive and verify shard-0 PDAs for USDC/USD and USDT/USD on devnet (write a short helper script using `@pythnetwork/pyth-solana-receiver`). **Status: open**, defer to Step 0.1 (EUR/USD slab).
2. ~~Decide `max_staleness_secs` and `conf_filter_bps` for `init-market`~~ **Resolved 2026-05-01:** `max_staleness_secs=120` and `conf_filter_bps=200`, applied in `scripts/ballast/setup-ballast-sol-market.ts`.
3. ~~Confirm Pyth's `invert` flag (`--invert 1`) is the correct path for the inverted SOL/USD market~~ **Resolved 2026-05-01 (PR #3):** `invert=1` deployed cleanly. `engine.lastOraclePrice = 11954` after warmup crank, which equals `1e12 / SOL_USD_e6 ≈ 11955` at SOL ≈ \$83.65 — within rounding. Pyth Pull + invert is the correct path.
4. Build a Hermes-relay watchdog (PRD Step 0.5) that pushes EUR/USD on a 15-min cadence and aborts gracefully if Hermes is unreachable. **Status: open.**

## 8. Operational findings (post-deploy)

- **`PriceUpdateV2` `VerificationLevel` is variable-size** (Full=1 byte, Partial=2 bytes). Sponsored push feeds use Full, so `price_message` starts at byte 41, not 42. The hand-rolled parser in `scripts/ballast/utils/pyth.ts` reads the variant tag at byte 40 and branches accordingly. An earlier draft assumed fixed-2 — produced garbage prices.
- **Engine-state ops gate on `lastGoodOracleSlot` freshness, not Pyth freshness.** `TopUpInsurance`, `InitLP`, `DepositCollateral`, and `Trade*` all revert with `OracleStale (0x6)` if more than ~`permissionlessResolveStaleSlots` (= 100 slots ≈ 40 s) elapsed since the last `KeeperCrank`. The Pyth feed itself can be fresh and you'll still get the error. Without the keeper bot from PRD Step 0.5 running continuously, every one-shot Ballast script must prepend a `KeeperCrank` instruction to the same transaction. The matcher PR will introduce a `prependCrankIfStale()` helper.

## 9. References

- [Pyth Solana Receiver SDK](https://github.com/pyth-network/pyth-crosschain/tree/main/target_chains/solana/pyth_solana_receiver_sdk) — `PriceUpdateV2` layout, PDA derivation
- [Pyth Pull Oracle on Solana](https://docs.pyth.network/price-feeds/core/use-real-time-data/pull-integration/solana) — receiver program ID, Anchor account type
- [Pyth Sponsored Push Feeds — Solana](https://docs.pyth.network/price-feeds/core/push-feeds/solana) — heartbeat / deviation cadence, sponsored feed list
- [Pyth Price Feed IDs](https://docs.pyth.network/price-feeds/price-feeds) — full cross-chain feed registry
- Upstream `percolator-cli` `src/commands/init-market.ts:17,21` — Pyth Pull oracle support
