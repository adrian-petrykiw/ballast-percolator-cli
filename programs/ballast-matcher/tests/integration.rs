//! Integration tests for ballast-matcher via solana-program-test.
//!
//! These tests exercise paths the inline `lib.rs` unit tests can't reach:
//!   - The `process_instruction` entrypoint dispatch.
//!   - Account-handling at the runtime level (ownership, writability,
//!     signer flag, magic check, FM-3 stored-lp equality).
//!   - State mutation visible across transaction boundaries.
//!
//! Pricing-math correctness is covered by the 62 inline tests; these tests
//! deliberately do not re-test the math. Each test sets up a fresh
//! `ProgramTest` so cases don't share state.

use ballast_matcher::*;
use solana_program::{
    instruction::{AccountMeta, Instruction, InstructionError},
    pubkey::Pubkey,
    system_instruction,
};
use solana_program_test::{processor, BanksClient, BanksClientError, ProgramTest, ProgramTestContext};
use solana_sdk::{
    signature::{Keypair, Signer},
    transaction::{Transaction, TransactionError},
};

// =============================================================================
// Setup helpers
// =============================================================================

/// Deterministic program ID for tests. Any non-zero 32-byte value works;
/// solana-program-test doesn't enforce PDA-derivation properties on test IDs.
fn test_program_id() -> Pubkey {
    Pubkey::new_from_array([7u8; 32])
}

async fn start_test() -> (ProgramTestContext, Pubkey) {
    let prog_id = test_program_id();
    let pt = ProgramTest::new("ballast_matcher", prog_id, processor!(process_instruction));
    let ctx = pt.start_with_context().await;
    (ctx, prog_id)
}

// =============================================================================
// Payload + instruction builders
// =============================================================================

/// All fields of the 200-byte init payload, with sane defaults so tests can
/// override only the fields they care about.
struct InitArgs {
    kind: u8,
    allow_fills: u8,
    fee_bps: u32,
    spread_bps: u32,
    max_total_bps: u32,
    impact_k_bps: u32,
    liquidity_e6: u128,
    max_fill_abs: u128,
    max_inventory_abs: u128,
    allowlist_count: u8,
    allowlist: [[u8; 32]; 4],
}

impl Default for InitArgs {
    fn default() -> Self {
        Self {
            kind: KIND_PASSIVE,
            allow_fills: ALLOW_FILLS_NEVER,
            fee_bps: 0,
            spread_bps: 0,
            max_total_bps: 0,
            impact_k_bps: 0,
            liquidity_e6: 0,
            max_fill_abs: 0,
            max_inventory_abs: 0,
            allowlist_count: 0,
            allowlist: [[0u8; 32]; 4],
        }
    }
}

fn encode_init_payload(a: &InitArgs) -> Vec<u8> {
    let mut buf = vec![0u8; INIT_LEN];
    buf[0] = TAG_INIT;
    buf[INIT_OFF_KIND] = a.kind;
    buf[INIT_OFF_ALLOW_FILLS] = a.allow_fills;
    buf[INIT_OFF_FEE..INIT_OFF_FEE + 4].copy_from_slice(&a.fee_bps.to_le_bytes());
    buf[INIT_OFF_SPREAD..INIT_OFF_SPREAD + 4].copy_from_slice(&a.spread_bps.to_le_bytes());
    buf[INIT_OFF_MAX_TOTAL_BPS..INIT_OFF_MAX_TOTAL_BPS + 4]
        .copy_from_slice(&a.max_total_bps.to_le_bytes());
    buf[INIT_OFF_IMPACT_K..INIT_OFF_IMPACT_K + 4]
        .copy_from_slice(&a.impact_k_bps.to_le_bytes());
    buf[INIT_OFF_LIQUIDITY..INIT_OFF_LIQUIDITY + 16]
        .copy_from_slice(&a.liquidity_e6.to_le_bytes());
    buf[INIT_OFF_MAX_FILL..INIT_OFF_MAX_FILL + 16]
        .copy_from_slice(&a.max_fill_abs.to_le_bytes());
    buf[INIT_OFF_MAX_INVENTORY..INIT_OFF_MAX_INVENTORY + 16]
        .copy_from_slice(&a.max_inventory_abs.to_le_bytes());
    buf[INIT_OFF_COUNT] = a.allowlist_count;
    for i in 0..4 {
        let off = INIT_OFF_ALLOWLIST + i * ALLOWLIST_SLOT;
        buf[off..off + ALLOWLIST_SLOT].copy_from_slice(&a.allowlist[i]);
    }
    buf
}

