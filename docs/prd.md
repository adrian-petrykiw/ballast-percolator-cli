# Ballast — Product Requirements Document: Phase 0 & Phase 1

**Document Version:** 1.0
**Date:** April 29, 2026
**Author:** CargoBill Engineering
**Classification:** Confidential — CargoBill Inc.
**Status:** Draft — Pending CTO Review

---

## 0. Document Purpose & Scope

This PRD defines the technical requirements, architecture, acceptance criteria, and execution plan for Ballast Phase 0 (Treasury Hedging) and Phase 1 (Freight Rate Hedging) — two proof-of-concept applications built on Anatoly Yakovenko's Percolator perpetual futures protocol on the Solana blockchain.

This document does NOT cover Phase 2 (Next.js UI application), production deployment, or regulatory compliance implementation beyond what is necessary to validate the POC's defensibility. It does, however, flag every decision point where production compliance intersects with POC architecture choices, because choosing the wrong abstraction boundary during POC will create expensive rework later.

**Intended audience:** CargoBill CTO (Adrian), engineering team, and any external counsel or advisors reviewing the technical architecture for regulatory defensibility.

---

## 1. Strategic Context

### 1.1 Why Ballast Exists

CargoBill is building a verticalized financial operating system for the global logistics industry. The platform today handles cross-border stablecoin payments (USDC/EURC on Solana), with yield, credit, and US bank accounts as expanding layers. The company holds SOL and stablecoins as operational treasury and serves freight forwarders whose core business risk is freight rate volatility.

Ballast explores whether Percolator — an open-source, Apache-2.0, formally-verified perpetual futures risk engine on Solana — can serve as infrastructure for two commercial hedging use cases:

**Use Case A (Phase 0):** CargoBill hedging its own SOL/stablecoin treasury exposure using an on-chain perpetual future.

**Use Case B (Phase 1):** Two freight forwarding companies hedging opposing freight rate exposure via a bespoke on-chain perpetual future referencing a freight rate index.

Both use cases are structured as bilateral swaps between Eligible Contract Participants (ECPs) under the Commodity Exchange Act, leveraging the ECP-to-ECP bilateral exemption to operate without DCM/SEF registration.

### 1.2 Why Now

The regulatory window is unusually favorable. As of April 2026, CFTC Chairman Michael Selig has explicitly stated the agency will "clear a path" for U.S. perpetual futures. The SEC and CFTC signed a formal MOU on March 11, 2026, establishing joint "Project Crypto" with six areas of coordination. Both agencies are prepared to consider "innovation exemptions" for peer-to-peer trading of derivatives including perpetual contracts over DeFi protocols. The CFTC's Innovation Task Force launched in March 2026 is specifically developing frameworks for DeFi software providers. A compliant bilateral hedging POC positions CargoBill to be an early mover as these frameworks materialize.

### 1.3 What Ballast Is Not

Ballast is not a derivatives exchange. It is not a trading platform. It is not multi-participant price discovery. It is bilateral settlement infrastructure — two known, KYB-verified counterparties executing a negotiated hedge. This distinction is not cosmetic; it is the architectural foundation of the regulatory defensibility argument and must be preserved in every technical decision.

---

## 2. Percolator Protocol Assessment (Updated April 2026)

### 2.1 Protocol Maturity Update

Since the original research session, Percolator has evolved materially:

**v12.20 Mainnet Deployment (Being Sunset):** A mainnet deployment at program ID `BCGNFw6vDinWTF9AybAbi8vr69gx5nk5w8o2vEWgpsiw` was launched with all four market authorities and the program upgrade authority burned. A public bounty program invited developers to exploit the immutable binary. As of April 22, 2026, this deployment is being sunset in favor of the v12.21 wire format. The CLI on master is not backwards-compatible with v12.20.

**v12.21 Devnet Deployment (Active Target):** The current development target. Program ID `2SSnp35m7FQ7cRLNKGdW5UzjYFF6RBUNq7d3m5mqNByp` on devnet. This is the version Ballast will build against.

**percolator-match Repository:** A new standalone repo for the passive LP matcher program (50bps spread) has been published. This provides a reference implementation for custom matchers — directly relevant to Ballast's allowlist matcher requirement.

**CLI Maturity:** The percolator-cli repo has reached 159 commits with three contributors (including Anatoly Yakovenko and Claude). It now includes mainnet bounty cron scripts, comprehensive stress testing, oracle pen-testing, and a unified v3 matcher context layout. The CLI's TypeScript modules are substantially more battle-tested than at initial assessment.

### 2.2 Three-Repo Architecture (Unchanged)

The architecture described in the handoff document remains accurate:

**percolator** — Core risk engine (Rust crate). H (haircut ratio) for fair exits, A/K (lazy side indices) for fair overhang clearing, warmup period for oracle manipulation defense, deterministic three-phase market reset cycle, formal verification via Kani.

**percolator-prog** — Solana on-chain program. One market = one slab account. Vault token account controlled by PDA. Two trade paths: TradeNoCpi and TradeCpi. LP owner must sign every trade.

**percolator-cli** — CLI tooling (TypeScript). User/LP lifecycle management, trade execution, keeper cranks, stress testing, devnet integration tests.

### 2.3 Access Control Model (Confirmed and Strengthened)

The LP-signature-gating model identified in the original research is confirmed in the v12.21 CLI README with explicit security documentation. Key reinforcement:

The README now contains a dedicated "Matcher Interface — Security Requirements" section with explicit language: "CRITICAL: The matcher program MUST error if the LP PDA is not a signer. The percolator program signs the LP PDA via invoke_signed during CPI. If your matcher accepts unsigned calls, attackers can bypass LP authorization and steal funds."

This confirms the access control model is not an incidental property of the architecture — it is a deliberate, documented security mechanism. The addition of the percolator-match reference implementation with explicit LP PDA verification strengthens confidence in building a custom allowlist matcher.

### 2.4 Devnet Test Market (Current Configuration)

```
Program:        2SSnp35m7FQ7cRLNKGdW5UzjYFF6RBUNq7d3m5mqNByp
Matcher:        4HcGCsyjAqnFua5ccuXyt8KRRQzKFbGTJkVChpS7Yfzy
Slab:           A7wQtRT9DhFqYho8wTVqQCDc7kYPTUXGPATiyVbZKVFs
Mint:           So11111111111111111111111111111111111111112 (Wrapped SOL)
Vault:          63juJmvm1XHCHveWv9WdanxqJX6tD6DLFTZD7dvH12dc
Oracle:         99B2bTijsU6f1GCT73HmdR7HCFFjGMBcPZY6jZ96ynrR (Chainlink SOL/USD)
Type:           INVERTED (price = 1/SOL in USD terms)

Risk Parameters:
  Maintenance Margin: 5%
  Initial Margin:     10%
  Trading Fee:        10 bps (0.1%)
```

The existing devnet market includes two LP configurations: a passive matcher (LP 0, 50bps spread) and a vAMM matcher (LP 4, tighter spreads with impact pricing). Both are operational with funded collateral and an insurance fund of approximately 8.8 SOL.

---

## 3. Open Questions — Resolved

The handoff document identified fifteen open questions across technical, regulatory, and business domains. This section resolves the technical and business questions required for Phase 0 and Phase 1 execution. Regulatory questions are flagged as "deferred to post-POC" where appropriate.

### 3.1 Should Phase 0 use the existing devnet slab or deploy a new one?

**Decision: Deploy a fresh slab.**

Rationale: The existing devnet slab is a shared test market with pre-existing positions, two LP configurations, and ongoing activity from other developers. A Ballast-dedicated slab provides clean state for reproducible testing, eliminates interference from external actors, and allows configuring margin parameters specific to a treasury hedging use case. The cost of deployment (one `setup-devnet-market.ts` execution) is trivial. The existing slab remains useful as a reference and for initial familiarization.

