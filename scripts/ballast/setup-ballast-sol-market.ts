/**
 * Ballast Phase 0 — SOL/USD slab deployment (CLAUDE.md "Phase 0 Step 0.0",
 * PRD §4.9 Step 0.2; satisfies SC-0.1 once executed).
 *
 * Forks the structure of upstream `scripts/setup-devnet-market.ts` but:
 *   - swaps Chainlink for Pyth Pull (sponsored devnet SOL/USD push feed,
 *     shard 0; see docs/pyth-oracle-compatibility.md)
 *   - emits Ballast-specific risk parameters from PRD §3.2 + §4.4
 *     (mm 5%, im 10%, fee 10bps, INVERTED market type)
 *   - reads/writes `config/ballast-config.json` (the runtime config —
 *     gitignored; derived from `config/ballast-config.example.json`)
 *   - is idempotent: refuses to redeploy when the slab pubkey is already
 *     populated unless `--force` is passed; insurance top-up may be
 *     re-run with `--insurance-only`
 *   - leaves LP creation to the matcher PR (PRD Step 0.4) — the Ballast
 *     allowlist matcher is bound at LP-init time, so initing an LP here
 *     against the upstream passive matcher would just create churn
 *
 * Prerequisites:
 *   - pnpm install && pnpm build (or `npx tsx` for direct execution)
 *   - keypairs at the paths in `config/ballast-config.json` (default:
 *     `~/.config/ballast/ballast-{hedger,lp,oracle-authority}.json`)
 *   - LP wallet funded with ~12 SOL (slab rent ~5.3 SOL + insurance 5 SOL +
 *     ATA + tx fees); request more via `solana airdrop` if needed
 *
 * Usage:
 *   npx tsx scripts/ballast/setup-ballast-sol-market.ts
 *   npx tsx scripts/ballast/setup-ballast-sol-market.ts --simulate
 *   npx tsx scripts/ballast/setup-ballast-sol-market.ts --insurance-only
 *   npx tsx scripts/ballast/setup-ballast-sol-market.ts --force
 *
 * Verify after:
 *   npx tsx scripts/dump-market.ts --slab <slab>
 */

import {
  Connection, Keypair, PublicKey, Transaction, sendAndConfirmTransaction,
  ComputeBudgetProgram, SystemProgram, LAMPORTS_PER_SOL,
} from "@solana/web3.js";
import {
  getOrCreateAssociatedTokenAccount, NATIVE_MINT, TOKEN_PROGRAM_ID,
} from "@solana/spl-token";
import * as fs from "fs";
import * as path from "path";
import * as os from "os";
import {
  encodeInitMarket, encodeKeeperCrank, encodeTopUpInsurance,
} from "../../src/abi/instructions.js";
import {
  ACCOUNTS_INIT_MARKET, ACCOUNTS_KEEPER_CRANK, ACCOUNTS_TOPUP_INSURANCE,
  buildAccountMetas, WELL_KNOWN,
} from "../../src/abi/accounts.js";
import { deriveVaultAuthority } from "../../src/solana/pda.js";
import { fetchSlab, parseConfig, parseEngine, parseHeader, SLAB_LEN } from "../../src/solana/slab.js";
import { buildIx } from "../../src/runtime/tx.js";
import { verifyPythPriceAccount } from "./utils/pyth.js";

// ─── Constants ────────────────────────────────────────────────────────────

const CONFIG_PATH = path.resolve(process.cwd(), "config/ballast-config.json");

// PRD §4.4 — initial insurance fund.
const INSURANCE_FUND_LAMPORTS = 5n * BigInt(LAMPORTS_PER_SOL);

// Wrap headroom: insurance 5 SOL + ATA rent + tx fees + crank fees ≈ 5.3 SOL.
const WRAP_LAMPORTS = 6n * BigInt(LAMPORTS_PER_SOL);

// Per docs/pyth-oracle-compatibility.md §7: floor of 120 s (2× heartbeat).
const PYTH_MAX_STALENESS_SECS = "120";
// 2 % — matches upstream test defaults (`defaultInitMarketArgs`).
const PYTH_CONF_FILTER_BPS = 200;

// ─── Config I/O ───────────────────────────────────────────────────────────

interface SlabConfig {
  slab: string;
  vault: string;
  vaultPda: string;
  oracle: string;
  oracleType: string;
  marketType: string;
  collateralMint: string;
  lpIndex: number | null;
  hedgerIndex: number | null;
  matcherCtx: string;
}