fn encode_match_payload(req_id: u64, oracle_e6: u64, req_size: i128, lp_acct_id: u64) -> Vec<u8> {
    let mut buf = vec![0u8; MATCH_LEN];
    buf[0] = TAG_MATCH;
    buf[MATCH_OFF_REQ_ID..MATCH_OFF_REQ_ID + 8].copy_from_slice(&req_id.to_le_bytes());
    buf[MATCH_OFF_LP_IDX..MATCH_OFF_LP_IDX + 2].copy_from_slice(&0u16.to_le_bytes());
    buf[MATCH_OFF_LP_ACCT..MATCH_OFF_LP_ACCT + 8].copy_from_slice(&lp_acct_id.to_le_bytes());
    buf[MATCH_OFF_ORACLE..MATCH_OFF_ORACLE + 8].copy_from_slice(&oracle_e6.to_le_bytes());
    buf[MATCH_OFF_REQ_SIZE..MATCH_OFF_REQ_SIZE + 16].copy_from_slice(&req_size.to_le_bytes());
    buf
}

/// Atomically create the matcher_ctx account and call BallastMatcher.Init.
/// Returns the ctx pubkey on success.
async fn create_and_init_ctx(
    test_ctx: &mut ProgramTestContext,
    prog_id: &Pubkey,
    lp_pda: &Pubkey,
    args: &InitArgs,
) -> Pubkey {
    let ctx_kp = Keypair::new();
    let ctx_pk = ctx_kp.pubkey();
    let rent = test_ctx.banks_client.get_rent().await.unwrap();
    let lamports = rent.minimum_balance(MATCHER_CTX_SIZE);

    let create_ix = system_instruction::create_account(
        &test_ctx.payer.pubkey(),
        &ctx_pk,
        lamports,
        MATCHER_CTX_SIZE as u64,
        prog_id,
    );
    let init_ix = Instruction {
        program_id: *prog_id,
        accounts: vec![
            AccountMeta::new_readonly(*lp_pda, false),
            AccountMeta::new(ctx_pk, false),
        ],
        data: encode_init_payload(args),
    };
    let mut tx = Transaction::new_with_payer(&[create_ix, init_ix], Some(&test_ctx.payer.pubkey()));
    tx.sign(&[&test_ctx.payer, &ctx_kp], test_ctx.last_blockhash);
    test_ctx
        .banks_client
        .process_transaction(tx)
        .await
        .expect("create_and_init_ctx should succeed");
    ctx_pk
}

/// Send a Match call. `lp_signer` is the keypair whose pubkey was stored as lp_pda
/// at init; if omitted, builds a tx with a non-signer lp meta to exercise FM-2.
async fn send_match(
    test_ctx: &mut ProgramTestContext,
    prog_id: &Pubkey,
    ctx_pk: &Pubkey,
    lp_pubkey: &Pubkey,
    lp_is_signer_meta: bool,
    lp_signer: Option<&Keypair>,
    payload: Vec<u8>,
) -> Result<(), BanksClientError> {
    let match_ix = Instruction {
        program_id: *prog_id,
        accounts: vec![
            AccountMeta {
                pubkey: *lp_pubkey,
                is_signer: lp_is_signer_meta,
                is_writable: false,
            },
            AccountMeta::new(*ctx_pk, false),
        ],
        data: payload,
    };
    let mut tx = Transaction::new_with_payer(&[match_ix], Some(&test_ctx.payer.pubkey()));
    let blockhash = test_ctx.banks_client.get_latest_blockhash().await.unwrap();
    if let Some(kp) = lp_signer {
        tx.try_sign(&[&test_ctx.payer, kp], blockhash).unwrap();
    } else {
        tx.try_sign(&[&test_ctx.payer], blockhash).unwrap();
    }
    test_ctx.banks_client.process_transaction(tx).await
}

async fn fetch_ctx_data(banks: &mut BanksClient, ctx_pk: &Pubkey) -> Vec<u8> {
    banks
        .get_account(*ctx_pk)
        .await
        .unwrap()
        .expect("ctx account must exist")
        .data
}