### 3.2 What margin parameters make sense for each use case?

**Phase 0 (Treasury Hedging — SOL/USD):**

Initial Margin: 10% — Consistent with the existing devnet market and appropriate for SOL volatility. A treasury hedge is typically sized to offset existing long exposure, so the margin requirement should not be so high as to make hedging capital-inefficient, nor so low as to create liquidation risk from normal SOL price swings.

Maintenance Margin: 5% — The default Percolator parameter. SOL's historical daily volatility of approximately 3-6% means a 5% maintenance margin provides adequate buffer for a well-monitored position that can be topped up.

Trading Fee: 10 bps — The default. For a bilateral POC where we control both sides, this fee accrues to the insurance fund and has no material impact on test economics.

**Phase 1 (Freight Rate Hedging — FBX/WCI):**

Initial Margin: 15% — Higher than the SOL/USD market because freight rate indices can experience sharp moves (the Freightos Baltic Index saw 300%+ swings during 2020-2022). A higher initial margin reflects the lower liquidity and wider bid-ask spreads inherent in freight derivatives. This is a starting parameter; the POC should test its adequacy through scenario simulation.

Maintenance Margin: 7.5% — Proportionally higher than the SOL/USD market for the same volatility reasons. The keeper crank must be run frequently enough to prevent positions from blowing through maintenance margin between updates.

Trading Fee: 10 bps — Same as Phase 0. The fee is not economically significant for a bilateral POC.

### 3.3 How should the custom matcher's allowlist be stored?

**Decision: On-chain account with explicit allowlist, verified during CPI.**

Rationale: For a POC, an off-chain allowlist with signature verification is simpler but introduces an off-chain dependency and a class of attacks (replay, front-running the authorization check) that complicate the security argument. An on-chain allowlist stored in the matcher context account is deterministic, auditable, and verifiable by anyone inspecting the chain. The percolator-match reference implementation already demonstrates how to use the 320-byte matcher context account. With only two participants in the POC, the storage requirement is 64 bytes (two pubkeys), well within the context allocation. For production scaling beyond a handful of participants, a separate PDA-derived allowlist account with dynamic sizing would be appropriate, but that is out of scope for the POC.

### 3.4 What oracle update frequency is appropriate for freight rate indices?

**Decision: Every 15 minutes during market hours, with manual overrides for testing.**

Rationale: Freight rate indices update far less frequently than crypto prices. The Freightos Baltic Index (FBX) publishes daily. The Drewry World Container Index (WCI) publishes weekly. However, the Percolator risk engine requires oracle prices for keeper cranks, margin calculations, and liquidation checks. An overly stale oracle creates liquidation risk (positions become undercollateralized without the engine knowing) or conversely prevents legitimate trades (the engine refuses risk-increasing trades if the crank/sweep is too old).

For the POC, the admin-pushed oracle (`PushOraclePrice`) will be used with a 15-minute update cadence during active testing. This cadence allows observing funding rate accrual, position mark-to-market, and keeper crank behavior without the impractical requirement of second-by-second price feeds for an asset class that moves daily or weekly. The POC scripts will include a configurable oracle pusher that can simulate various update patterns: steady prices, sudden jumps, gradual drift, and gap scenarios.

The keeper crank's 200-slot freshness requirement (approximately 80 seconds) means the crank bot must run independently of the oracle update cadence. The crank will run on a 30-second interval, and oracle updates will be pushed at the 15-minute interval.

### 3.5 How should the LP signing service be implemented for the POC?

**Decision: Automated local signing service with file-based keypair, not manual signing.**

Rationale: Manual signing (human approving each trade) is impractical even for a POC — it breaks the keeper crank requirement, makes stress testing impossible, and doesn't validate the production architecture. The POC will use a lightweight Node.js service that holds the LP keypair in a file (standard Solana CLI keypair format), listens for trade requests, validates the requesting wallet against the on-chain allowlist, and co-signs the transaction. This is architecturally identical to the production flow (replace file-based key with HSM/MPC, replace local service with hosted service with KYB verification) but executable in a devnet environment.

For Phase 0 and Phase 1, the LP signing service can be a simple script that runs alongside the CLI, since both counterparties are controlled by the Ballast developer. The important thing is that the signing logic is structured as a separable module — not inline in the trade script — so it can be extracted and hardened for production without refactoring.

### 3.6 Who is the target counterparty for the initial POC?

**Decision: CargoBill controls both sides for Phase 0 and Phase 1 validation.**

Phase 0 validates technology mechanics, not counterparty relationships. CargoBill will create two wallets (Wallet A: "CargoBill Treasury" acting as the hedger, Wallet B: "Counterparty LP" taking the other side). Both are controlled by the Ballast developer. This allows full control over test scenarios, reproducible results, and rapid iteration.

For Phase 1, the second wallet represents "Counterparty Freight Forwarder." The bilateral nature of the trade is validated architecturally (LP-signature gating, allowlist enforcement, separate wallets) even though a single developer controls both sides. A real second-party test with an external freight forwarder is a Phase 2/production milestone, not a Phase 0/1 requirement.

### 3.7 How does Ballast relate to CargoBill's core product?

**Decision: Separate product line with future integration pathway.**

Ballast is a research and POC project exploring whether CargoBill should add derivatives/hedging to its financial OS. It is not integrated with the CargoBill production codebase. If the POC validates the technology and business case, the integration pathway is: Ballast's transaction construction modules become a service layer within CargoBill's Next.js application, exposed to users through the existing Squads multisig wallet interface. The hedging position would appear alongside payment history, yield positions, and credit facilities in the CargoBill dashboard. But this integration is Phase 2+ and should not influence Phase 0/1 architecture decisions.

---

## 4. Phase 0 — Treasury Hedging (SOL/USD)

### 4.1 Objective

Validate that Percolator's architecture supports a bilateral treasury hedge between two wallets on Solana devnet, with LP-signature-gated access control, correct margin mechanics, accurate PnL settlement, and functioning keeper infrastructure.

### 4.2 Success Criteria

Phase 0 is complete when ALL of the following are demonstrated and documented:

**SC-0.1 — Market Deployment:** A dedicated SOL/USD slab is deployed on devnet with Ballast-specific configuration (parameters per Section 3.2). The slab is operational and can be inspected via `dump-state.ts` and `dump-market.ts`.

**SC-0.2 — Participant Initialization:** Two wallets (CargoBill Hedger, Counterparty LP) have initialized user accounts on the slab, deposited collateral, and can have their state inspected.

**SC-0.3 — Hedge Position Opening:** CargoBill Hedger can open a short SOL-perp position (hedging long SOL treasury exposure) against the Counterparty LP. The transaction succeeds, correct margin is reserved, and the position appears in the slab state.

**SC-0.4 — PnL Accuracy:** After oracle price changes (pushed via admin oracle authority), PnL for both the hedger and LP are calculated correctly. Specifically: if SOL price drops 10%, the short hedger's unrealized PnL should be positive by approximately 10% of notional (adjusted for funding), and the LP's unrealized PnL should be negative by the same amount. Verify within 0.5% tolerance to account for funding accrual and fee effects.

**SC-0.5 — Funding Rate Mechanics:** Funding rates accrue over time via keeper crank. The funding rate direction is correct (longs pay shorts when mark > index, shorts pay longs when mark < index). Funding accrual is observable in account state after multiple crank cycles.

**SC-0.6 — Position Closure and Settlement:** CargoBill Hedger can close the position (trade in the opposite direction). Realized PnL is correct. Collateral can be withdrawn after position closure.