interface BallastConfig {
  network: string;
  rpcUrl: string;
  percolatorProgramId: string;
  matcherProgramId: string;
  wallets: { hedger: string; lp: string; oracleAuthority: string };
  slabs: { solUsd: SlabConfig; eurUsd: SlabConfig };
  pyth: {
    hermesUrl: string;
    receiverProgramId: string;
    feeds: Record<string, string>;
    sponsoredFeedAccountsDevnet: Record<string, string>;
  };
  keeper: { crankIntervalMs: number; oracleRelayIntervalMs: number };
}

function loadConfig(): BallastConfig {
  if (!fs.existsSync(CONFIG_PATH)) {
    throw new Error(
      `${CONFIG_PATH} not found. Copy config/ballast-config.example.json first.`,
    );
  }
  return JSON.parse(fs.readFileSync(CONFIG_PATH, "utf-8"));
}

function saveConfig(cfg: BallastConfig): void {
  fs.writeFileSync(CONFIG_PATH, JSON.stringify(cfg, null, 2) + "\n");
}

function expandTilde(p: string): string {
  return p.startsWith("~") ? path.join(os.homedir(), p.slice(1)) : p;
}

function loadKeypair(p: string): Keypair {
  const expanded = expandTilde(p);
  if (!fs.existsSync(expanded)) {
    throw new Error(
      `Keypair not found at ${expanded}. Generate with:\n  solana-keygen new --outfile ${expanded} --no-bip39-passphrase`,
    );
  }
  return Keypair.fromSecretKey(new Uint8Array(JSON.parse(fs.readFileSync(expanded, "utf-8"))));
}

// ─── CLI ──────────────────────────────────────────────────────────────────

function hasFlag(name: string): boolean {
  return process.argv.includes(name);
}

// ─── Steps ────────────────────────────────────────────────────────────────

async function ensureWrappedSol(
  conn: Connection,
  payer: Keypair,
  amount: bigint,
): Promise<PublicKey> {
  const ata = await getOrCreateAssociatedTokenAccount(conn, payer, NATIVE_MINT, payer.publicKey);
  const balanceInfo = await conn.getTokenAccountBalance(ata.address);
  const have = BigInt(balanceInfo.value.amount);
  if (have >= amount) {
    console.log(`    wSOL ata ${ata.address.toBase58()} already holds ${Number(have) / LAMPORTS_PER_SOL} SOL — skipping wrap`);
    return ata.address;
  }
  const need = amount - have;
  const tx = new Transaction()
    .add(ComputeBudgetProgram.setComputeUnitLimit({ units: 30_000 }))
    .add(SystemProgram.transfer({
      fromPubkey: payer.publicKey, toPubkey: ata.address, lamports: Number(need),
    }))
    .add({
      programId: TOKEN_PROGRAM_ID,
      keys: [{ pubkey: ata.address, isSigner: false, isWritable: true }],
      data: Buffer.from([17]), // SyncNative
    });
  await sendAndConfirmTransaction(conn, tx, [payer], { commitment: "confirmed" });
  console.log(`    wrapped ${Number(need) / LAMPORTS_PER_SOL} SOL → ${ata.address.toBase58()}`);
  return ata.address;
}