fn expect_instruction_error(result: Result<(), BanksClientError>, expected: InstructionError) {
    match result {
        Ok(()) => panic!("expected tx to fail with {:?}, but it succeeded", expected),
        Err(BanksClientError::TransactionError(TransactionError::InstructionError(_, ie))) => {
            assert_eq!(ie, expected, "expected {:?}, got {:?}", expected, ie);
        }
        Err(other) => panic!("expected InstructionError({:?}), got {:?}", expected, other),
    }
}

fn parse_match_return(data: &[u8]) -> (u32, u32, u64, i128, u64, u64, u64) {
    let abi_version = u32::from_le_bytes(data[0..4].try_into().unwrap());
    let flags = u32::from_le_bytes(data[4..8].try_into().unwrap());
    let exec_price = u64::from_le_bytes(data[8..16].try_into().unwrap());
    let exec_size = i128::from_le_bytes(data[16..32].try_into().unwrap());
    let req_id = u64::from_le_bytes(data[32..40].try_into().unwrap());
    let lp_acct = u64::from_le_bytes(data[40..48].try_into().unwrap());
    let oracle = u64::from_le_bytes(data[48..56].try_into().unwrap());
    (abi_version, flags, exec_price, exec_size, req_id, lp_acct, oracle)
}

// =============================================================================
// Init tests
// =============================================================================

#[tokio::test]
async fn init_writes_state_and_allowlist() {
    let (mut tctx, prog_id) = start_test().await;
    let lp = Keypair::new();

    let mut allowlist = [[0u8; 32]; 4];
    allowlist[0] = [0xaa; 32];
    allowlist[1] = [0xbb; 32];

    let args = InitArgs {
        kind: KIND_PASSIVE,
        allow_fills: ALLOW_FILLS_FILL,
        fee_bps: 10,
        spread_bps: 40,
        max_total_bps: 500,
        max_fill_abs: 1_000_000,
        max_inventory_abs: 10_000_000,
        allowlist_count: 2,
        allowlist,
        ..Default::default()
    };
    let ctx_pk = create_and_init_ctx(&mut tctx, &prog_id, &lp.pubkey(), &args).await;

    let data = fetch_ctx_data(&mut tctx.banks_client, &ctx_pk).await;
    assert_eq!(data.len(), MATCHER_CTX_SIZE);

    let state = MatcherState::read_from(&data).unwrap();
    assert_eq!(state.magic, BALLAST_MAGIC);
    assert_eq!(state.version, BALLAST_VERSION);
    assert_eq!(state.kind, KIND_PASSIVE);
    assert_eq!(state.allow_trade_cpi_fills, ALLOW_FILLS_FILL);
    assert_eq!(state.lp_pda, lp.pubkey().to_bytes());
    assert_eq!(state.trading_fee_bps, 10);
    assert_eq!(state.base_spread_bps, 40);
    assert_eq!(state.max_total_bps, 500);
    assert_eq!(state.max_fill_abs, 1_000_000);
    assert_eq!(state.max_inventory_abs, 10_000_000);
    // Mutable state initialized to zero.
    assert_eq!(state.inventory_base, 0);
    assert_eq!(state.last_oracle_price_e6, 0);
    assert_eq!(state.last_exec_price_e6, 0);

    // Allowlist count + entries written; unused slots remain zero.
    assert_eq!(data[OFF_ALLOWLIST_COUNT], 2);
    assert_eq!(&data[OFF_PAD1..OFF_PAD1 + 3], &[0u8; 3]);
    assert_eq!(&data[OFF_ALLOWLIST..OFF_ALLOWLIST + 32], &[0xaau8; 32]);
    assert_eq!(&data[OFF_ALLOWLIST + 32..OFF_ALLOWLIST + 64], &[0xbbu8; 32]);
    assert_eq!(&data[OFF_ALLOWLIST + 64..OFF_ALLOWLIST + 96], &[0u8; 32]);
    assert_eq!(&data[OFF_ALLOWLIST + 96..OFF_ALLOWLIST + 128], &[0u8; 32]);
    // Reserved tail untouched.
    assert_eq!(&data[OFF_RESERVED..MATCHER_CTX_SIZE], &[0u8; 44]);
    // Return scratch still zero (Init doesn't write it).
    assert_eq!(&data[0..RETURN_LEN], &[0u8; RETURN_LEN]);
}