**SC-0.7 — Access Control Enforcement:** An unauthorized third wallet (Wallet C, not in the allowlist) can InitUser and DepositCollateral but CANNOT execute any trade. The trade attempt must fail at the program level (not application level). Document the exact error returned.

**SC-0.8 — Keeper Operation:** A keeper crank bot runs continuously during testing. The crank updates mark price, processes funding, and the market does not enter DrainOnly or ResetPending states during normal operation. Document keeper crank cadence, sweep cycle behavior, and any anomalies.

**SC-0.9 — Liquidation Behavior:** Simulate a scenario where the oracle price moves adversely enough to push one participant below maintenance margin. Verify that the keeper crank correctly flags the position for liquidation and processes it. Document the liquidation mechanics (haircut, insurance fund behavior).

**SC-0.10 — Stress Scenario:** Run the existing `stress-worst-case.ts` and `stress-haircut-system.ts` scripts against the Ballast slab. All invariants hold. Document results.

### 4.3 Technical Architecture — Phase 0

```
                      ┌─────────────────────────────────────────────┐
                      │           Ballast Phase 0 (Devnet)          │
                      │                                             │
                      │  ┌─────────────────────────────────────┐    │
                      │  │         SOL/USD Slab                │    │
                      │  │  (Dedicated Ballast Market)         │    │
                      │  │                                     │    │
                      │  │  ┌──────────┐    ┌──────────────┐   │    │
                      │  │  │ CargoBill│    │ Counterparty │   │    │
                      │  │  │ Hedger   │◄──►│ LP           │   │    │
                      │  │  │ (User)   │    │ (LP + Signer)│   │    │
                      │  │  └──────────┘    └──────┬───────┘   │    │
                      │  │                         │           │    │
                      │  │              ┌──────────▼─────────┐ │    │
                      │  │              │  Allowlist Matcher  │ │    │
                      │  │              │  (Custom Program)   │ │    │
                      │  │              └────────────────────┘ │    │
                      │  └──────────────────────┬──────────────┘    │
                      │                         │                   │
                      │  ┌──────────────────────▼──────────────┐    │
                      │  │        Percolator Program            │    │
                      │  │  (2SSnp35m7FQ7cRLN...unmodified)    │    │
                      │  └──────────────────────┬──────────────┘    │
                      │                         │                   │
                      │  ┌──────────────────────▼──────────────┐    │
                      │  │      Chainlink SOL/USD Oracle        │    │
                      │  │  (99B2bTijsU6f1GCT73HmdR7HCFFjGMB) │    │
                      │  │  OR Admin-Pushed Oracle              │    │
                      │  └─────────────────────────────────────┘    │
                      └─────────────────────────────────────────────┘

Supporting Infrastructure:
  ├── Keeper Crank Bot (30-second interval)
  ├── LP Signing Service (co-signs authorized trades)
  ├── Event Logger (captures all tx signatures + state snapshots)
  └── Test Harness (scenario runner for SC-0.1 through SC-0.10)
```

### 4.4 Slab Configuration Specification

```json
{
  "name": "ballast-treasury-sol-usd",
  "network": "devnet",
  "programId": "2SSnp35m7FQ7cRLNKGdW5UzjYFF6RBUNq7d3m5mqNByp",
  "mint": "So11111111111111111111111111111111111111112",
  "marketType": "INVERTED",
  "riskParameters": {
    "maintenanceMarginBps": 500,
    "initialMarginBps": 1000,
    "tradingFeeBps": 10
  },
  "fundingParameters": {
    "fundingHorizonSlots": 216000,
    "fundingKBps": 100,
    "fundingMaxPremiumBps": 500,
    "fundingMaxBpsPerSlot": 1
  },
  "oracle": {
    "primary": "chainlink",
    "oracleAddress": "99B2bTijsU6f1GCT73HmdR7HCFFjGMBcPZY6jZ96ynrR",
    "adminOracleEnabled": true,
    "adminOracleNote": "Enable admin oracle for controlled test scenarios; Chainlink is the default for normal operation"
  },
  "initialInsuranceFund": "5 SOL",
  "lpCollateral": "10 SOL",
  "userCollateral": "5 SOL"
}
```

### 4.5 Oracle Strategy — Phase 0

Phase 0 has two oracle modes that serve different purposes:

**Mode 1 — Chainlink Live Oracle (Default):** The devnet Chainlink SOL/USD oracle at `99B2bTijsU6f1GCT73HmdR7HCFFjGMBcPZY6jZ96ynrR` provides live SOL/USD prices on devnet. This is the default mode and validates that Ballast works with a production-grade oracle feed. Most testing (SC-0.2 through SC-0.6, SC-0.8) should use this mode.

**Mode 2 — Admin-Pushed Oracle (Controlled Scenarios):** For SC-0.4 (precise PnL verification), SC-0.9 (liquidation), and SC-0.10 (stress testing), the admin oracle authority is enabled to push precise prices. This allows deterministic scenario construction. After controlled testing, disable admin oracle authority (set to zero address) to revert to Chainlink.

The Percolator program auto-detects oracle type by checking the account owner. If `oracle_authority != 0` AND `authority_price_e6 != 0` AND the timestamp is recent, it uses the authority price; otherwise it falls back to Chainlink. This priority system means both modes can coexist on the same slab.

### 4.6 Inverted Market Semantics

The SOL/USD slab is INVERTED. This is critical to understand for correct position interpretation:

In an inverted market, the internal price is `1/SOL_in_USD`. Collateral is in SOL (wrapped SOL). Positions are denominated in the inverse unit.

**Going LONG in the inverted market = LONG USD = PROFIT IF SOL DROPS.** This is the hedging direction for CargoBill. If CargoBill holds SOL as treasury and wants to hedge against SOL price decline, CargoBill opens a LONG position in the inverted market.

**Going SHORT in the inverted market = SHORT USD = PROFIT IF SOL RISES.** The counterparty LP takes this position.

This is counterintuitive and is the single most common source of confusion in testing. Every test script must explicitly comment the economic direction (not just the trade direction) of each position.

### 4.7 Allowlist Matcher Specification

The custom allowlist matcher for Ballast extends the percolator-match reference implementation with an explicit wallet allowlist. The matcher context layout must conform to the v3 unified format (320 bytes total, with 64 bytes reserved for return data at the start of the account).

**Functional Requirements:**

FM-1: The matcher MUST reject any trade where the counterparty wallet is not in the allowlist.

FM-2: The matcher MUST verify that the LP PDA is a signer (standard Percolator security requirement).

FM-3: The matcher MUST verify that the LP PDA matches the stored PDA in the context.

FM-4: The allowlist is stored in the matcher context account. For the POC, the context stores up to 4 wallet pubkeys (128 bytes) in the reserved area of the v3 layout.