async function deploySlab(
  conn: Connection,
  payer: Keypair,
  programId: PublicKey,
  oracleAccount: PublicKey,
  feedIdHex: string,
): Promise<{ slab: PublicKey; vaultPda: PublicKey; vaultAta: PublicKey }> {
  // ── slab account ──
  const slab = Keypair.generate();
  const rent = await conn.getMinimumBalanceForRentExemption(SLAB_LEN);
  console.log(`\n[2] Creating slab account ${slab.publicKey.toBase58()}`);
  console.log(`    size: ${SLAB_LEN} bytes  rent: ${(rent / LAMPORTS_PER_SOL).toFixed(4)} SOL`);
  await sendAndConfirmTransaction(
    conn,
    new Transaction()
      .add(ComputeBudgetProgram.setComputeUnitLimit({ units: 50_000 }))
      .add(SystemProgram.createAccount({
        fromPubkey: payer.publicKey, newAccountPubkey: slab.publicKey,
        lamports: rent, space: SLAB_LEN, programId,
      })),
    [payer, slab],
    { commitment: "confirmed" },
  );

  // ── vault PDA + ATA ──
  const [vaultPda] = deriveVaultAuthority(programId, slab.publicKey);
  const vaultAta = await getOrCreateAssociatedTokenAccount(
    conn, payer, NATIVE_MINT, vaultPda, true,
  );
  console.log(`    vault pda:  ${vaultPda.toBase58()}`);
  console.log(`    vault ata:  ${vaultAta.address.toBase58()}`);

  // ── InitMarket (Pyth Pull, inverted SOL/USD) ──
  console.log(`\n[3] InitMarket (Pyth Pull, inverted SOL/USD, SOL collateral)`);
  const initArgs = ballastSolUsdInitArgs(payer.publicKey, feedIdHex);
  const sig = await sendAndConfirmTransaction(
    conn,
    new Transaction()
      .add(ComputeBudgetProgram.setComputeUnitLimit({ units: 400_000 }))
      .add(buildIx({
        programId,
        keys: buildAccountMetas(ACCOUNTS_INIT_MARKET, [
          payer.publicKey, slab.publicKey, NATIVE_MINT, vaultAta.address,
          WELL_KNOWN.clock, oracleAccount,
        ]),
        data: encodeInitMarket(initArgs),
      })),
    [payer],
    { commitment: "confirmed" },
  );
  console.log(`    sig: ${sig}`);

  // ── warm-up keeper crank (mark price seed) ──
  console.log(`\n[4] Initial permissionless KeeperCrank`);
  const crankSig = await sendAndConfirmTransaction(
    conn,
    new Transaction()
      .add(ComputeBudgetProgram.setComputeUnitLimit({ units: 400_000 }))
      .add(buildIx({
        programId,
        keys: buildAccountMetas(ACCOUNTS_KEEPER_CRANK, [
          payer.publicKey, slab.publicKey, WELL_KNOWN.clock, oracleAccount,
        ]),
        data: encodeKeeperCrank({ callerIdx: 65535, candidates: [] }),
      })),
    [payer],
    { commitment: "confirmed", skipPreflight: true },
  );
  console.log(`    sig: ${crankSig}`);

  return { slab: slab.publicKey, vaultPda, vaultAta: vaultAta.address };
}

async function topUpInsurance(
  conn: Connection,
  payer: Keypair,
  programId: PublicKey,
  slab: PublicKey,
  vaultAta: PublicKey,
  payerAta: PublicKey,
  amount: bigint,
): Promise<string> {
  const sig = await sendAndConfirmTransaction(
    conn,
    new Transaction()
      .add(ComputeBudgetProgram.setComputeUnitLimit({ units: 60_000 }))
      .add(buildIx({
        programId,
        keys: buildAccountMetas(ACCOUNTS_TOPUP_INSURANCE, [
          payer.publicKey, slab, payerAta, vaultAta,
          WELL_KNOWN.tokenProgram, WELL_KNOWN.clock,
        ]),
        data: encodeTopUpInsurance({ amount: amount.toString() }),
      })),
    [payer],
    { commitment: "confirmed" },
  );
  return sig;
}

// ─── InitMarket arg builder ───────────────────────────────────────────────

/**
 * Ballast SOL/USD init-market args (PRD §3.2, §4.4).
 *
 * Risk: mm 5%, im 10%, fee 10bps, INVERTED.
 * Oracle: Pyth Pull, sponsored devnet feed (60 s heartbeat / 0.5 % deviation).
 * Collateral: wSOL (9 dec, unit_scale=0 → 1 lamport = 1 engine unit).
 *
 * Authorities are deliberately NOT burned here — Phase 0 needs admin and
 * insurance authority live to push controlled-scenario prices (PRD §4.5
 * Mode 2) and to top up insurance during scenario testing. Burns happen
 * post-validation, before any production-style bilateral hand-off.
 */