#[tokio::test]
async fn init_rejects_reinit() {
    let (mut tctx, prog_id) = start_test().await;
    let lp = Keypair::new();
    let args = InitArgs::default();
    let ctx_pk = create_and_init_ctx(&mut tctx, &prog_id, &lp.pubkey(), &args).await;

    // Try to init the already-initialized ctx again (no create_account this time).
    let init_ix = Instruction {
        program_id: prog_id,
        accounts: vec![
            AccountMeta::new_readonly(lp.pubkey(), false),
            AccountMeta::new(ctx_pk, false),
        ],
        data: encode_init_payload(&args),
    };
    let blockhash = tctx.banks_client.get_latest_blockhash().await.unwrap();
    let mut tx = Transaction::new_with_payer(&[init_ix], Some(&tctx.payer.pubkey()));
    tx.try_sign(&[&tctx.payer], blockhash).unwrap();
    let result = tctx.banks_client.process_transaction(tx).await;
    expect_instruction_error(
        result.map(|_| ()),
        InstructionError::AccountAlreadyInitialized,
    );
}

// =============================================================================
// Match tests — FM-2 / FM-3 / uninitialized ctx
// =============================================================================

#[tokio::test]
async fn match_rejects_uninit_ctx() {
    let (mut tctx, prog_id) = start_test().await;
    let lp = Keypair::new();

    // Create the ctx account but DO NOT call Init.
    let ctx_kp = Keypair::new();
    let rent = tctx.banks_client.get_rent().await.unwrap();
    let lamports = rent.minimum_balance(MATCHER_CTX_SIZE);
    let create_ix = system_instruction::create_account(
        &tctx.payer.pubkey(),
        &ctx_kp.pubkey(),
        lamports,
        MATCHER_CTX_SIZE as u64,
        &prog_id,
    );
    let mut tx = Transaction::new_with_payer(&[create_ix], Some(&tctx.payer.pubkey()));
    tx.try_sign(&[&tctx.payer, &ctx_kp], tctx.last_blockhash).unwrap();
    tctx.banks_client.process_transaction(tx).await.unwrap();

    // Match against the uninit ctx → UninitializedAccount.
    let result = send_match(
        &mut tctx,
        &prog_id,
        &ctx_kp.pubkey(),
        &lp.pubkey(),
        true,
        Some(&lp),
        encode_match_payload(1, 100_000_000, 1_000, 7),
    )
    .await;
    expect_instruction_error(result, InstructionError::UninitializedAccount);
}

#[tokio::test]
async fn match_rejects_missing_signer() {
    let (mut tctx, prog_id) = start_test().await;
    let lp = Keypair::new();
    let args = InitArgs {
        allow_fills: ALLOW_FILLS_FILL,
        max_fill_abs: 1_000_000,
        max_total_bps: 100,
        ..Default::default()
    };
    let ctx_pk = create_and_init_ctx(&mut tctx, &prog_id, &lp.pubkey(), &args).await;

    // FM-2: build the Match tx with the LP meta is_signer=false. The tx is signed only
    // by the payer, so the runtime sees lp_pda as a non-signer ⇒ MissingRequiredSignature.
    let result = send_match(
        &mut tctx,
        &prog_id,
        &ctx_pk,
        &lp.pubkey(),
        false,        // is_signer meta = false
        None,         // no LP keypair in signers
        encode_match_payload(1, 100_000_000, 1_000, 7),
    )
    .await;
    expect_instruction_error(result, InstructionError::MissingRequiredSignature);
}

#[tokio::test]
async fn match_rejects_wrong_lp_pda() {
    let (mut tctx, prog_id) = start_test().await;
    let stored_lp = Keypair::new();
    let attacker_lp = Keypair::new();
    let args = InitArgs {
        allow_fills: ALLOW_FILLS_FILL,
        max_fill_abs: 1_000_000,
        max_total_bps: 100,
        ..Default::default()
    };
    let ctx_pk = create_and_init_ctx(&mut tctx, &prog_id, &stored_lp.pubkey(), &args).await;

    // FM-3: send Match against the real ctx but with a signed attacker LP at accounts[0].
    // is_signer = true (attacker signs), but stored lp_pda != accounts[0] ⇒ InvalidAccountData.
    let result = send_match(
        &mut tctx,
        &prog_id,
        &ctx_pk,
        &attacker_lp.pubkey(),
        true,
        Some(&attacker_lp),
        encode_match_payload(1, 100_000_000, 1_000, 7),
    )
    .await;
    expect_instruction_error(result, InstructionError::InvalidAccountData);
}

