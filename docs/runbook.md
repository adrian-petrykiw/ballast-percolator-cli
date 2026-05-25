# Ballast Operational Runbook

> Devnet POC. The procedures here are deliberately simple but written now so the muscle memory carries forward. The first time you reach for a runbook should never be in production.

## When to use this

Open this document when something is misbehaving on-chain, in a deployed program, or in the keeper/oracle relay loop and you need to decide an action under time pressure. For routine dev questions consult [`CLAUDE.md`](../CLAUDE.md) and [`docs/prd.md`](prd.md) instead.

## Standard incident response loop

For every incident — devnet or otherwise — follow the same six steps in order:

1. **STOP.** Halt any automated process touching the affected slab, oracle, matcher, or wallet (cranker, oracle relay, integration test runner). Resist the urge to "just try one more thing."
2. **ASSESS.** Snapshot current state immediately, before any recovery action:
   ```bash
   npx tsx scripts/ballast/dump-state.ts > docs/reports/incident-$(date -u +%Y%m%dT%H%M%SZ).json
   ```
   If the dump script doesn't exist for this market type yet, capture the relevant `solana account` outputs manually.
3. **CONTAIN.** Take the smallest reversible action that prevents further harm (pause the cranker, switch RPC, revoke an allowlist entry). Never reach for a destructive action when a soft one exists.
4. **RECOVER.** Execute the per-incident procedure below.
5. **RECORD.** Append a JSON line to `~/.cache/ballast/events.jsonl` with `event_type: "INCIDENT"`, including timestamp, incident class, slab pubkey, actions taken, and the path to the state snapshot from step 2.
6. **POSTMORTEM.** Within 24 hours, write a short report at `docs/reports/incident-<date>.md`: timeline, root cause, what worked, what didn't, what we'd do differently. Even on devnet. Even if "nothing serious happened." This is how the runbook itself improves.

## Incident index