function ballastSolUsdInitArgs(
  admin: PublicKey,
  pythFeedIdHex: string,
) {
  return {
    admin,
    collateralMint: NATIVE_MINT,
    indexFeedId: pythFeedIdHex,
    maxStalenessSecs: PYTH_MAX_STALENESS_SECS,
    confFilterBps: PYTH_CONF_FILTER_BPS,
    invert: 1,            // SOL collateral, USD-denominated index → invert
    unitScale: 0,         // 1 lamport = 1 engine unit
    initialMarkPriceE6: "0", // ignored for non-Hyperp; program reads Pyth at init
    // Maintenance fee (~$5/day at SOL=$170, slot=400ms): 250 lamports/slot
    // × 216_000 slots/day = 54M lamports = 0.054 SOL/day. Anti-spam
    // invariant satisfied (nonzero alongside newAccountFee).
    maintenanceFeePerSlot: "250",

    // RiskParams — PRD §3.2
    hMin:                  "10",      // ~4 s warmup floor
    maintenanceMarginBps:  "500",     // 5 %
    initialMarginBps:      "1000",    // 10 % → 10× max leverage
    tradingFeeBps:         "10",      // 10 bps
    maxAccounts:           "64",      // bilateral POC; well under 4096 cap
    // Anti-spam: $10-equivalent at SOL=$170 ≈ 0.058 SOL → round to 0.06.
    newAccountFee:         "60000000",
    hMax:                  "100",     // ~40 s warmup ceiling (= perm-resolve)
    maxCrankStalenessSlots:"0",       // v12.21 read+discard
    liquidationFeeBps:     "100",     // 1 %
    liquidationFeeCap:     "10000000000", // 10 SOL cap
    resolvePriceDeviationBps:"500",   // 5 %
    // §1.4 envelope: max(liq_fee_raw, min_liq_abs) + loss <= mm_req
    minLiquidationAbs:     "1000000",   // 0.001 SOL
    minNonzeroMmReq:       "10000000",  // 0.01 SOL
    minNonzeroImReq:       "20000000",  // 0.02 SOL
    // §1.4: cap × 100 + funding_part + liq_fee_bps <= mm_bps (500)
    //   2 bps × 100 = 200 + 100 (liq) = 300, leaves 200 for funding. OK.
    maxPriceMoveBpsPerSlot:"2",

    // Extended tail
    insuranceWithdrawMaxBps: 0,           // disable until matcher PR
    insuranceWithdrawCooldownSlots: "0",
    permissionlessResolveStaleSlots: "100", // = MAX_ACCRUAL_DT_SLOTS hard cap
    fundingHorizonSlots:  "7200",         // ~48 min EWMA
    fundingKBps:          "100",
    fundingMaxPremiumBps: "500",          // 5 % cap
    fundingMaxE9PerSlot:  "1000",         // 1e-6/slot ≈ 0.022%/day
    markMinFee:           "0",
    forceCloseDelaySlots: "200",          // 80 s post-resolve
  };
}

// ─── Main ─────────────────────────────────────────────────────────────────