// =============================================================================
// Match tests — happy-path dispatch + state mutation
// =============================================================================

#[tokio::test]
async fn match_never_mode_writes_rejected_and_leaves_state_unchanged() {
    let (mut tctx, prog_id) = start_test().await;
    let lp = Keypair::new();
    let args = InitArgs {
        allow_fills: ALLOW_FILLS_NEVER,
        max_fill_abs: 1_000_000,
        max_total_bps: 100,
        // inventory_base starts at 0 by init contract — nothing to compare against besides "still 0"
        ..Default::default()
    };
    let ctx_pk = create_and_init_ctx(&mut tctx, &prog_id, &lp.pubkey(), &args).await;

    send_match(
        &mut tctx,
        &prog_id,
        &ctx_pk,
        &lp.pubkey(),
        true,
        Some(&lp),
        encode_match_payload(42, 100_000_000, 1_000, 7),
    )
    .await
    .unwrap();

    let data = fetch_ctx_data(&mut tctx.banks_client, &ctx_pk).await;
    let (abi, flags, price, size, req_id, lp_acct, oracle) = parse_match_return(&data[0..64]);
    assert_eq!(abi, ABI_VERSION);
    assert_eq!(flags, FLAG_VALID | FLAG_REJECTED);
    assert_eq!(price, REJECTED_EXEC_PRICE);
    assert_eq!(size, 0);
    assert_eq!(req_id, 42);
    assert_eq!(lp_acct, 7);
    assert_eq!(oracle, 100_000_000);

    // State unchanged — never-mode short-circuits before any state mutation.
    let state = MatcherState::read_from(&data).unwrap();
    assert_eq!(state.inventory_base, 0);
    assert_eq!(state.last_oracle_price_e6, 0);
    assert_eq!(state.last_exec_price_e6, 0);
}

#[tokio::test]
async fn match_passive_buy_updates_state() {
    let (mut tctx, prog_id) = start_test().await;
    let lp = Keypair::new();
    // 50 bps total: exec = oracle * 10050 / 10000.
    let args = InitArgs {
        allow_fills: ALLOW_FILLS_FILL,
        kind: KIND_PASSIVE,
        fee_bps: 10,
        spread_bps: 40,
        max_total_bps: 50,
        max_fill_abs: 1_000_000,
        ..Default::default()
    };
    let ctx_pk = create_and_init_ctx(&mut tctx, &prog_id, &lp.pubkey(), &args).await;

    send_match(
        &mut tctx,
        &prog_id,
        &ctx_pk,
        &lp.pubkey(),
        true,
        Some(&lp),
        encode_match_payload(1, 100_000_000, 1_000, 7),
    )
    .await
    .unwrap();

    let data = fetch_ctx_data(&mut tctx.banks_client, &ctx_pk).await;
    let (_, flags, price, size, _, _, _) = parse_match_return(&data[0..64]);
    assert_eq!(flags, FLAG_VALID);
    assert_eq!(price, 100_500_000);
    assert_eq!(size, 1_000);

    // Taker buys 1000 ⇒ LP inventory decreases by 1000.
    let state = MatcherState::read_from(&data).unwrap();
    assert_eq!(state.inventory_base, -1_000);
    assert_eq!(state.last_oracle_price_e6, 100_000_000);
    assert_eq!(state.last_exec_price_e6, 100_500_000);
}

