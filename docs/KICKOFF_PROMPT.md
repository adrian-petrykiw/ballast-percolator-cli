# Ballast — Claude Code Kickoff Prompt

Copy everything below the line and paste it as your first message in the Claude Code chat after opening the repo in VS Code.

---

I'm building **Ballast** — a proof-of-concept for compliant on-chain bilateral derivatives (FX and freight rate hedging) built on Anatoly Yakovenko's Percolator perpetual futures protocol on Solana devnet.

## Context

This repo is a fork of `aeyakovenko/percolator-cli`. The upstream code is already here — the CLI builds and runs, and I can interact with Percolator's existing devnet deployment. **Do not modify any upstream files** in `scripts/`, `src/`, `test/`, or `tests/` — all Ballast-specific work goes in new directories.

The full PRD is at `docs/prd.md` — read it carefully before doing anything. The `CLAUDE.md` at the repo root has all project conventions, directory structure, security rules, and command reference.

I have the following devnet assets ready:
- ~10 SOL (airdropable for more)
- 20 USDC (Circle devnet faucet, can get more every 2 hours)
- 20 EURC (Circle devnet faucet)
- Two Solana wallets configured for devnet

## What I Need You To Do — Phase 0 Setup

Before writing any implementation code, I need you to scaffold the project structure and prepare for Phase 0 execution. Do this in a single PR on a `feat/phase-0-scaffold` branch.

### Step 1: Verify the repo state

1. Run `pnpm install && pnpm build` to confirm the upstream CLI compiles
2. Run `npx tsx scripts/dump-state.ts` to confirm devnet connectivity and that the existing Percolator devnet market is accessible (slab `A7wQtRT9DhFqYho8wTVqQCDc7kYPTUXGPATiyVbZKVFs`)
3. Report what you find — especially the program ID, oracle type, and current market state

### Step 2: Create the Ballast directory structure

Create the following directories and placeholder files:

```
scripts/ballast/           — Ballast-specific scripts (will be populated per PRD steps)
tests/ballast/             — Ballast-specific tests
tests/ballast/integration/ — Devnet integration tests
programs/                  — Parent for on-chain programs
programs/ballast-matcher/  — Custom allowlist matcher (Rust project)
config/                    — Ballast configuration
docs/reports/              — Validation reports
```

Create `config/ballast-config.example.json` with this structure (all values as placeholders):

```json
{
  "network": "devnet",
  "rpcUrl": "https://api.devnet.solana.com",
  "percolatorProgramId": "2SSnp35m7FQ7cRLNKGdW5UzjYFF6RBUNq7d3m5mqNByp",
  "matcherProgramId": "",
  "wallets": {
    "hedger": "~/.config/ballast/ballast-hedger.json",
    "lp": "~/.config/ballast/ballast-lp.json",
    "oracleAuthority": "~/.config/ballast/ballast-oracle-authority.json"
  },
  "slabs": {
    "solUsd": {
      "slab": "",
      "vault": "",
      "vaultPda": "",
      "oracle": "",
      "oracleType": "pyth",
      "marketType": "INVERTED",
      "collateralMint": "So11111111111111111111111111111111111111112",
      "lpIndex": null,
      "hedgerIndex": null,
      "matcherCtx": ""
    },
    "eurUsd": {
      "slab": "",
      "vault": "",
      "vaultPda": "",
      "oracle": "",
      "oracleType": "admin",
      "oracleSource": "pyth-hermes-relay",
      "pythFeedId": "",
      "marketType": "NORMAL",
      "collateralMint": "",
      "lpIndex": null,
      "hedgerIndex": null,
      "matcherCtx": ""
    }
  },
  "pyth": {
    "hermesUrl": "https://hermes.pyth.network",
    "feeds": {
      "SOL/USD": "",
      "EUR/USD": "",
      "INR/USD": "",
      "GBP/USD": "",
      "USDC/USD": "",
      "USDT/USD": ""
    }
  },
  "keeper": {
    "crankIntervalMs": 30000,
    "oracleRelayIntervalMs": 900000
  }
}
```

### Step 3: Investigate Pyth on Solana devnet

This is critical research before we start building. I need you to:

1. Check what Pyth program is deployed on Solana devnet — find the program ID
2. Determine if the Pyth SOL/USD push feed account exists on devnet and find its address
3. Look at how the upstream `percolator-prog` handles oracle detection — check the `src/` directory for any oracle parsing logic that indicates which Pyth account format it expects (legacy vs PriceUpdateV2 vs price feed accounts)
4. Check the upstream `percolator-cli` source to see how it passes oracle accounts in `keeper-crank` and `trade-cpi` commands — does it do any Pyth-specific preprocessing?
5. Try running `percolator-cli keeper-crank` against the existing devnet market to see what oracle format it currently uses (Chainlink) and what the oracle account structure looks like

Report your findings. I need to know:
- Can we use Pyth instead of Chainlink for the SOL/USD slab?
- If yes, what's the exact Pyth SOL/USD devnet account address to pass as `--oracle`?
- If no, what's the incompatibility and can we work around it?

### Step 4: Update .gitignore and verify security

1. Verify the `.gitignore` covers all sensitive files (keypairs, env files, etc.)
2. Verify no keypair files exist in the repo
3. Verify the `.claude/settings.json` is properly configured

### Step 5: Commit and create PR

After completing steps 1-4:
1. Create a branch `feat/phase-0-scaffold`
2. Commit all scaffolding files: `chore(ballast): scaffold project structure and config [PRD Phase 0 Setup]`
3. Commit the Pyth investigation findings as a doc: `docs(ballast): Pyth oracle compatibility analysis`
4. Use the `/create-pr` command to create the PR

## Important Constraints

- **pnpm only** — not npm or yarn
- **Devnet only** — never reference mainnet RPC URLs or deploy to mainnet
- **No keypair access** — never read, display, or log private key content
- **No upstream modifications** — all new code in `scripts/ballast/`, `tests/ballast/`, `programs/`, `config/`, `docs/`
- **Read the CLAUDE.md** before starting — it has all conventions and security rules
- **Read the PRD** (`docs/prd.md`) before making architectural decisions — it contains resolved design questions

Start by reading CLAUDE.md and docs/prd.md, then proceed with Step 1.