async function main() {
  const force = hasFlag("--force");
  const insuranceOnly = hasFlag("--insurance-only");
  const simulate = hasFlag("--simulate");
  if (simulate) {
    console.log("(--simulate not implemented; this script is destructive — use --force / --insurance-only flags)");
    process.exitCode = 2;
    return;
  }

  console.log("═".repeat(72));
  console.log("BALLAST PHASE 0 — SOL/USD SLAB DEPLOYMENT (Pyth Pull, INVERTED)");
  console.log("═".repeat(72));

  const cfg = loadConfig();
  const conn = new Connection(cfg.rpcUrl, "confirmed");
  const programId = new PublicKey(cfg.percolatorProgramId);
  const payer = loadKeypair(cfg.wallets.lp);
  const oracleAccount = new PublicKey(cfg.slabs.solUsd.oracle);
  const feedIdHex = cfg.pyth.feeds["SOL/USD"];

  console.log(`RPC:          ${cfg.rpcUrl}`);
  console.log(`Program:      ${programId.toBase58()}`);
  console.log(`Payer/admin:  ${payer.publicKey.toBase58()}  (LP wallet from config)`);
  const startBal = await conn.getBalance(payer.publicKey);
  console.log(`SOL balance:  ${(startBal / LAMPORTS_PER_SOL).toFixed(4)}`);

  // ── [1] Pyth oracle liveness ──
  console.log(`\n[1] Verifying Pyth SOL/USD oracle ${oracleAccount.toBase58()}`);
  const px = await verifyPythPriceAccount(conn, oracleAccount, {
    expectFeedIdHex: feedIdHex,
    maxAgeSecs: 300,
  });
  console.log(`    feed_id:     ${px.feedIdHex}`);
  console.log(`    price:       $${px.priceFloat.toFixed(4)}  (publish_time=${px.publishTime}, age ${px.ageSec}s)`);
  console.log(`    conf:        ${px.conf}  (${px.confBps} bps of price)`);
  console.log(`    exponent:    ${px.exponent}`);

  // ── existing-slab guard ──
  const existing = cfg.slabs.solUsd.slab;
  if (existing && !insuranceOnly && !force) {
    console.log(`\nSlab already deployed at ${existing}. Re-run with:`);
    console.log("  --insurance-only   to top up the insurance fund only");
    console.log("  --force            to deploy a NEW slab (overwrites config)");
    process.exitCode = 0;
    return;
  }

  let slabPk: PublicKey;
  let vaultAta: PublicKey;
  let vaultPda: PublicKey;

  if (insuranceOnly) {
    if (!existing) throw new Error("--insurance-only requires an existing slab in config");
    slabPk = new PublicKey(existing);
    vaultAta = new PublicKey(cfg.slabs.solUsd.vault);
    vaultPda = new PublicKey(cfg.slabs.solUsd.vaultPda);
    console.log(`\n[2-4] Skipped (--insurance-only). Using slab ${slabPk.toBase58()}.`);
  } else {
    const out = await deploySlab(conn, payer, programId, oracleAccount, feedIdHex);
    slabPk = out.slab;
    vaultAta = out.vaultAta;
    vaultPda = out.vaultPda;

    cfg.slabs.solUsd.slab = slabPk.toBase58();
    cfg.slabs.solUsd.vault = vaultAta.toBase58();
    cfg.slabs.solUsd.vaultPda = vaultPda.toBase58();
    cfg.slabs.solUsd.collateralMint = NATIVE_MINT.toBase58();
    saveConfig(cfg);
    console.log(`\n[5] Wrote ${CONFIG_PATH} (slabs.solUsd.{slab,vault,vaultPda,collateralMint})`);
  }

  // ── [6] Wrap + insurance top-up ──
  console.log(`\n[6] Wrapping SOL for insurance top-up`);
  const payerAta = await ensureWrappedSol(conn, payer, WRAP_LAMPORTS);

  console.log(`\n[7] TopUpInsurance ${Number(INSURANCE_FUND_LAMPORTS) / LAMPORTS_PER_SOL} SOL`);
  const topupSig = await topUpInsurance(
    conn, payer, programId, slabPk, vaultAta, payerAta, INSURANCE_FUND_LAMPORTS,
  );
  console.log(`    sig: ${topupSig}`);

  // ── [8] Verify ──
  console.log(`\n[8] Verifying slab state`);
  const slabData = await fetchSlab(conn, slabPk);
  const header = parseHeader(slabData);
  const mcfg = parseConfig(slabData);
  const engine = parseEngine(slabData);
  console.log(`    admin:               ${header.admin.toBase58()}`);
  console.log(`    inverted:            ${mcfg.invert === 1 ? "yes" : "no"}`);
  console.log(`    max_staleness_secs:  ${mcfg.maxStalenessSecs}`);
  console.log(`    conf_filter_bps:     ${mcfg.confFilterBps}`);
  console.log(`    perm_resolve slots:  ${mcfg.permissionlessResolveStaleSlots}`);
  console.log(`    last_oracle_price:   ${engine.lastOraclePrice}  (engine-space, after invert)`);
  console.log(`    insurance balance:   ${engine.insuranceFund.balance}  (= ${Number(engine.insuranceFund.balance) / LAMPORTS_PER_SOL} SOL)`);

  // ── deployment manifest ──
  const manifest = {
    network: cfg.network,
    createdAt: new Date().toISOString(),
    programId: programId.toBase58(),
    slab: slabPk.toBase58(),
    slabSize: SLAB_LEN,
    mint: NATIVE_MINT.toBase58(),
    collateral: "wSOL (9 decimals, unit_scale=0)",
    vault: vaultAta.toBase58(),
    vaultPda: vaultPda.toBase58(),
    oracle: oracleAccount.toBase58(),
    oracleType: "pyth-pull",
    oracleFeedId: feedIdHex,
    inverted: true,
    maxStalenessSecs: PYTH_MAX_STALENESS_SECS,
    confFilterBps: PYTH_CONF_FILTER_BPS,
    insuranceFundLamports: Number(INSURANCE_FUND_LAMPORTS),
    insuranceTopUpSig: topupSig,
    notes: [
      "LP / matcher / participant init deferred to PRD Step 0.4 (matcher PR).",
      "Authorities (admin, insurance, hyperp_mark) deliberately NOT burned —",
      "Phase 0 controlled scenarios (PRD §4.5 Mode 2 / SC-0.4 / SC-0.9) need them live.",
    ],
  };
  const manifestPath = path.resolve(process.cwd(), "config/ballast-sol-deploy.json");
  fs.writeFileSync(manifestPath, JSON.stringify(manifest, null, 2) + "\n");
  console.log(`\n[9] Wrote ${manifestPath}`);

  const endBal = await conn.getBalance(payer.publicKey);
  console.log(`\nSOL spent:    ${((startBal - endBal) / LAMPORTS_PER_SOL).toFixed(4)}`);
  console.log(`SOL remain:   ${(endBal / LAMPORTS_PER_SOL).toFixed(4)}`);
  console.log("═".repeat(72));
  console.log("Done. Next: verify with");
  console.log(`  npx tsx scripts/dump-market.ts --slab ${slabPk.toBase58()}`);
}

main().catch((e) => { console.error("FATAL:", e?.message ?? e); process.exit(1); });