1. [Oracle gone stale](#1-oracle-gone-stale)
2. [Cranker died or lag > 30s](#2-cranker-died-or-lag--30s)
3. [Bad market parameter deployed](#3-bad-market-parameter-deployed)
4. [Matcher allowlist mistake](#4-matcher-allowlist-mistake)
5. [Upgrade authority key lost or compromised](#5-upgrade-authority-key-lost-or-compromised)
6. [Keypair role mix-up](#6-keypair-role-mix-up)
7. [RPC endpoint flapping](#7-rpc-endpoint-flapping)
8. [Wallet out of devnet SOL](#8-wallet-out-of-devnet-sol)
9. [USDC faucet rate-limited](#9-usdc-faucet-rate-limited)

---

### 1. Oracle gone stale

**Symptom.** Engine ops (`TopUpInsurance`, `Init*`, `Trade*`, `KeeperCrank`) revert with `OracleStale (0x6)`. Frontend reads show no recent mark-price movement.

**Quick check.**
```bash
solana account <oracle-pubkey> --url devnet --output json | jq '.account.lamports, .account.data'
# Compare last slot/timestamp in the oracle account against current slot:
solana slot --url devnet
```

**Decision tree.**
- **Pyth feed stale** (devnet Pyth normally pushes ~1×/min): wait 60 s, recheck. If still stale, Pyth devnet itself is having problems — switch to fallback or pause.
- **Admin-pushed oracle (FX relay)**:
  - Relay process still running? `ps aux | grep ballast-oracle-relay`. If dead, restart: `npx tsx scripts/ballast/ballast-oracle-relay.ts`.
  - Push authority key still in place? `solana account <oracle-pubkey>` → check authority field against [`config/ballast-config.json`](../config/ballast-config.example.json).
  - If both up and still stale, push a manual price using the oracle authority keypair.
- **Push authority key lost.** STOP all trading on the affected slab. Escalate. Recovery requires rotating the authority via the slab's `SetConfig` instruction.

**Recovery.** Once the oracle is fresh, prepend a `KeeperCrank` instruction to the next engine-touching call (see memory note `percolator-oracle-stale-gate.md` for the rationale).

**Postmortem note.** Log the staleness duration in seconds and the recovery path taken. A staleness > 5 min on any slab is a candidate for adding a watchdog alert.

---

### 2. Cranker died or lag > 30s

**Symptom.** Cranker process is not in `ps`, or the most recent `CRANK` event in `~/.cache/ballast/events.jsonl` is > 30 s old. Liquidations not firing on positions that should be liquidated.

**Quick check.**
```bash
ps aux | grep ballast-crank-bot
tail -n 20 ~/.cache/ballast/events.jsonl | grep CRANK
solana slot --url devnet      # compare slot delta against last CRANK
```

**Decision tree.**
- **Process dead.** Restart: `npx tsx scripts/ballast/ballast-crank-bot.ts`. Verify by watching for new `CRANK` events in `events.jsonl`.
- **Lag 30–40 s** (within `MAX_ACCRUAL_DT_SLOTS = 100`, ~40 s): catch up quickly. Cranker should self-correct; if not, restart it.
- **Lag > 40 s.** `TopUpInsurance` / `Trade*` ops now revert with `OracleStale 0x6` (the engine sees too many slots between accruals). All user-facing trades blocked until catch-up. Restart cranker; first crank may need to be sent manually with a higher compute budget.

**Recovery.** Confirm with at least 5 consecutive `CRANK` events at < 30 s intervals before declaring incident resolved.

**Postmortem note.** Capture *why* the cranker died — OOM, RPC timeout, unhandled exception, signal. The fix isn't "restart"; it's the underlying cause.

---

### 3. Bad market parameter deployed

**Symptom.** New slab init succeeded but values are wrong: oracle pubkey, fee bps, leverage limits, `max_price_move_bps_per_slot`, anti-spam fees, matcher pubkey, collateral mint.

**Quick check.**
```bash
npx tsx scripts/ballast/dump-market.ts <slab-pubkey> > /tmp/market-actual.json
diff <(jq -S . /tmp/market-actual.json) <(jq -S . docs/reports/market-expected-<id>.json)
```

**Decision tree.**
- **Parameter is mutable via `SetConfig`.** Build the `SetConfig` instruction, simulate first (`--simulate` flag), then emit a fenced bash block for the user to run.
- **Parameter is immutable post-init** (oracle pubkey, matcher pubkey, collateral mint, certain risk params). The slab must be retired. Procedure:
  1. STOP cranker for this slab.
  2. Document all open positions in a snapshot.
  3. Communicate to any affected counterparties (LP, hedger).
  4. Deploy a corrected slab to a new pubkey.
  5. Update [`config/ballast-config.json`](../config/ballast-config.example.json) to point at the new slab.
  6. Decide whether to leave the bad slab as a zombie or to drain it (close all accounts, then leave the slab as inert).

**Recovery.** New slab must pass the same pre-deploy checklist (bottom of this file) before being declared production-ready for the next phase.

**Postmortem note.** Add the wrong-parameter case to the deploy checklist so the same class of mistake can't recur.

---

### 4. Matcher allowlist mistake

**Symptom.** A trader is rejected when they should be allowed, or — much worse — an unauthorized counterparty unexpectedly succeeds at trading.

**Quick check.**
```bash
solana account <matcher-allowlist-pda> --url devnet --output json | jq '.account.data'
# Compare against the intended set in config/ballast-config.json
```

**Decision tree.**
- **Authorized pubkey missing from allowlist.** Build the matcher `AddAllowlist` (or equivalent) instruction. Submit. Verify by reading the account again.
- **Unauthorized pubkey present.**
  1. Build the `RemoveAllowlist` instruction. Submit immediately.
  2. Audit recent trades by that pubkey: `solana transaction-history <pubkey>` and cross-reference against the slab's TradeCpi events.
  3. If any trades executed: capture the trade details, evaluate whether they need to be unwound or accepted-as-is, and record the decision.

**Recovery.** Confirm the allowlist matches the intended set exactly before resuming normal operations.

**Postmortem note.** How did the wrong pubkey get added? Manual error, copy-paste from the wrong file, automation bug? The fix lives in whatever produced the wrong set, not in the matcher.

---

### 5. Upgrade authority key lost or compromised

**Symptom.**
- *Lost:* `solana program upgrade` fails with signer errors; you can no longer modify the deployed matcher binary.
- *Compromised:* `solana program show <program-id>` reports an upgrade you did not authorize, or you observe behavior changes in the matcher's logic.

**Quick check.**
```bash
solana program show <program-id> --url devnet
# Verify upgrade-authority pubkey against config/ballast-config.json
```

**Decision tree.**
- **Lost authority (immutable now).** Program can no longer be upgraded on devnet. Deploy a new copy of the program at a fresh program ID, update [`config/ballast-config.json`](../config/ballast-config.example.json), and migrate any slabs that referenced the old program ID. Old program remains inert.
- **Compromised, you still hold a copy of the key.** Immediately rotate: `solana program set-upgrade-authority <program-id> --new-upgrade-authority <new-pubkey>` (run by the user, not Claude — this command is hook-blocked). Then treat as "lost" for the old key — the old key is burnt.
- **Compromised, key not recoverable.** Assume any user of the program is at risk. Pause every slab that references the program. Deploy a clean copy at a new program ID with a fresh upgrade authority. Communicate broadly.

**Recovery.** New upgrade authority is stored separately from any other Ballast role keypair, with provenance documented in the postmortem.

**Postmortem note.** Where did the key live? Was it on disk? In a multisig? In a hardware wallet? The answer drives the rotation policy for every other key.

---

### 6. Keypair role mix-up

**Symptom.** A transaction is signed by an unexpected key, or a transaction unexpectedly fails with a signer-authorization error. Examples: hedger keypair attempts to sign as LP, oracle authority key is used to push to a different oracle account, admin keypair is used in a user-only flow.

**Quick check.**
- Get the pubkey from the failing transaction (logs or RPC response).
- Cross-reference against [CLAUDE.md → Wallet Architecture](../CLAUDE.md).

**Decision tree.**
- **Wrong key signed a reversible action** (a trade about to clear, a pending update): cancel/revert before it lands.
- **Wrong key signed an irreversible action**: capture the tx signature and full context, plan the corrective action separately. Do not try to "fix" by signing more transactions with the wrong key — that compounds the audit problem.
- **Detected via failed signer auth in a script**: fix the script's keypair path. Verify by re-running. Do NOT commit the fix without verifying no real key exposure happened.

**Recovery.** Rotate any keypair that was used out-of-role, even if the action it took was benign. Role separation is the property; once violated, the key is no longer trustworthy in any role.

**Postmortem note.** What in the script/config selected the wrong key? Hardcoded path, env-var fallback, missing assertion? Add the assertion.

---

### 7. RPC endpoint flapping

**Symptom.** Intermittent RPC errors — `Failed to fetch`, HTTP 429, HTTP 503, occasional `BlockhashNotFound`. Affects cranker, oracle relay, and integration tests.

**Quick check.**
```bash
solana cluster-version --url devnet
# If that hangs or fails, the endpoint is the problem, not your code.
```

**Decision tree.**
- **Rate-limited (HTTP 429).** Back off. Reduce poll frequency in the cranker / oracle relay. Consider an alternate devnet RPC (e.g., a different public endpoint listed in Solana docs).
- **Endpoint down.** Switch via the `--rpc` CLI flag or update `config/ballast-config.json`. Restart processes that hold a long-lived connection.
- **Sustained issues across multiple endpoints.** Pause non-critical scripts entirely; prioritize cranker continuity so liquidations can still fire when the endpoint recovers.

**Recovery.** After 10 consecutive successful operations on the new endpoint, resume normal cadence.

**Postmortem note.** Track which endpoint flapped and how long. If a particular endpoint flaps repeatedly, document it as "do not use for production cranker."

---

### 8. Wallet out of devnet SOL

**Symptom.** Transactions fail with `Transaction simulation failed: Attempt to debit an account but found no record of a prior credit` or balance-too-low errors.

**Quick check.**
```bash
solana balance --url devnet --keypair ~/.config/ballast/<role>.json
```

**Decision tree.**
- **Below 0.1 SOL.** Top up: `solana airdrop 2 --url devnet --keypair ~/.config/ballast/<role>.json`. If the faucet is cold (~6h cooldown), drain from another funded role wallet via a one-off `solana transfer` (this is hook-blocked for Claude — emit the bash block for the user).
- **Below 0.01 SOL.** Cranker / oracle relay will start failing soon. Top up *all* role wallets pre-emptively before they hit zero.

**Recovery.** Maintain a "fuel" wallet (separate keypair, not in the role list) funded to ~5 SOL so role wallets can be topped up without waiting on the faucet.

**Postmortem note.** Devnet SOL accumulation is cheap. Aim to keep every role wallet above 1 SOL at all times. If a wallet hit zero, the monitoring gap is the bug.

---

### 9. USDC faucet rate-limited

**Symptom.** The Circle devnet USDC faucet returns "rate limited" or "wait N minutes" for the requesting wallet. Affects USDC-collateralized FX market work.

**Quick check.** Browser request to the Circle devnet USDC faucet returns the rate-limit message.

**Decision tree.**
- **Within the 2h cooldown window for that wallet.** Either wait, or route the USDC through an already-funded wallet via a `spl-token transfer` (hook-blocked; emit bash block).
- **All your wallets are simultaneously rate-limited.** Stagger faucet requests across wallets so they don't all enter cooldown together.

**Recovery.** Maintain a "USDC reserve" wallet (separate keypair) funded to ~100 USDC so role wallets can be topped up without waiting on the faucet.

**Postmortem note.** If you're hitting the rate limit weekly, restructure the integration tests to use less faucet bandwidth (mint a custom test USDC, or amortize fixture funding across runs).

---

## Pre-deploy checklist

Run before *every* `solana program deploy` or `solana program upgrade`. Both commands are hook-blocked for Claude — the user runs them.

- [ ] Working tree is clean: `git status` shows no uncommitted changes
- [ ] Build is reproducible: `cargo clean && cargo build-sbf` succeeds on a fresh checkout
- [ ] Previous `.so` archived: `cp target/deploy/<name>.so programs/<name>/archive/<commit>.so` (so we can roll back without rebuilding from history)
- [ ] Upgrade authority confirmed: `solana program show <program-id> --url devnet` matches expected key in `config/ballast-config.json`
- [ ] Pre-deploy state snapshot saved: `npx tsx scripts/ballast/dump-state.ts > docs/reports/pre-deploy-$(date -u +%Y%m%dT%H%M%SZ).json`
- [ ] Rollback plan written: which `.so` to redeploy, who holds the upgrade authority, who needs to be notified if rollback happens mid-day
- [ ] Cranker paused if upgrading the matcher (prevents mid-deploy trades from hitting a half-updated binary)
- [ ] Post-deploy verification plan written: which scripts/tests to run to confirm the deploy succeeded

## Postmortem template

Each incident postmortem (`docs/reports/incident-<date>.md`) should answer, in order:

1. **What happened?** One paragraph, no jargon.
2. **Timeline** with timestamps from `~/.cache/ballast/events.jsonl`.
3. **Root cause.** Be specific. "RPC was slow" is not a root cause; "we polled every 200ms and triggered rate-limiting on a 100-req/s endpoint" is.
4. **What worked.** What part of the response was effective?
5. **What didn't.** Where did we improvise or guess?
6. **Action items.** Specific, owned, dated. Update this runbook if any procedure changed.

---

*This runbook will be wrong in places by the time you read it. Update it.*