FM-5: The allowlist is initialized atomically with LP creation (single transaction containing context account creation, matcher init, and LP init — per the CLI README's security guidance on preventing race conditions).

FM-6: The matcher supports passive pricing mode (fixed spread around oracle price) for the POC. vAMM pricing is not required for Phase 0 or Phase 1.

**Context Layout Extension (Ballast):**

```
Offset  Size  Field                    Description
0       64    [reserved]               Matcher return data (per Percolator spec)
64      8     magic                    0x42414C4C41535400 ("BALLAST\0")
72      4     version                  1
76      1     kind                     0=Passive (Phase 0/1 only)
77      3     _pad0
80      32    lp_pda                   LP PDA for signature verification
112     4     trading_fee_bps          Fee on fills
116     4     base_spread_bps          Minimum spread
120     1     allowlist_count          Number of entries (0-4)
121     3     _pad1
124     32    allowlist_0              First allowed wallet pubkey
156     32    allowlist_1              Second allowed wallet pubkey
188     32    allowlist_2              Third allowed wallet pubkey
220     32    allowlist_3              Fourth allowed wallet pubkey
252     68    _reserved                Future use
```

### 4.8 Event Logging Specification

All Ballast operations must be logged for post-POC analysis and as the foundation for the Layer 4 (Reporting) architecture. For the POC, logging is file-based JSONL written to `~/.cache/ballast/events.jsonl`.

**Required fields per event:**

```json
{
  "timestamp": "ISO-8601",
  "event_type": "MARKET_DEPLOY | USER_INIT | DEPOSIT | WITHDRAW | TRADE | CRANK | LIQUIDATION | ORACLE_PUSH | STATE_SNAPSHOT",
  "tx_signature": "base58-encoded Solana signature",
  "slab": "slab pubkey",
  "actor": "wallet pubkey that initiated the action",
  "counterparty": "LP or user pubkey on the other side (for trades)",
  "details": {
    "trade_size": "i128 as string",
    "trade_direction": "LONG | SHORT",
    "economic_direction": "HEDGE_SOL_DECLINE | SPECULATE_SOL_RISE (human-readable)",
    "oracle_price_e6": "number",
    "execution_price_e6": "number",
    "margin_before": "number",
    "margin_after": "number",
    "pnl_realized": "number",
    "pnl_unrealized": "number",
    "funding_accrued": "number"
  },
  "state_hash": "SHA-256 of full slab state at time of event"
}
```

This schema is intentionally verbose for a POC. It captures everything needed to reconstruct the full audit trail if the POC is used to demonstrate regulatory compliance mechanics to counsel or regulators.

### 4.9 Phase 0 Execution Steps

The following steps are sequential. Each step has explicit entry and exit criteria.

**Step 0.1 — Environment Setup**

Entry: Developer has Solana CLI, Node.js 18+, pnpm installed.

Actions:
1. Fork `aeyakovenko/percolator-cli` into `ballast-percolator-cli` on GitHub.
2. Clone the fork locally.
3. Run `pnpm install && pnpm build`.
4. Verify the existing devnet market is accessible: `npx tsx scripts/dump-state.ts` should return market state without errors.
5. Create two Solana keypairs: `ballast-hedger.json` and `ballast-lp.json` in `~/.config/ballast/`.
6. Airdrop devnet SOL to both wallets: `solana airdrop 5 --url devnet` (repeat as needed; devnet airdrops may be rate-limited).
7. Create a `ballast-config.json` in the repo root with Ballast-specific configuration (slab address, wallet paths, oracle address — to be populated after slab deployment).
8. Create a `scripts/ballast/` directory for all Ballast-specific scripts.

Exit: Both wallets funded. CLI builds and runs. Existing devnet market accessible.

**Step 0.2 — Slab Deployment**

Entry: Step 0.1 complete.

Actions:
1. Create `scripts/ballast/setup-ballast-market.ts` based on `scripts/setup-devnet-market.ts` but with Ballast-specific parameters (Section 4.4).
2. Deploy the slab. Record the slab pubkey, vault pubkey, vault PDA.
3. Top up the insurance fund with 5 SOL.
4. Verify deployment: `npx tsx scripts/dump-market.ts --slab <slab>` returns correct configuration.
5. Update `ballast-config.json` with deployed addresses.

Exit: Slab deployed, configured, and verified. SC-0.1 satisfied.

**Step 0.3 — Participant Setup**

Entry: Step 0.2 complete.

Actions:
1. Initialize LP account using `ballast-lp.json`: `percolator-cli init-lp --slab <slab>`. Record LP index.
2. Initialize user account using `ballast-hedger.json`: `percolator-cli init-user --slab <slab>`. Record user index.
3. Wrap SOL for both wallets: `spl-token wrap <amount> --url devnet`.
4. Deposit collateral for LP: `percolator-cli deposit --slab <slab> --user-idx <lp-idx> --amount <lamports>`.
5. Deposit collateral for hedger: `percolator-cli deposit --slab <slab> --user-idx <hedger-idx> --amount <lamports>`.
6. Verify deposits: `npx tsx scripts/dump-state.ts --slab <slab>` shows correct collateral balances.

Exit: Both participants initialized with collateral. SC-0.2 satisfied.

**Step 0.4 — Allowlist Matcher Deployment**

Entry: Step 0.3 complete.

Actions:
1. Write the Ballast allowlist matcher program (Rust, based on percolator-match).
2. Build and deploy to devnet.
3. Create `scripts/ballast/setup-ballast-matcher.ts` that performs the atomic LP-creation + matcher-init transaction per the security guidance in the CLI README.
4. Execute the setup script. Record matcher program ID and matcher context pubkey.
5. Test unauthorized access: attempt a trade from a wallet NOT in the allowlist. Confirm failure with specific error.
6. Update `ballast-config.json`.

Exit: Matcher deployed with allowlist. Unauthorized trade rejected. SC-0.7 partially satisfied (full validation in Step 0.6).

**Step 0.5 — Keeper Infrastructure**

Entry: Step 0.4 complete.

Actions:
1. Create `scripts/ballast/ballast-crank-bot.ts` based on `scripts/crank-bot.ts` configured for the Ballast slab.
2. Start the crank bot with a 30-second interval.
3. Let it run for 10 minutes and verify: no errors, crank step advances through the 16-step cycle, mark price updates correctly.
4. Create `scripts/ballast/ballast-event-logger.ts` that subscribes to the slab account via WebSocket and logs state changes to `~/.cache/ballast/events.jsonl`.

Exit: Keeper bot running continuously. Event logger capturing state changes. SC-0.8 partially satisfied.

**Step 0.6 — Trade Execution and Validation**

Entry: Steps 0.4 and 0.5 complete. Keeper bot running.

Actions:
1. Record oracle price before trade.
2. Execute hedge trade: CargoBill Hedger opens a LONG position (in the inverted market = short SOL economically) via `trade-cpi` against the Ballast LP.
3. Verify margin reserved correctly for both sides.
4. Let the position run for at least 30 minutes with the keeper crank active and Chainlink oracle providing live prices.
5. Record oracle price after 30 minutes.
6. Calculate expected PnL based on price change.
7. Compare expected PnL to actual PnL from slab state. Document any discrepancy.
8. Close the position (reverse trade).
9. Verify realized PnL is correct.
10. Withdraw collateral from both wallets.
11. Verify final balances.

For SC-0.7, also execute the unauthorized trade attempt with Wallet C (created but NOT in the allowlist):
12. Init user for Wallet C on the Ballast slab.
13. Deposit collateral from Wallet C.
14. Attempt trade-cpi from Wallet C against the Ballast LP.
15. Confirm failure. Document error.
16. Withdraw collateral from Wallet C (should succeed).

Exit: SC-0.3, SC-0.4, SC-0.5, SC-0.6, SC-0.7 satisfied.

**Step 0.7 — Controlled Scenarios**

Entry: Step 0.6 complete.

Actions:
1. Enable admin oracle authority on the slab.
2. Open a position.
3. Push a series of predetermined prices to validate precise PnL calculations (SC-0.4 refinement):
   - Push price +5%, verify PnL.
   - Push price -10% from original, verify PnL.
   - Push price +20% from original, verify PnL.
4. Test liquidation (SC-0.9):
   - Open a position with minimal excess margin.
   - Push oracle price adversely until the position breaches maintenance margin.
   - Run keeper crank. Verify liquidation is processed.
   - Document insurance fund behavior (did it absorb the loss? how much?).
5. Disable admin oracle authority (revert to Chainlink).

Exit: SC-0.4, SC-0.9 satisfied.

**Step 0.8 — Stress Testing**

Entry: Step 0.7 complete.

Actions:
1. Run `npx tsx scripts/stress-worst-case.ts` against the Ballast slab. Document results.
2. Run `npx tsx scripts/stress-haircut-system.ts` against the Ballast slab. Document results.
3. Run `npx tsx scripts/oracle-authority-stress.ts` against the Ballast slab. Document results.
4. Run `npx tsx scripts/pentest-oracle.ts` against the Ballast slab. Document results.
5. Compile a stress test report documenting all invariant checks, any failures, and any parameters that needed adjustment.

Exit: SC-0.10 satisfied. Phase 0 complete.

### 4.10 Phase 0 Deliverables

D-0.1: `ballast-percolator-cli` repository (fork) with all Ballast-specific scripts in `scripts/ballast/`.

D-0.2: Ballast allowlist matcher program (Rust source, deployed to devnet).

D-0.3: Phase 0 Validation Report — a markdown document in the repo covering every success criterion (SC-0.1 through SC-0.10) with evidence (transaction signatures, state snapshots, calculated vs. actual PnL comparison, stress test results).

D-0.4: `ballast-config.json` with all deployed addresses and configuration.

D-0.5: Event log (`events.jsonl`) from the full testing session.

---

## 5. Phase 1 — Freight Rate Hedging

### 5.1 Objective

Validate that Percolator supports a bespoke perpetual derivative on a non-crypto underlying (freight rate index) between two freight forwarding companies, with admin-pushed oracle prices, bilateral trade execution, and funding rate mechanics that are economically meaningful for freight rate hedging.

### 5.2 Prerequisites

Phase 0 must be complete. Specifically:
- The allowlist matcher is deployed and validated (reusable for Phase 1).
- The keeper crank bot is operational and tested.
- The event logging infrastructure is in place.
- PnL calculation mechanics are understood and validated.
- The LP signing service pattern is established.

### 5.3 Success Criteria

**SC-1.1 — Freight Rate Slab Deployment:** A separate slab for the freight rate index market is deployed on devnet with Phase 1 parameters (Section 3.2). The slab uses admin-pushed oracle prices. The slab is operational and inspectable.

**SC-1.2 — Oracle Price Pushing:** An automated oracle pusher script correctly pushes freight rate prices at configurable intervals. Prices conform to Percolator's `price_e6` format. The pushed prices are reflected in the slab state and used for margin calculations.

**SC-1.3 — Freight Rate Price Format Validation:** The freight rate index value (e.g., FBX = $1,847/FEU) is correctly transformed into Percolator's internal price format without precision loss or overflow. Document the transformation formula and verify with at least 5 historical price points.

**SC-1.4 — Bilateral Freight Hedge:** Two wallets execute a freight rate derivative trade. Wallet A (hedging freight rate INCREASES — e.g., a shipper locked into future shipping costs) opens one direction. Wallet B (hedging freight rate DECREASES — e.g., a carrier hedging revenue) opens the other direction. Both positions are opened, maintained, and closed correctly.

**SC-1.5 — Funding Rate Economic Validity:** The funding rate mechanism, designed for continuous alignment of mark-to-index in crypto perpetuals, behaves in a way that is economically coherent for a freight rate derivative. Specifically: the funding rate should not be so aggressive as to dominate the P&L for a low-volatility freight index, nor should it be so weak as to allow mark price to diverge materially from the pushed index price. Document the funding rate behavior over a multi-day simulation. If the default funding parameters are inappropriate for freight rates, document what adjustments are needed and why.

**SC-1.6 — Multi-Day Simulation:** Run a simulated 30-day freight rate scenario using historical FBX or WCI data. Push prices at the defined cadence (Section 3.4). Verify that positions track the index correctly, margin requirements are maintained, and the overall P&L at close matches the index movement within tolerance.

**SC-1.7 — Scenario: Rate Spike:** Simulate a sudden 50% freight rate increase (comparable to events during 2020-2021 supply chain crisis). Verify that margin mechanics prevent insolvency, liquidation triggers correctly if the adverse party is undercollateralized, and the insurance fund absorbs any shortfall.

**SC-1.8 — Scenario: Stale Oracle:** Stop pushing oracle prices for an extended period (e.g., 2 hours simulating a weekend gap). Verify that the market does not enter an unsafe state, that risk-increasing trades are correctly blocked when the crank/sweep is stale, and that normal operation resumes when prices are pushed again.

### 5.4 Technical Architecture — Phase 1

```
                      ┌─────────────────────────────────────────────┐
                      │           Ballast Phase 1 (Devnet)          │
                      │                                             │
                      │  ┌─────────────────────────────────────┐    │
                      │  │      Freight Rate Index Slab         │    │
                      │  │  (Dedicated Bilateral Market)        │    │
                      │  │                                     │    │
                      │  │  ┌──────────┐    ┌──────────────┐   │    │
                      │  │  │ Shipper  │    │ Carrier /    │   │    │
                      │  │  │ (Hedger) │◄──►│ Forwarder LP │   │    │
                      │  │  │ (User)   │    │ (LP + Signer)│   │    │
                      │  │  └──────────┘    └──────┬───────┘   │    │
                      │  │                         │           │    │
                      │  │              ┌──────────▼─────────┐ │    │
                      │  │              │  Allowlist Matcher  │ │    │
                      │  │              │  (Same program,     │ │    │
                      │  │              │   new context)      │ │    │
                      │  │              └────────────────────┘ │    │
                      │  └──────────────────────┬──────────────┘    │
                      │                         │                   │
                      │  ┌──────────────────────▼──────────────┐    │
                      │  │        Percolator Program            │    │
                      │  │  (Same program, no changes)          │    │
                      │  └──────────────────────┬──────────────┘    │
                      │                         │                   │
                      │  ┌──────────────────────▼──────────────┐    │
                      │  │     Admin-Pushed Freight Oracle       │    │
                      │  │  (ballast-freight-oracle.ts)         │    │
                      │  │  Source: FBX/WCI/bilateral-agreed    │    │
                      │  │  Cadence: 15 min (configurable)      │    │
                      │  └─────────────────────────────────────┘    │
                      └─────────────────────────────────────────────┘

Supporting Infrastructure (shared with Phase 0):
  ├── Keeper Crank Bot (30-second interval, configured for freight slab)
  ├── LP Signing Service (same module, new wallet + allowlist)
  ├── Event Logger (additional slab subscription)
  ├── Freight Oracle Pusher (new — reads CSV or API, pushes prices)
  └── Simulation Harness (new — replays historical freight rate data)
```

### 5.5 Slab Configuration Specification — Freight Rate Market

```json
{
  "name": "ballast-freight-fbx-global",
  "network": "devnet",
  "programId": "2SSnp35m7FQ7cRLNKGdW5UzjYFF6RBUNq7d3m5mqNByp",
  "mint": "So11111111111111111111111111111111111111112",
  "marketType": "NORMAL",
  "riskParameters": {
    "maintenanceMarginBps": 750,
    "initialMarginBps": 1500,
    "tradingFeeBps": 10
  },
  "fundingParameters": {
    "fundingHorizonSlots": 432000,
    "fundingKBps": 50,
    "fundingMaxPremiumBps": 200,
    "fundingMaxBpsPerSlot": 1,
    "note": "Lower funding aggressiveness than SOL/USD due to lower volatility and update cadence of freight indices"
  },
  "oracle": {
    "primary": "admin",
    "adminOracleEnabled": true,
    "oracleAuthority": "<ballast-oracle-authority-pubkey>",
    "updateCadence": "15 minutes",
    "dataSource": "Freightos Baltic Index (FBX) Global Container Freight Index",
    "fallback": "none — admin oracle is the only source for freight rates"
  },
  "initialInsuranceFund": "5 SOL",
  "lpCollateral": "10 SOL",
  "userCollateral": "5 SOL"
}
```

**Market Type: NORMAL (not INVERTED).** Unlike the SOL/USD slab, the freight rate slab uses NORMAL market type. The freight rate index is a dollar-denominated value (e.g., FBX = $1,847/FEU). There is no inversion needed. Collateral is in wrapped SOL (denominated in SOL terms), and positions are denominated in the freight rate index value. This means a long position profits when freight rates increase; a short position profits when freight rates decrease.

### 5.6 Oracle Design — Freight Rate

This is the most novel and technically sensitive aspect of Phase 1. There is no decentralized oracle for freight rate indices. The oracle trust model is fundamentally different from crypto oracles.

**Price Source Selection:**

Primary: Freightos Baltic Index (FBX) — the most widely cited container freight rate index, covering 12 major trade lanes with a composite global index. Published daily (with intraday updates available via API for subscribers). FBX is published by the Baltic Exchange, the same institution behind the Baltic Dry Index used in traditional freight derivatives.

Alternative: Drewry World Container Index (WCI) — weekly publication, covers 8 major trade routes. Less granular but more established in the freight forwarding community.

For the POC, historical FBX data will be used for simulation. The oracle pusher script will read from a CSV file of historical prices and push them at the configured cadence to simulate real-time operation.

**Price Format Transformation:**

Percolator's `PushOraclePrice` command accepts a price in USD terms (e.g., `143.50` for $143.50). The `price_e6` internal format stores this as an integer with 6 decimal places of precision (143500000 for $143.50).

The FBX Global Composite typically ranges from $1,000 to $10,000 per FEU (forty-foot equivalent unit), with historical extremes from $1,200 (Q3 2019) to $11,000+ (Q3 2021).

Transformation: `price_e6 = FBX_value * 1_000_000`. For FBX = $1,847, `price_e6 = 1_847_000_000`. This is within Percolator's u64 range and does not cause precision loss. The oracle pusher script must document this transformation explicitly and validate against historical extremes.

**Bilateral Agreement on Reference Rate:**

For production, both counterparties must agree on the reference rate source, update schedule, and dispute resolution mechanism BEFORE opening positions. For the POC, this agreement is implicit (CargoBill controls both sides). However, the POC documentation should include a template "Bilateral Oracle Agreement" that specifies: the data source (FBX Global Composite), the update cadence (daily for FBX, with 15-minute interpolation for Percolator), the handling of data gaps (use last published price), and the dispute mechanism (neither party can push prices; only the designated oracle authority can).

**Oracle Authority Key Management:**

The oracle authority keypair is a separate keypair from the admin, LP, or user keypairs. It is stored at `~/.config/ballast/ballast-oracle-authority.json`. The oracle pusher script is the only process that uses this keypair. This separation of concerns means: the oracle authority cannot trade, the LP cannot push prices, and the admin cannot push prices (unless the admin and oracle authority are the same key, which they should NOT be for the POC).

### 5.7 Funding Rate Analysis for Freight Rates

Percolator's funding rate mechanism is designed for crypto perpetuals where mark price should track a continuous spot market. Freight rate indices are fundamentally different: they update daily or weekly, move in trends (not random walks), and have seasonal patterns.

**Concern:** If the funding rate is too aggressive, the holder of a winning position will have their profits eroded by funding payments before the freight rate index actually moves. If the funding rate is too passive, mark price will diverge from the index, creating basis risk that undermines the hedge.

**POC Approach:** Phase 1 will use reduced funding parameters (Section 5.5 — lower `fundingKBps`, lower `fundingMaxPremiumBps`, longer `fundingHorizonSlots`) and document the funding rate behavior over the 30-day simulation (SC-1.5). The simulation report must include: cumulative funding paid/received per side, funding as a percentage of gross P&L, maximum mark-to-index divergence, and a qualitative assessment of whether the funding mechanism is fit for purpose.

If the funding mechanism is found to be fundamentally inappropriate for freight rates (e.g., it consistently erodes more than 20% of hedge P&L), this is a critical finding that would require either Percolator fork modifications or a different settlement architecture for freight derivatives. This outcome is itself a valuable result of the POC.

### 5.8 Phase 1 Execution Steps

**Step 1.1 — Historical Data Preparation**

Entry: Phase 0 complete.

Actions:
1. Obtain 12 months of historical FBX Global Composite daily data.
2. Format into a CSV: `date,fbx_value_usd` with one row per business day.
3. Create `scripts/ballast/ballast-freight-oracle.ts` — a script that reads the CSV and pushes prices at configurable intervals (default: 15 minutes simulated from daily data, meaning one day of real FBX data maps to 15 minutes of devnet time for compressed simulation).
4. Validate the price transformation for 10+ data points covering the full FBX range.

Exit: Oracle pusher script operational. Price format validated. SC-1.3 satisfied.

**Step 1.2 — Freight Rate Slab Deployment**

Entry: Step 1.1 complete.

Actions:
1. Create `scripts/ballast/setup-ballast-freight-market.ts` with Phase 1 parameters.
2. Deploy the slab. Record addresses.
3. Set oracle authority to the dedicated oracle authority keypair.
4. Deploy a new allowlist matcher context for the freight slab (reusing the same matcher program from Phase 0, but with a new context account pointing to the freight slab's LP PDA and containing the freight-specific allowlist).
5. Initialize LP and user accounts on the freight slab.
6. Deposit collateral to both.
7. Verify: `dump-market.ts` shows correct configuration, `dump-state.ts` shows correct balances.

Exit: SC-1.1 satisfied.

**Step 1.3 — Oracle Integration Test**

Entry: Step 1.2 complete. Keeper crank bot running against freight slab.

Actions:
1. Start the freight oracle pusher.
2. Push 10 sequential prices.
3. After each push, run keeper crank and verify the mark price updates correctly.
4. Verify that the slab state reflects the pushed price.
5. Verify that margin calculations use the pushed price.

Exit: SC-1.2 satisfied.

**Step 1.4 — Bilateral Freight Trade**

Entry: Step 1.3 complete.

Actions:
1. Record initial oracle price (e.g., FBX = $1,847).
2. Wallet A (Shipper) opens a LONG position (profits if freight rates increase).
3. Wallet B (Carrier LP) is on the other side (profits if freight rates decrease).
4. Verify margin reserved correctly.
5. Push 5 sequential prices (simulating freight rate movement over several days).
6. After each price push, run crank and verify PnL for both sides.
7. Close the position.
8. Verify final settlement.

Exit: SC-1.4 satisfied.

**Step 1.5 — 30-Day Simulation**

Entry: Step 1.4 complete.

Actions:
1. Create `scripts/ballast/ballast-freight-simulation.ts` — an automated script that opens a position, pushes 30 days of historical FBX data at the configured cadence, runs crank after each push, snapshots state, and closes the position at the end.
2. Run the simulation.
3. Produce a simulation report covering: daily PnL for both sides, cumulative funding paid/received, margin utilization over time, maximum drawdown, final PnL vs. index change, funding as a percentage of gross PnL.
4. Run the simulation twice with different historical periods (one trending up, one trending down) to validate both directions.

Exit: SC-1.5, SC-1.6 satisfied.

**Step 1.6 — Stress Scenarios**

Entry: Step 1.5 complete.

Actions:
1. Rate spike scenario (SC-1.7): Open a position, then push a 50% price increase in a single oracle update. Verify margin mechanics, liquidation behavior, insurance fund impact.
2. Stale oracle scenario (SC-1.8): Open a position, stop pushing prices for 2 hours (simulated). Verify market state remains safe, risk-increasing trades are blocked, recovery when prices resume.
3. Run `stress-worst-case.ts` and `stress-haircut-system.ts` against the freight slab.
4. Document all results.

Exit: SC-1.7, SC-1.8 satisfied. Phase 1 complete.

### 5.9 Phase 1 Deliverables

D-1.1: All Phase 1 scripts in `scripts/ballast/` within the `ballast-percolator-cli` repo.

D-1.2: Freight oracle pusher module (`ballast-freight-oracle.ts`), reusable for production with a real-time data source.

D-1.3: Historical FBX data CSV used for simulation.

D-1.4: Phase 1 Validation Report — covering every success criterion (SC-1.1 through SC-1.8) with evidence.

D-1.5: 30-Day Simulation Report — detailed analysis of funding rate behavior, PnL tracking accuracy, margin utilization, and fitness-for-purpose assessment.

D-1.6: Freight Rate Oracle Design Document — specifying the bilateral oracle agreement template, price format transformation, update cadence rationale, and key management model.

D-1.7: Updated `ballast-config.json` with freight slab addresses.

D-1.8: Complete event log (`events.jsonl`) for all Phase 1 testing.

---

## 6. Risk Register

### 6.1 Technical Risks

**TR-1: Percolator v12.21 devnet instability.** The protocol is experimental and unaudited. Devnet deployments may be updated or reset by Yakovenko without notice.

Mitigation: Pin the exact program commit and program ID in `ballast-config.json`. If the devnet program is updated and breaks compatibility, Ballast can deploy its own instance of the program from the pinned source. The `percolator-prog` repo contains build instructions for reproducible deployment.

Likelihood: Medium. Impact: Medium. Residual risk: Low (with pinned commit).

**TR-2: Keeper crank reliability.** If the crank bot crashes or hangs, the market enters a stale state where risk-increasing trades are blocked, mark prices don't update, and funding doesn't accrue. This is not a safety issue (the market degrades gracefully) but blocks testing.

Mitigation: Implement health checking in the crank bot (log heartbeats, alert on missed cranks). For the POC, a simple process monitor (e.g., `pm2` or a shell loop with restart) is sufficient.

Likelihood: Medium. Impact: Low (testing delay only). Residual risk: Low.

**TR-3: Devnet SOL supply.** Devnet airdrops are rate-limited and occasionally unavailable. Collateral and transaction fees consume devnet SOL.

Mitigation: Airdrop aggressively during initial setup (accumulate 50+ SOL across wallets). Use minimal collateral amounts for testing. Keep a reserve wallet. If devnet airdrops fail persistently, consider using a local validator for isolated testing (at the cost of losing Chainlink oracle access).

Likelihood: Medium. Impact: Low. Residual risk: Low.

**TR-4: Allowlist matcher security vulnerability.** The custom matcher is new code interacting with a protocol-level security mechanism. A bug in the matcher could allow unauthorized trades or, worse, lock funds.

Mitigation: The matcher is a small program (estimated <200 lines of Rust). Extensive review before deployment. Test the specific attack vector: unauthorized wallet attempting to trade through the matcher. Test fund recovery: can the LP and user withdraw collateral if the matcher has a bug? (Answer: yes — `WithdrawCollateral` does not go through the matcher.)

Likelihood: Low. Impact: High. Residual risk: Medium (acceptable for a devnet POC).

**TR-5: Freight oracle manipulation.** The admin-pushed oracle for Phase 1 is a single point of trust. Whoever holds the oracle authority keypair can push any price, potentially creating artificial PnL.

Mitigation: For the POC, this is accepted as a known limitation. The oracle authority keypair is separate from all other keypairs. The event log records every price push with timestamp and value. For production, the oracle trust model would require a multi-sig oracle committee or an on-chain verifiable data source (e.g., a freight rate oracle backed by signed data from the Baltic Exchange).

Likelihood: N/A (single-operator POC). Impact: High (in production). Residual risk: Accepted for POC.

### 6.2 Regulatory Risks

**RR-1: Platform classification as a trading facility.** If Ballast is later classified as providing multi-participant order matching or price discovery, it could require DCM/SEF registration.

Mitigation: Architecture is strictly bilateral. One slab per counterparty pair. No order book. No price discovery. LP-signature gating prevents unauthorized participation. Document this explicitly in all deliverables.

Likelihood: Low (for bilateral POC). Impact: High (in production). Residual risk: Requires legal opinion before production.

**RR-2: ECP qualification uncertainty.** CargoBill's ECP status under Path 2 ($1M+ net worth, commercial risk) is stronger for the freight rate hedge than the treasury hedge. A treasury hedge on SOL held as operational capital has a reasonable "in connection with business conduct" argument, but it is less clear-cut than hedging freight rates directly tied to the core business.

Mitigation: Document the commercial risk nexus for both use cases explicitly in the validation reports. For treasury hedging: SOL is held as operational treasury for a business that processes payments on Solana; hedging SOL price risk is managing an asset owned in the conduct of business. For freight rate hedging: freight rate volatility directly impacts the margins of CargoBill's customers; CargoBill facilitates payments tied to freight shipments and is exposed to rate-driven payment volume changes.

Likelihood: Medium. Impact: High (in production). Residual risk: Requires legal opinion before production.

### 6.3 Business Risks

**BR-1: Percolator project abandonment.** Percolator is an experimental project by one developer (with Claude AI). If Yakovenko moves on, the protocol has no organizational support structure.

Mitigation: Apache-2.0 license means CargoBill can maintain its own fork indefinitely. The protocol is formally verified via Kani, reducing the maintenance burden. The code is well-documented (159 commits, comprehensive README). The risk is acceptable for a POC; for production, an audit and a maintained fork would be necessary regardless.

Likelihood: Medium. Impact: Medium. Residual risk: Low (with fork strategy).

**BR-2: No real counterparty interest.** The POC validates technology, not market demand. Freight forwarders may have no interest in on-chain derivatives regardless of technical feasibility.

Mitigation: Phase 0 and Phase 1 are designed to be low-cost (engineering time only, no capital at risk). The POC produces artifacts (validation reports, simulation data) that can be used in customer discovery conversations without requiring customers to use the technology. Azim's network provides access to freight forwarders for demand validation in parallel with technical POC execution.

Likelihood: Medium. Impact: Medium. Residual risk: Accepted.

---

## 7. Dependency Matrix

| Dependency | Phase | Status | Risk | Fallback |
|---|---|---|---|---|
| Solana devnet | 0, 1 | Available | Low (occasional instability) | Local validator |
| Percolator program (devnet, v12.21) | 0, 1 | Deployed | Medium (may be updated) | Pin commit, deploy own instance |
| percolator-cli (master) | 0, 1 | 159 commits, stable | Low | Pin to specific commit |
| Chainlink SOL/USD oracle (devnet) | 0 | Available | Low | Admin-pushed oracle |
| Rust toolchain (for matcher) | 0 | Available | None | — |
| Node.js 18+ / pnpm | 0, 1 | Available | None | — |
| Solana CLI / SPL token CLI | 0, 1 | Available | None | — |
| Historical FBX data (CSV) | 1 | Requires procurement | Medium (may be paywalled) | Use synthetic data or Drewry WCI |
| Solana web3.js / SPL token library | 0, 1 | Available (via CLI) | None | — |

---

## 8. Non-Functional Requirements

### 8.1 Security

NFR-SEC-1: No private keys are stored in the repository. All keypairs use file paths in configuration. `.gitignore` must exclude all `.json` keypair files.

NFR-SEC-2: The allowlist matcher must be reviewed by at least one additional engineer before devnet deployment.

NFR-SEC-3: All event logs must be tamper-evident. Each event includes a `state_hash` (SHA-256 of slab state) that can be independently verified against on-chain data.

NFR-SEC-4: Oracle authority, LP, admin, and user keypairs must be separate keypairs with separate file paths. No keypair reuse across roles.

### 8.2 Observability

NFR-OBS-1: The keeper crank bot must log every crank attempt (success or failure) with timestamp, slot number, and any errors.

NFR-OBS-2: The event logger must capture all state-changing transactions within 10 seconds of confirmation.

NFR-OBS-3: A market dashboard script (`scripts/ballast/ballast-dashboard.ts`) must be available to display current positions, margin utilization, funding rate, oracle price, and insurance fund balance in a human-readable format.

### 8.3 Reproducibility

NFR-REP-1: All test scenarios must be scriptable and reproducible. No manual CLI commands for validation steps.

NFR-REP-2: The 30-day freight simulation must be deterministic given the same input CSV and starting state.

NFR-REP-3: All deployed program IDs, slab addresses, and configuration parameters are recorded in `ballast-config.json` and committed to the repository.

### 8.4 Documentation

NFR-DOC-1: Every Ballast-specific script must have a header comment explaining what it does, what prerequisites it requires, and what it validates.

NFR-DOC-2: The repo README must include setup instructions sufficient for a new developer to go from zero to running Phase 0 validation in under 2 hours.

NFR-DOC-3: The validation reports must be written for a dual audience: technical (engineers verifying correctness) and non-technical (counsel and investors understanding the regulatory architecture).

---

## 9. Timeline Estimate

These estimates assume one full-time developer (the CTO) working with Claude Code.

| Milestone | Estimated Duration | Cumulative |
|---|---|---|
| Phase 0: Environment setup + slab deployment (Steps 0.1-0.2) | 1-2 days | Day 2 |
| Phase 0: Participant setup + allowlist matcher (Steps 0.3-0.4) | 3-5 days | Day 7 |
| Phase 0: Keeper + trade execution + validation (Steps 0.5-0.6) | 2-3 days | Day 10 |
| Phase 0: Controlled scenarios + stress testing (Steps 0.7-0.8) | 2-3 days | Day 13 |
| Phase 0: Validation report writing | 1 day | Day 14 |
| Phase 1: Data preparation + slab deployment (Steps 1.1-1.2) | 2-3 days | Day 17 |
| Phase 1: Oracle integration + bilateral trade (Steps 1.3-1.4) | 2-3 days | Day 20 |
| Phase 1: 30-day simulation + stress scenarios (Steps 1.5-1.6) | 3-4 days | Day 24 |
| Phase 1: Validation report + simulation report writing | 2 days | Day 26 |

**Total estimated duration: 4-5 weeks.**

The dominant risk to this timeline is the allowlist matcher development (Step 0.4). If the matcher takes longer than expected (debugging Rust BPF, handling edge cases in the Percolator CPI), the entire timeline shifts. The mitigation is to start with `TradeNoCpi` (which only requires LP signature, not matcher CPI) for initial validation and add the matcher CPI path as a parallel workstream.

---

## 10. Glossary

| Term | Definition |
|---|---|
| Slab | A single Percolator market account containing header, config, and the risk engine state. One slab = one market. |
| LP | Liquidity Provider. In Percolator, the LP is a special account type that takes the other side of user trades and must co-sign every trade. |
| Matcher | An external Solana program called via CPI during `TradeCpi` that determines trade pricing and can reject trades. |
| Keeper Crank | A permissionless transaction that updates the market's mark price, accrues funding, and processes liquidations. Must be run regularly for the market to function. |
| Haircut Ratio (H) | Percolator's global ratio determining how much of accumulated profits are actually withdrawable. Prevents insolvency by treating profits as junior claims on the vault. |
| Inverted Market | A market where the internal price is 1/spot. Used for SOL/USD where collateral is SOL and users want USD exposure. |
| ECP | Eligible Contract Participant. A qualified entity under the Commodity Exchange Act that can enter bilateral swaps off-exchange. |
| FBX | Freightos Baltic Index. A container freight rate index published by the Baltic Exchange. |
| WCI | Drewry World Container Index. An alternative container freight rate index. |
| DCM | Designated Contract Market. A CFTC-registered exchange. Ballast must NOT function as one. |
| SEF | Swap Execution Facility. A CFTC-registered platform for swap trading. Ballast must NOT function as one. |
| SDR | Swap Data Repository. Where swap data is reported under CFTC Parts 43/45. Required for production, not POC. |
| PDA | Program Derived Address. A Solana address derived deterministically from a program ID and seeds, controlled by the program. |

---

## 11. Decision Log

| # | Decision | Date | Rationale | Reversibility |
|---|---|---|---|---|
| 1 | Deploy fresh slabs (not reuse existing devnet market) | Apr 2026 | Clean state, custom parameters, no interference | Easy — can always fall back to existing market |
| 2 | On-chain allowlist in matcher context (not off-chain) | Apr 2026 | Deterministic, auditable, no off-chain dependency | Medium — would require new matcher deployment |
| 3 | 15-minute oracle cadence for freight rates | Apr 2026 | Balances Percolator's freshness requirements with freight index publication cadence | Easy — configurable parameter |
| 4 | Automated LP signing service (not manual) | Apr 2026 | Required for keeper crank, stress testing, and production-like architecture | Easy — can always add manual approval |
| 5 | CargoBill controls both sides for POC | Apr 2026 | Fastest path to validation, full scenario control | Easy — add real counterparty in Phase 2 |
| 6 | Separate product line (not integrated with CargoBill app) | Apr 2026 | De-risks POC from production codebase, allows independent iteration | Medium — integration is Phase 2+ work |
| 7 | NORMAL market type for freight (INVERTED for SOL/USD) | Apr 2026 | Freight rates are naturally USD-denominated, no inversion needed | Hard — market type is set at slab creation |
| 8 | Reduced funding parameters for freight slab | Apr 2026 | Freight indices are less volatile than crypto; aggressive funding would erode hedge PnL | Easy — configurable via update-config |
| 9 | Pin Percolator program to v12.21 commit | Apr 2026 | Prevents breaking changes from upstream updates | Easy — can update pin later |

---

## 12. Post-POC Path (Preview)

This section is not in scope for Phase 0/1 execution but documents the known production requirements to ensure POC architecture decisions do not create unnecessary rework.

**Mandatory before any real funds:**

1. Security audit of Percolator risk engine and on-chain program.
2. Security audit of the Ballast allowlist matcher.
3. Legal opinion from derivatives counsel on the full structure (bilateral ECP framework, LP-signature gating as access control, platform classification).
4. Off-chain master agreement template (ISDA-like bilateral derivative agreement).
5. KYB and ECP verification integration (Footprint for KYB, manual or automated asset verification for ECP status).
6. SDR reporting integration (CFTC Parts 43 and 45).
7. Mainnet deployment with dedicated RPC, monitoring, alerting, and on-call.
8. Decision on whether to pursue CFTC Innovation Task Force engagement or formal no-action letter.

**Phase 2 scope (Next.js application):**

The `ballast-percolator-cli` TypeScript modules (transaction construction, account derivation, slab parsing, oracle interaction) become the SDK for a Next.js application. The app provides: Solana Wallet Adapter integration (Phantom, etc.), position dashboard with PnL display, trade interface for opening/closing hedges, oracle status display, margin and liquidation warnings, and integration with CargoBill's existing auth (Privy) and wallet (Squads) infrastructure.

---

*This document is confidential and proprietary to CargoBill Inc. 2025-2026 CargoBill Inc. All rights reserved.*