#[tokio::test]
async fn match_passive_sell_marks_down_and_increments_inventory() {
    let (mut tctx, prog_id) = start_test().await;
    let lp = Keypair::new();
    let args = InitArgs {
        allow_fills: ALLOW_FILLS_FILL,
        kind: KIND_PASSIVE,
        fee_bps: 10,
        spread_bps: 40,
        max_total_bps: 50,
        max_fill_abs: 1_000_000,
        ..Default::default()
    };
    let ctx_pk = create_and_init_ctx(&mut tctx, &prog_id, &lp.pubkey(), &args).await;

    send_match(
        &mut tctx,
        &prog_id,
        &ctx_pk,
        &lp.pubkey(),
        true,
        Some(&lp),
        encode_match_payload(1, 100_000_000, -1_000, 7),
    )
    .await
    .unwrap();

    let data = fetch_ctx_data(&mut tctx.banks_client, &ctx_pk).await;
    let (_, flags, price, size, _, _, _) = parse_match_return(&data[0..64]);
    assert_eq!(flags, FLAG_VALID);
    assert_eq!(price, 99_500_000);
    assert_eq!(size, -1_000);

    // Taker sells 1000 ⇒ LP inventory increases by 1000.
    let state = MatcherState::read_from(&data).unwrap();
    assert_eq!(state.inventory_base, 1_000);
    assert_eq!(state.last_exec_price_e6, 99_500_000);
}

#[tokio::test]
async fn match_vamm_kind_dispatches_with_impact() {
    let (mut tctx, prog_id) = start_test().await;
    let lp = Keypair::new();
    // vAMM with fee + spread = 50 bps, impact_k = 100, liquidity = 100_000_000 (notional-e6).
    // Taker buys 1_000_000 at oracle 100_000_000 → abs_notional_e6 = 100_000_000 → impact = 100bps.
    // total = 50 + 100 = 150bps. exec = oracle * 10150 / 10000 = 101_500_000.
    let args = InitArgs {
        allow_fills: ALLOW_FILLS_FILL,
        kind: KIND_VAMM,
        fee_bps: 10,
        spread_bps: 40,
        max_total_bps: 1_000,
        impact_k_bps: 100,
        liquidity_e6: 100_000_000,
        max_fill_abs: 10_000_000,
        ..Default::default()
    };
    let ctx_pk = create_and_init_ctx(&mut tctx, &prog_id, &lp.pubkey(), &args).await;

    send_match(
        &mut tctx,
        &prog_id,
        &ctx_pk,
        &lp.pubkey(),
        true,
        Some(&lp),
        encode_match_payload(1, 100_000_000, 1_000_000, 7),
    )
    .await
    .unwrap();

    let data = fetch_ctx_data(&mut tctx.banks_client, &ctx_pk).await;
    let (_, flags, price, size, _, _, _) = parse_match_return(&data[0..64]);
    assert_eq!(flags, FLAG_VALID);
    assert_eq!(price, 101_500_000);
    assert_eq!(size, 1_000_000);

    let state = MatcherState::read_from(&data).unwrap();
    assert_eq!(state.inventory_base, -1_000_000);
    assert_eq!(state.last_exec_price_e6, 101_500_000);
}

#[tokio::test]
async fn match_zero_fill_leaves_state_unchanged() {
    let (mut tctx, prog_id) = start_test().await;
    let lp = Keypair::new();
    // max_fill_abs = 0 forces zero-fill regardless of request size.
    let args = InitArgs {
        allow_fills: ALLOW_FILLS_FILL,
        kind: KIND_PASSIVE,
        fee_bps: 10,
        spread_bps: 40,
        max_total_bps: 50,
        max_fill_abs: 0,
        ..Default::default()
    };
    let ctx_pk = create_and_init_ctx(&mut tctx, &prog_id, &lp.pubkey(), &args).await;

    send_match(
        &mut tctx,
        &prog_id,
        &ctx_pk,
        &lp.pubkey(),
        true,
        Some(&lp),
        encode_match_payload(1, 100_000_000, 1_000, 7),
    )
    .await
    .unwrap();

    let data = fetch_ctx_data(&mut tctx.banks_client, &ctx_pk).await;
    let (_, flags, price, size, _, _, _) = parse_match_return(&data[0..64]);
    assert_eq!(flags, FLAG_VALID | FLAG_PARTIAL_OK);
    assert_eq!(price, 100_000_000); // oracle, not REJECTED_EXEC_PRICE
    assert_eq!(size, 0);

    // State must remain at init values — zero-fill is exec_size == 0, no state write.
    let state = MatcherState::read_from(&data).unwrap();
    assert_eq!(state.inventory_base, 0);
    assert_eq!(state.last_oracle_price_e6, 0);
    assert_eq!(state.last_exec_price_e6, 0);
}
