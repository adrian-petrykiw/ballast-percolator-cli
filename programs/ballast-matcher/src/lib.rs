//! Ballast allowlist matcher — Percolator-compatible matcher program (devnet POC).
//!
//! Implements PRD §4.7 (post-D1-pivot — see docs/prompts/MATCHER_IMPL_HANDOFF.md §1):
//!   - FM-2: LP PDA must be a signer on every Match call. The Percolator program is the
//!     only entity that can produce this signature (via `invoke_signed` from TradeCpi),
//!     so this gate restricts callers to "the percolator program acting on its TradeCpi
//!     handler's behalf".
//!   - FM-3: stored LP PDA must equal accounts[0] passed in the Match call. Defends
//!     against ctx-substitution where an attacker pairs our matcher_ctx with a different
//!     LP PDA's signature.
//!   - Default Match behavior (`allow_trade_cpi_fills = 0`): zero-fill + REJECTED. This
//!     closes the trade-cpi side door so the off-chain LP signing service's gate over
//!     trade-nocpi is the only fill path during Phase 0.
//!   - Optional fill modes (`allow_trade_cpi_fills = 1`):
//!       * Passive (kind=0): exec_price = oracle ± min(max_total_bps, fee + spread).
//!       * vAMM    (kind=1): exec_price = oracle ± min(max_total_bps, fee + spread +
//!         clamped_impact), where impact_bps = impact_k_bps * |notional_e6| / liquidity.
//!     Reserved for Phase 1 freight (Hyperp slabs can't use trade-nocpi); same binary,
//!     different init flag and kind.
//!
//! The on-chain allowlist (≤ 4 pubkey slots) is audit metadata: the LP signing service's
//! public commitment to whom it will sign for. Per-counterparty enforcement is off-chain
//! because the Match-CPI ABI does not pass counterparty pubkey to the matcher.
//!
//! Pricing math (compute_passive_fill / compute_vamm_fill / check_inventory_limit) is
//! ported from upstream `percolator-match` (aeyakovenko/percolator-match) vamm.rs:516-683
//! with field-name updates. Layout mirrors upstream's MatcherCtx field-for-field through
//! byte 207 (with one byte stolen from upstream's _pad0 for `allow_trade_cpi_fills`); our
//! allowlist + reduced reserved tail replaces upstream's 112-byte _reserved.
//!
//! Hand-rolled byte slicing — no bytemuck/borsh, no allocations.

#![allow(unexpected_cfgs)]

use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    program_error::ProgramError,
    pubkey::Pubkey,
};

// =============================================================================
// Constants
// =============================================================================

/// Total ctx account size: 64-byte return scratch + 144-byte runtime state +
/// 4-byte (count + pad) + 128-byte allowlist + 44-byte reserved tail = 384 bytes.
pub const MATCHER_CTX_SIZE: usize = 384;
/// Return data is written to ctx[0..64] on every Match call (Percolator ABI v2).
pub const RETURN_OFFSET: usize = 0;
pub const RETURN_LEN: usize = 64;

// --- Runtime state field offsets (absolute, within the 384-byte ctx data) ---
// Mirrors upstream MatcherCtx layout starting at byte 64. One byte stolen from
// upstream's _pad0[3] for our `allow_trade_cpi_fills` flag at byte 77.
pub const OFF_MAGIC: usize           = 64;   // u64 LE
pub const OFF_VERSION: usize         = 72;   // u32 LE
pub const OFF_KIND: usize            = 76;   // u8 (0 = Passive, 1 = vAMM)
pub const OFF_ALLOW_FILLS: usize     = 77;   // u8 (0 = never, 1 = fill)
pub const OFF_PAD0: usize            = 78;   // [u8; 2] — strict zero
pub const OFF_LP_PDA: usize          = 80;   // [u8; 32] — FM-3 verification target
pub const OFF_TRADING_FEE_BPS: usize = 112;  // u32 LE
pub const OFF_BASE_SPREAD_BPS: usize = 116;  // u32 LE
pub const OFF_MAX_TOTAL_BPS: usize   = 120;  // u32 LE — stored cap (≤ 9000)
pub const OFF_IMPACT_K_BPS: usize    = 124;  // u32 LE — vAMM only
pub const OFF_LIQUIDITY: usize       = 128;  // u128 LE — vAMM only
pub const OFF_MAX_FILL_ABS: usize    = 144;  // u128 LE — per-call cap
pub const OFF_INVENTORY_BASE: usize  = 160;  // i128 LE — mutable on fill
pub const OFF_LAST_ORACLE: usize     = 176;  // u64 LE — audit trail
pub const OFF_LAST_EXEC: usize       = 184;  // u64 LE — audit trail
pub const OFF_MAX_INVENTORY: usize   = 192;  // u128 LE — inventory cap (0 = uncapped)

// --- Allowlist + reserved tail (absolute) ---
pub const OFF_ALLOWLIST_COUNT: usize = 208;  // u8 (≤ ALLOWLIST_MAX)
pub const OFF_PAD1: usize            = 209;  // [u8; 3] — strict zero
pub const OFF_ALLOWLIST: usize       = 212;  // 4 × 32 = 128 bytes
pub const OFF_RESERVED: usize        = 340;  // [u8; 44] — ends at 384

pub const ALLOWLIST_MAX: u8 = 4;
pub const ALLOWLIST_SLOT: usize = 32;

/// "BALLAST\0" little-endian: u64 LE bytes are 0x42 0x41 0x4C 0x4C 0x41 0x53 0x54 0x00.
/// Distinct from upstream's MATCHER_MAGIC (0x5045_5243_4d41_5443 → "PERCMATC"); ensures
/// an upstream ctx fed to our matcher (or vice versa) fails the magic check immediately.
pub const BALLAST_MAGIC: u64   = 0x0054_5341_4C4C_4142;
pub const BALLAST_VERSION: u32 = 1;

pub const KIND_PASSIVE: u8 = 0;
pub const KIND_VAMM: u8    = 1;

pub const ALLOW_FILLS_NEVER: u8 = 0;
pub const ALLOW_FILLS_FILL: u8  = 1;

// Instruction tags — fixed by upstream Percolator-Match ABI.
pub const TAG_MATCH: u8 = 0;
pub const TAG_INIT: u8  = 2;

// Match payload (67 bytes, upstream-fixed).
pub const MATCH_LEN: usize          = 67;
pub const MATCH_OFF_REQ_ID: usize   = 1;
pub const MATCH_OFF_LP_IDX: usize   = 9;
pub const MATCH_OFF_LP_ACCT: usize  = 11;
pub const MATCH_OFF_ORACLE: usize   = 19;
pub const MATCH_OFF_REQ_SIZE: usize = 27;
pub const MATCH_OFF_RESERVED: usize = 43;

// Init payload (200 bytes, Ballast-custom — upstream superset with allow_fills + allowlist).
pub const INIT_LEN: usize                 = 200;
pub const INIT_OFF_KIND: usize            = 1;
pub const INIT_OFF_ALLOW_FILLS: usize     = 2;
pub const INIT_OFF_PAD0: usize            = 3;   // 1 byte, must be zero
pub const INIT_OFF_FEE: usize             = 4;
pub const INIT_OFF_SPREAD: usize          = 8;
pub const INIT_OFF_MAX_TOTAL_BPS: usize   = 12;
pub const INIT_OFF_IMPACT_K: usize        = 16;
pub const INIT_OFF_LIQUIDITY: usize       = 20;
pub const INIT_OFF_MAX_FILL: usize        = 36;
pub const INIT_OFF_MAX_INVENTORY: usize   = 52;
pub const INIT_OFF_COUNT: usize           = 68;
pub const INIT_OFF_PAD1: usize            = 69;  // 3 bytes, must be zero
pub const INIT_OFF_ALLOWLIST: usize       = 72;

// Return ABI (64 bytes).
pub const ABI_VERSION: u32     = 2;
pub const FLAG_VALID: u32      = 1;
pub const FLAG_PARTIAL_OK: u32 = 2;
pub const FLAG_REJECTED: u32   = 4;

// Pricing.
pub const BPS_DENOM: u128 = 10_000;
/// Hard ceiling on the configurable per-instance `max_total_bps`. Matches upstream's
/// `MatcherCtx::validate` heuristic — leaves a 1000-bps safety margin under the
/// 10000-bps underflow cliff in the sell branch.
pub const MAX_TOTAL_BPS_CEILING: u32 = 9_000;
/// Upstream convention: REJECTED returns set exec_price = 1 to avoid downstream
/// divide-by-zero in callers that compute notionals.
pub const REJECTED_EXEC_PRICE: u64 = 1;

// =============================================================================
// InitPayload
// =============================================================================

#[derive(Clone, Copy, Debug)]
pub struct InitPayload {
    pub kind: u8,
    pub allow_trade_cpi_fills: u8,
    pub trading_fee_bps: u32,
    pub base_spread_bps: u32,
    pub max_total_bps: u32,
    pub impact_k_bps: u32,
    pub liquidity_notional_e6: u128,
    pub max_fill_abs: u128,
    pub max_inventory_abs: u128,
    pub allowlist_count: u8,
    pub allowlist: [[u8; 32]; 4],
}

pub fn parse_init_payload(data: &[u8]) -> Result<InitPayload, ProgramError> {
    if data.len() != INIT_LEN {
        return Err(ProgramError::InvalidInstructionData);
    }
    if data[0] != TAG_INIT {
        return Err(ProgramError::InvalidInstructionData);
    }
    let kind = data[INIT_OFF_KIND];
    if kind != KIND_PASSIVE && kind != KIND_VAMM {
        return Err(ProgramError::InvalidInstructionData);
    }
    let allow = data[INIT_OFF_ALLOW_FILLS];
    if allow != ALLOW_FILLS_NEVER && allow != ALLOW_FILLS_FILL {
        return Err(ProgramError::InvalidInstructionData);
    }
    // Strict-zero alignment pads — same discipline upstream uses for the reserved bytes
    // in MatcherCall[43..67]; preserves wire-extension optionality.
    if data[INIT_OFF_PAD0] != 0 {
        return Err(ProgramError::InvalidInstructionData);
    }
    for &b in &data[INIT_OFF_PAD1..INIT_OFF_PAD1 + 3] {
        if b != 0 {
            return Err(ProgramError::InvalidInstructionData);
        }
    }
    let trading_fee_bps =
        u32::from_le_bytes(data[INIT_OFF_FEE..INIT_OFF_FEE + 4].try_into().unwrap());
    let base_spread_bps =
        u32::from_le_bytes(data[INIT_OFF_SPREAD..INIT_OFF_SPREAD + 4].try_into().unwrap());
    let max_total_bps = u32::from_le_bytes(
        data[INIT_OFF_MAX_TOTAL_BPS..INIT_OFF_MAX_TOTAL_BPS + 4]
            .try_into()
            .unwrap(),
    );
    let impact_k_bps = u32::from_le_bytes(
        data[INIT_OFF_IMPACT_K..INIT_OFF_IMPACT_K + 4].try_into().unwrap(),
    );
    let liquidity_notional_e6 = u128::from_le_bytes(
        data[INIT_OFF_LIQUIDITY..INIT_OFF_LIQUIDITY + 16]
            .try_into()
            .unwrap(),
    );
    let max_fill_abs = u128::from_le_bytes(
        data[INIT_OFF_MAX_FILL..INIT_OFF_MAX_FILL + 16].try_into().unwrap(),
    );
    let max_inventory_abs = u128::from_le_bytes(
        data[INIT_OFF_MAX_INVENTORY..INIT_OFF_MAX_INVENTORY + 16]
            .try_into()
            .unwrap(),
    );

    if max_total_bps > MAX_TOTAL_BPS_CEILING {
        return Err(ProgramError::InvalidInstructionData);
    }
    // `fee + spread` is the floor of total_bps; cap must accommodate it. Use u64 to avoid
    // u32 overflow on the sum even though each side is bounded by 9000 in practice.
    let fee_plus_spread = (trading_fee_bps as u64) + (base_spread_bps as u64);
    if fee_plus_spread > max_total_bps as u64 {
        return Err(ProgramError::InvalidInstructionData);
    }
    // Kind-mode consistency: passive must have no vAMM params; vAMM must have liquidity.
    match kind {
        KIND_PASSIVE => {
            if impact_k_bps != 0 || liquidity_notional_e6 != 0 {
                return Err(ProgramError::InvalidInstructionData);
            }
        }
        KIND_VAMM => {
            if liquidity_notional_e6 == 0 {
                return Err(ProgramError::InvalidInstructionData);
            }
        }
        _ => unreachable!("kind already validated above"),
    }

    let allowlist_count = data[INIT_OFF_COUNT];
    if allowlist_count > ALLOWLIST_MAX {
        return Err(ProgramError::InvalidInstructionData);
    }
    let mut allowlist = [[0u8; 32]; 4];
    for i in 0..4 {
        let off = INIT_OFF_ALLOWLIST + i * ALLOWLIST_SLOT;
        allowlist[i].copy_from_slice(&data[off..off + ALLOWLIST_SLOT]);
    }
    for i in (allowlist_count as usize)..4 {
        if allowlist[i] != [0u8; 32] {
            return Err(ProgramError::InvalidInstructionData);
        }
    }
    Ok(InitPayload {
        kind,
        allow_trade_cpi_fills: allow,
        trading_fee_bps,
        base_spread_bps,
        max_total_bps,
        impact_k_bps,
        liquidity_notional_e6,
        max_fill_abs,
        max_inventory_abs,
        allowlist_count,
        allowlist,
    })
}

// =============================================================================
// MatchCall (67-byte Percolator-Match payload)
// =============================================================================

#[derive(Clone, Copy, Debug)]
pub struct MatchCall {
    pub req_id: u64,
    pub lp_idx: u16,
    pub lp_account_id: u64,
    pub oracle_price_e6: u64,
    pub req_size: i128,
}

pub fn parse_match_call(data: &[u8]) -> Result<MatchCall, ProgramError> {
    // Upstream-compat: accept length >= 67 so future wrappers can append fields after the
    // reserved area without breaking older matchers. The reserved-zero check below is the
    // tampering boundary, not the length check.
    if data.len() < MATCH_LEN {
        return Err(ProgramError::InvalidInstructionData);
    }
    if data[0] != TAG_MATCH {
        return Err(ProgramError::InvalidInstructionData);
    }
    for &b in &data[MATCH_OFF_RESERVED..MATCH_LEN] {
        if b != 0 {
            return Err(ProgramError::InvalidInstructionData);
        }
    }
    let req_id =
        u64::from_le_bytes(data[MATCH_OFF_REQ_ID..MATCH_OFF_REQ_ID + 8].try_into().unwrap());
    let lp_idx =
        u16::from_le_bytes(data[MATCH_OFF_LP_IDX..MATCH_OFF_LP_IDX + 2].try_into().unwrap());
    let lp_account_id =
        u64::from_le_bytes(data[MATCH_OFF_LP_ACCT..MATCH_OFF_LP_ACCT + 8].try_into().unwrap());
    let oracle_price_e6 =
        u64::from_le_bytes(data[MATCH_OFF_ORACLE..MATCH_OFF_ORACLE + 8].try_into().unwrap());
    let req_size = i128::from_le_bytes(
        data[MATCH_OFF_REQ_SIZE..MATCH_OFF_REQ_SIZE + 16].try_into().unwrap(),
    );
    Ok(MatchCall {
        req_id,
        lp_idx,
        lp_account_id,
        oracle_price_e6,
        req_size,
    })
}

// =============================================================================
// MatcherState (covers ctx bytes 64..208 — header + params + mutable state)
// =============================================================================
//
// The allowlist + reserved tail (208..384) is set once at init and never read by Match,
// so it lives outside MatcherState. process_init writes those bytes directly.

#[derive(Clone, Copy, Debug)]
pub struct MatcherState {
    pub magic: u64,
    pub version: u32,
    pub kind: u8,
    pub allow_trade_cpi_fills: u8,
    pub lp_pda: [u8; 32],
    pub trading_fee_bps: u32,
    pub base_spread_bps: u32,
    pub max_total_bps: u32,
    pub impact_k_bps: u32,
    pub liquidity_notional_e6: u128,
    pub max_fill_abs: u128,
    pub inventory_base: i128,
    pub last_oracle_price_e6: u64,
    pub last_exec_price_e6: u64,
    pub max_inventory_abs: u128,
}

impl MatcherState {
    /// Read MatcherState from the full ctx data slice (uses absolute OFF_* offsets).
    /// Verifies magic; returns UninitializedAccount on mismatch.
    pub fn read_from(data: &[u8]) -> Result<Self, ProgramError> {
        if data.len() < MATCHER_CTX_SIZE {
            return Err(ProgramError::AccountDataTooSmall);
        }
        let magic = u64::from_le_bytes(data[OFF_MAGIC..OFF_MAGIC + 8].try_into().unwrap());
        if magic != BALLAST_MAGIC {
            return Err(ProgramError::UninitializedAccount);
        }
        let version = u32::from_le_bytes(data[OFF_VERSION..OFF_VERSION + 4].try_into().unwrap());
        let kind = data[OFF_KIND];
        let allow_trade_cpi_fills = data[OFF_ALLOW_FILLS];
        let mut lp_pda = [0u8; 32];
        lp_pda.copy_from_slice(&data[OFF_LP_PDA..OFF_LP_PDA + 32]);
        let trading_fee_bps = u32::from_le_bytes(
            data[OFF_TRADING_FEE_BPS..OFF_TRADING_FEE_BPS + 4]
                .try_into()
                .unwrap(),
        );
        let base_spread_bps = u32::from_le_bytes(
            data[OFF_BASE_SPREAD_BPS..OFF_BASE_SPREAD_BPS + 4]
                .try_into()
                .unwrap(),
        );
        let max_total_bps = u32::from_le_bytes(
            data[OFF_MAX_TOTAL_BPS..OFF_MAX_TOTAL_BPS + 4]
                .try_into()
                .unwrap(),
        );
        let impact_k_bps = u32::from_le_bytes(
            data[OFF_IMPACT_K_BPS..OFF_IMPACT_K_BPS + 4]
                .try_into()
                .unwrap(),
        );
        let liquidity_notional_e6 = u128::from_le_bytes(
            data[OFF_LIQUIDITY..OFF_LIQUIDITY + 16].try_into().unwrap(),
        );
        let max_fill_abs = u128::from_le_bytes(
            data[OFF_MAX_FILL_ABS..OFF_MAX_FILL_ABS + 16].try_into().unwrap(),
        );
        let inventory_base = i128::from_le_bytes(
            data[OFF_INVENTORY_BASE..OFF_INVENTORY_BASE + 16]
                .try_into()
                .unwrap(),
        );
        let last_oracle_price_e6 = u64::from_le_bytes(
            data[OFF_LAST_ORACLE..OFF_LAST_ORACLE + 8].try_into().unwrap(),
        );
        let last_exec_price_e6 = u64::from_le_bytes(
            data[OFF_LAST_EXEC..OFF_LAST_EXEC + 8].try_into().unwrap(),
        );
        let max_inventory_abs = u128::from_le_bytes(
            data[OFF_MAX_INVENTORY..OFF_MAX_INVENTORY + 16]
                .try_into()
                .unwrap(),
        );
        Ok(Self {
            magic,
            version,
            kind,
            allow_trade_cpi_fills,
            lp_pda,
            trading_fee_bps,
            base_spread_bps,
            max_total_bps,
            impact_k_bps,
            liquidity_notional_e6,
            max_fill_abs,
            inventory_base,
            last_oracle_price_e6,
            last_exec_price_e6,
            max_inventory_abs,
        })
    }

    /// Write MatcherState fields back to ctx data (uses absolute OFF_* offsets).
    /// Does NOT touch the allowlist + reserved tail.
    pub fn write_to(&self, data: &mut [u8]) -> Result<(), ProgramError> {
        if data.len() < MATCHER_CTX_SIZE {
            return Err(ProgramError::AccountDataTooSmall);
        }
        data[OFF_MAGIC..OFF_MAGIC + 8].copy_from_slice(&self.magic.to_le_bytes());
        data[OFF_VERSION..OFF_VERSION + 4].copy_from_slice(&self.version.to_le_bytes());
        data[OFF_KIND] = self.kind;
        data[OFF_ALLOW_FILLS] = self.allow_trade_cpi_fills;
        data[OFF_PAD0..OFF_PAD0 + 2].fill(0);
        data[OFF_LP_PDA..OFF_LP_PDA + 32].copy_from_slice(&self.lp_pda);
        data[OFF_TRADING_FEE_BPS..OFF_TRADING_FEE_BPS + 4]
            .copy_from_slice(&self.trading_fee_bps.to_le_bytes());
        data[OFF_BASE_SPREAD_BPS..OFF_BASE_SPREAD_BPS + 4]
            .copy_from_slice(&self.base_spread_bps.to_le_bytes());
        data[OFF_MAX_TOTAL_BPS..OFF_MAX_TOTAL_BPS + 4]
            .copy_from_slice(&self.max_total_bps.to_le_bytes());
        data[OFF_IMPACT_K_BPS..OFF_IMPACT_K_BPS + 4]
            .copy_from_slice(&self.impact_k_bps.to_le_bytes());
        data[OFF_LIQUIDITY..OFF_LIQUIDITY + 16]
            .copy_from_slice(&self.liquidity_notional_e6.to_le_bytes());
        data[OFF_MAX_FILL_ABS..OFF_MAX_FILL_ABS + 16]
            .copy_from_slice(&self.max_fill_abs.to_le_bytes());
        data[OFF_INVENTORY_BASE..OFF_INVENTORY_BASE + 16]
            .copy_from_slice(&self.inventory_base.to_le_bytes());
        data[OFF_LAST_ORACLE..OFF_LAST_ORACLE + 8]
            .copy_from_slice(&self.last_oracle_price_e6.to_le_bytes());
        data[OFF_LAST_EXEC..OFF_LAST_EXEC + 8]
            .copy_from_slice(&self.last_exec_price_e6.to_le_bytes());
        data[OFF_MAX_INVENTORY..OFF_MAX_INVENTORY + 16]
            .copy_from_slice(&self.max_inventory_abs.to_le_bytes());
        Ok(())
    }

    /// Defense-in-depth validation at execution time. Guards against any path where the
    /// ctx state was mutated outside our parser (currently none exists, but a future bug
    /// could open one). Same checks as init.
    pub fn validate(&self) -> Result<(), ProgramError> {
        if self.kind != KIND_PASSIVE && self.kind != KIND_VAMM {
            return Err(ProgramError::InvalidAccountData);
        }
        if self.allow_trade_cpi_fills != ALLOW_FILLS_NEVER
            && self.allow_trade_cpi_fills != ALLOW_FILLS_FILL
        {
            return Err(ProgramError::InvalidAccountData);
        }
        if self.max_total_bps > MAX_TOTAL_BPS_CEILING {
            return Err(ProgramError::InvalidAccountData);
        }
        let fee_plus_spread = (self.trading_fee_bps as u64) + (self.base_spread_bps as u64);
        if fee_plus_spread > self.max_total_bps as u64 {
            return Err(ProgramError::InvalidAccountData);
        }
        if self.kind == KIND_VAMM && self.liquidity_notional_e6 == 0 {
            return Err(ProgramError::InvalidAccountData);
        }
        if self.lp_pda == [0u8; 32] {
            return Err(ProgramError::InvalidAccountData);
        }
        Ok(())
    }
}

// =============================================================================
// MatchReturn (64-byte Percolator-Match ABI v2)
// =============================================================================

#[derive(Clone, Copy, Debug)]
pub struct MatchReturn {
    pub flags: u32,
    pub exec_price_e6: u64,
    pub exec_size: i128,
    pub req_id: u64,
    pub lp_account_id: u64,
    pub oracle_price_e6: u64,
}

impl MatchReturn {
    /// LP refuses to participate (e.g. allow_trade_cpi_fills = NEVER). Sets exec_price = 1
    /// per upstream convention; FLAG_REJECTED tells the caller this was a hard refusal.
    pub fn rejected(req_id: u64, lp_account_id: u64, oracle_price_e6: u64) -> Self {
        Self {
            flags: FLAG_VALID | FLAG_REJECTED,
            exec_price_e6: REJECTED_EXEC_PRICE,
            exec_size: 0,
            req_id,
            lp_account_id,
            oracle_price_e6,
        }
    }

    /// LP participated but couldn't fill anything (caps zeroed out the fill). Sets
    /// exec_price = oracle so the caller has a meaningful audit price; PARTIAL_OK tells
    /// the caller this is a soft refusal, not a programmer error.
    pub fn zero_fill(req_id: u64, lp_account_id: u64, oracle_price_e6: u64) -> Self {
        Self {
            flags: FLAG_VALID | FLAG_PARTIAL_OK,
            exec_price_e6: oracle_price_e6,
            exec_size: 0,
            req_id,
            lp_account_id,
            oracle_price_e6,
        }
    }

    /// LP filled (possibly partially). FLAG_VALID without PARTIAL_OK; caller infers
    /// partial vs full by comparing exec_size to req_size.
    pub fn filled(
        exec_price_e6: u64,
        exec_size: i128,
        req_id: u64,
        lp_account_id: u64,
        oracle_price_e6: u64,
    ) -> Self {
        Self {
            flags: FLAG_VALID,
            exec_price_e6,
            exec_size,
            req_id,
            lp_account_id,
            oracle_price_e6,
        }
    }

    pub fn write_to(&self, dst: &mut [u8]) -> Result<(), ProgramError> {
        if dst.len() < RETURN_LEN {
            return Err(ProgramError::AccountDataTooSmall);
        }
        dst[0..4].copy_from_slice(&ABI_VERSION.to_le_bytes());
        dst[4..8].copy_from_slice(&self.flags.to_le_bytes());
        dst[8..16].copy_from_slice(&self.exec_price_e6.to_le_bytes());
        dst[16..32].copy_from_slice(&self.exec_size.to_le_bytes());
        dst[32..40].copy_from_slice(&self.req_id.to_le_bytes());
        dst[40..48].copy_from_slice(&self.lp_account_id.to_le_bytes());
        dst[48..56].copy_from_slice(&self.oracle_price_e6.to_le_bytes());
        dst[56..64].copy_from_slice(&0u64.to_le_bytes());
        Ok(())
    }
}

// =============================================================================
// Pricing helpers (ported verbatim from upstream vamm.rs:516-683, field renames only)
// =============================================================================

/// Clamp `fill_abs` to respect the LP inventory cap. Returns the clamped fill magnitude.
/// `is_buy = true` ⇒ taker is buying ⇒ LP inventory decreases by fill_abs.
/// `is_buy = false` ⇒ taker is selling ⇒ LP inventory increases by fill_abs.
///
/// Returns 0 (signals zero-fill upstream) if LP is already at the boundary in the
/// direction the request would push it.
fn check_inventory_limit(
    inventory_base: i128,
    max_inventory_abs: u128,
    fill_abs: u128,
    is_buy: bool,
) -> u128 {
    if max_inventory_abs == 0 {
        return fill_abs;
    }

    let max_inv = max_inventory_abs as i128;

    let inv_delta = if is_buy {
        -(fill_abs as i128)
    } else {
        fill_abs as i128
    };

    let new_inv = inventory_base.saturating_add(inv_delta);

    if new_inv.unsigned_abs() <= max_inventory_abs {
        return fill_abs;
    }

    if is_buy {
        if inventory_base <= -max_inv {
            return 0;
        }
        let max_fill = (inventory_base + max_inv).unsigned_abs();
        core::cmp::min(fill_abs, max_fill)
    } else {
        if inventory_base >= max_inv {
            return 0;
        }
        let max_fill = (max_inv - inventory_base).unsigned_abs();
        core::cmp::min(fill_abs, max_fill)
    }
}

/// Compute passive fill: exec_price = oracle ± min(max_total_bps, fee + spread).
///
/// Returns `(MatchReturn, exec_size_signed)`. Caller updates `inventory_base -= exec_size`
/// when `exec_size != 0` (sign convention: req_size positive = taker buy = LP sells = LP
/// inventory decreases).
pub fn compute_passive_fill(
    state: &MatcherState,
    call: &MatchCall,
) -> Result<(MatchReturn, i128), ProgramError> {
    // Defense-in-depth: even though parse_match_call doesn't validate these, process_match
    // checks them before calling us. Re-check here so unit tests can exercise the guards
    // and so any future caller path is covered.
    if call.oracle_price_e6 == 0 {
        return Err(ProgramError::InvalidInstructionData);
    }
    if call.req_size == i128::MIN {
        return Err(ProgramError::InvalidInstructionData);
    }
    state.validate()?;

    let req_abs = call.req_size.unsigned_abs();
    let is_buy = call.req_size > 0;

    let fill_abs = if state.max_fill_abs == 0 {
        0u128
    } else {
        core::cmp::min(req_abs, state.max_fill_abs)
    };
    let fill_abs =
        check_inventory_limit(state.inventory_base, state.max_inventory_abs, fill_abs, is_buy);

    if fill_abs == 0 {
        return Ok((
            MatchReturn::zero_fill(call.req_id, call.lp_account_id, call.oracle_price_e6),
            0,
        ));
    }

    let exec_size = if is_buy {
        fill_abs as i128
    } else {
        -(fill_abs as i128)
    };

    let base = state.base_spread_bps as u128;
    let fee = state.trading_fee_bps as u128;
    let max_total = state.max_total_bps as u128;
    let total_bps = core::cmp::min(max_total, base + fee);

    let oracle = call.oracle_price_e6 as u128;
    let exec_price_u128 = if is_buy {
        oracle
            .checked_mul(BPS_DENOM + total_bps)
            .ok_or(ProgramError::ArithmeticOverflow)?
            / BPS_DENOM
    } else {
        // total_bps ≤ max_total ≤ MAX_TOTAL_BPS_CEILING (9000) < BPS_DENOM (10000); the
        // subtraction cannot underflow given state.validate() above.
        oracle
            .checked_mul(BPS_DENOM - total_bps)
            .ok_or(ProgramError::ArithmeticOverflow)?
            / BPS_DENOM
    };
    if exec_price_u128 == 0 || exec_price_u128 > u64::MAX as u128 {
        return Err(ProgramError::ArithmeticOverflow);
    }

    Ok((
        MatchReturn::filled(
            exec_price_u128 as u64,
            exec_size,
            call.req_id,
            call.lp_account_id,
            call.oracle_price_e6,
        ),
        exec_size,
    ))
}

/// Compute vAMM fill: exec_price = oracle ± min(max_total_bps, fee + spread +
/// clamped_impact), where impact_bps = impact_k_bps * |notional_e6| / liquidity_notional_e6.
pub fn compute_vamm_fill(
    state: &MatcherState,
    call: &MatchCall,
) -> Result<(MatchReturn, i128), ProgramError> {
    if call.oracle_price_e6 == 0 {
        return Err(ProgramError::InvalidInstructionData);
    }
    if call.req_size == i128::MIN {
        return Err(ProgramError::InvalidInstructionData);
    }
    state.validate()?;

    let req_abs = call.req_size.unsigned_abs();
    let is_buy = call.req_size > 0;

    let fill_abs = if state.max_fill_abs == 0 {
        0u128
    } else {
        core::cmp::min(req_abs, state.max_fill_abs)
    };
    let fill_abs =
        check_inventory_limit(state.inventory_base, state.max_inventory_abs, fill_abs, is_buy);

    if fill_abs == 0 {
        return Ok((
            MatchReturn::zero_fill(call.req_id, call.lp_account_id, call.oracle_price_e6),
            0,
        ));
    }

    let exec_size = if is_buy {
        fill_abs as i128
    } else {
        -(fill_abs as i128)
    };

    let oracle = call.oracle_price_e6 as u128;
    let abs_notional_e6 = fill_abs
        .checked_mul(oracle)
        .ok_or(ProgramError::ArithmeticOverflow)?
        / 1_000_000u128;

    let impact_k = state.impact_k_bps as u128;
    let impact_bps = if state.liquidity_notional_e6 > 0 {
        abs_notional_e6
            .checked_mul(impact_k)
            .ok_or(ProgramError::ArithmeticOverflow)?
            / state.liquidity_notional_e6
    } else {
        // state.validate() rejects vAMM with zero liquidity; this branch is unreachable
        // under normal flow but kept as a safe fallback rather than a panic.
        0
    };

    let base = state.base_spread_bps as u128;
    let fee = state.trading_fee_bps as u128;
    let max_total = state.max_total_bps as u128;
    let max_impact = max_total.saturating_sub(base).saturating_sub(fee);
    let clamped_impact = core::cmp::min(impact_bps, max_impact);

    let total_bps = core::cmp::min(max_total, base + fee + clamped_impact);

    let exec_price_u128 = if is_buy {
        oracle
            .checked_mul(BPS_DENOM + total_bps)
            .ok_or(ProgramError::ArithmeticOverflow)?
            / BPS_DENOM
    } else {
        oracle
            .checked_mul(BPS_DENOM - total_bps)
            .ok_or(ProgramError::ArithmeticOverflow)?
            / BPS_DENOM
    };
    if exec_price_u128 == 0 || exec_price_u128 > u64::MAX as u128 {
        return Err(ProgramError::ArithmeticOverflow);
    }

    Ok((
        MatchReturn::filled(
            exec_price_u128 as u64,
            exec_size,
            call.req_id,
            call.lp_account_id,
            call.oracle_price_e6,
        ),
        exec_size,
    ))
}

// =============================================================================
// Dispatcher
// =============================================================================

pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    if instruction_data.is_empty() {
        return Err(ProgramError::InvalidInstructionData);
    }
    match instruction_data[0] {
        TAG_MATCH => process_match(program_id, accounts, instruction_data),
        TAG_INIT => process_init(program_id, accounts, instruction_data),
        _ => Err(ProgramError::InvalidInstructionData),
    }
}

// =============================================================================
// Init handler (tag 2)
// =============================================================================
//
// Accounts:
//   [0] lp_pda — read; key stored at OFF_LP_PDA. Not a signer (matches upstream's
//                "passive init" pattern; auth comes from atomic create_account+init in
//                setup-ballast-matcher.ts).
//   [1] ctx    — writable, owned by us, freshly created (magic == 0).

fn process_init(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    let iter = &mut accounts.iter();
    let lp_pda = next_account_info(iter)?;
    let ctx = next_account_info(iter)?;

    if ctx.owner != program_id {
        return Err(ProgramError::IncorrectProgramId);
    }
    if !ctx.is_writable {
        return Err(ProgramError::InvalidAccountData);
    }
    if ctx.data_len() < MATCHER_CTX_SIZE {
        return Err(ProgramError::AccountDataTooSmall);
    }
    {
        let data = ctx.try_borrow_data()?;
        let magic = u64::from_le_bytes(data[OFF_MAGIC..OFF_MAGIC + 8].try_into().unwrap());
        if magic != 0 {
            return Err(ProgramError::AccountAlreadyInitialized);
        }
    }

    let p = parse_init_payload(instruction_data)?;

    let state = MatcherState {
        magic: BALLAST_MAGIC,
        version: BALLAST_VERSION,
        kind: p.kind,
        allow_trade_cpi_fills: p.allow_trade_cpi_fills,
        lp_pda: lp_pda.key.to_bytes(),
        trading_fee_bps: p.trading_fee_bps,
        base_spread_bps: p.base_spread_bps,
        max_total_bps: p.max_total_bps,
        impact_k_bps: p.impact_k_bps,
        liquidity_notional_e6: p.liquidity_notional_e6,
        max_fill_abs: p.max_fill_abs,
        inventory_base: 0,
        last_oracle_price_e6: 0,
        last_exec_price_e6: 0,
        max_inventory_abs: p.max_inventory_abs,
    };
    // Belt-and-suspenders: parser already enforces these, but cheap to re-check before
    // we commit state to chain.
    state.validate()?;

    let mut data = ctx.try_borrow_mut_data()?;
    state.write_to(&mut data)?;
    data[OFF_ALLOWLIST_COUNT] = p.allowlist_count;
    data[OFF_PAD1..OFF_PAD1 + 3].fill(0);
    for i in 0..4 {
        let off = OFF_ALLOWLIST + i * ALLOWLIST_SLOT;
        data[off..off + ALLOWLIST_SLOT].copy_from_slice(&p.allowlist[i]);
    }
    // OFF_RESERVED..MATCHER_CTX_SIZE and the [0..64] return scratch are already zero
    // (create_account hands us zeroed memory); leave them.

    Ok(())
}

// =============================================================================
// Match handler (tag 0)
// =============================================================================
//
// Accounts:
//   [0] lp_pda — must be a signer (FM-2). Only the percolator program can produce this
//                signature, via invoke_signed during TradeCpi.
//   [1] ctx    — writable, owned by us, initialized.

fn process_match(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    let iter = &mut accounts.iter();
    let lp_pda = next_account_info(iter)?;
    let ctx = next_account_info(iter)?;

    if ctx.owner != program_id {
        return Err(ProgramError::IncorrectProgramId);
    }
    if ctx.data_len() < MATCHER_CTX_SIZE {
        return Err(ProgramError::AccountDataTooSmall);
    }

    let mut state = {
        let data = ctx.try_borrow_data()?;
        MatcherState::read_from(&data)?
    };
    state.validate()?;

    // FM-2: only the Percolator program can produce this signature via invoke_signed.
    if !lp_pda.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }
    // FM-3: bind the ctx to the LP PDA it was initialized for.
    if state.lp_pda != lp_pda.key.to_bytes() {
        return Err(ProgramError::InvalidAccountData);
    }

    let call = parse_match_call(instruction_data)?;
    if call.oracle_price_e6 == 0 {
        return Err(ProgramError::InvalidInstructionData);
    }
    if call.req_size == i128::MIN {
        return Err(ProgramError::InvalidInstructionData);
    }

    let (ret, exec_size) = match state.allow_trade_cpi_fills {
        ALLOW_FILLS_NEVER => (
            MatchReturn::rejected(call.req_id, call.lp_account_id, call.oracle_price_e6),
            0i128,
        ),
        ALLOW_FILLS_FILL => match state.kind {
            KIND_PASSIVE => compute_passive_fill(&state, &call)?,
            KIND_VAMM => compute_vamm_fill(&state, &call)?,
            _ => return Err(ProgramError::InvalidAccountData),
        },
        _ => return Err(ProgramError::InvalidAccountData),
    };

    let mut data = ctx.try_borrow_mut_data()?;
    ret.write_to(&mut data[RETURN_OFFSET..RETURN_OFFSET + RETURN_LEN])?;
    if exec_size != 0 {
        // Taker buys (exec_size > 0) ⇒ LP sells ⇒ LP inventory decreases. saturating_sub
        // mirrors upstream; check_inventory_limit guarantees we never come near i128::MIN
        // in practice.
        state.inventory_base = state.inventory_base.saturating_sub(exec_size);
        state.last_oracle_price_e6 = call.oracle_price_e6;
        state.last_exec_price_e6 = ret.exec_price_e6;
        state.write_to(&mut data)?;
    }

    Ok(())
}

// =============================================================================
// Entrypoint
// =============================================================================

#[cfg(not(feature = "no-entrypoint"))]
solana_program::entrypoint!(process_instruction);

// =============================================================================
// Inline unit tests for pure helpers
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Test helpers ----

    fn empty_init_buf() -> [u8; INIT_LEN] {
        let mut buf = [0u8; INIT_LEN];
        buf[0] = TAG_INIT;
        buf
    }

    fn empty_match_buf() -> [u8; MATCH_LEN] {
        let mut buf = [0u8; MATCH_LEN];
        buf[0] = TAG_MATCH;
        buf
    }

    fn match_call(oracle: u64, req_size: i128) -> MatchCall {
        MatchCall {
            req_id: 1,
            lp_idx: 0,
            lp_account_id: 2,
            oracle_price_e6: oracle,
            req_size,
        }
    }

    /// Minimal valid passive state for pricing-helper tests. Defaults to fee=0, spread=0,
    /// max_total=0 (so total_bps = 0 ⇒ exec_price = oracle), max_fill = huge so we don't
    /// accidentally trigger the cap.
    fn passive_state() -> MatcherState {
        MatcherState {
            magic: BALLAST_MAGIC,
            version: BALLAST_VERSION,
            kind: KIND_PASSIVE,
            allow_trade_cpi_fills: ALLOW_FILLS_FILL,
            lp_pda: [1u8; 32],
            trading_fee_bps: 0,
            base_spread_bps: 0,
            max_total_bps: 0,
            impact_k_bps: 0,
            liquidity_notional_e6: 0,
            max_fill_abs: u128::MAX,
            inventory_base: 0,
            last_oracle_price_e6: 0,
            last_exec_price_e6: 0,
            max_inventory_abs: 0,
        }
    }

    /// Minimal valid vAMM state. liquidity must be > 0 for validate() to pass.
    fn vamm_state() -> MatcherState {
        MatcherState {
            kind: KIND_VAMM,
            liquidity_notional_e6: 1_000_000_000u128, // 1000e6 notional depth
            impact_k_bps: 100,                         // 1% per unit of liquidity-scaled notional
            ..passive_state()
        }
    }

    // ---- Layout invariants ----

    #[test]
    fn magic_le_bytes_are_ballast_string() {
        assert_eq!(&BALLAST_MAGIC.to_le_bytes(), b"BALLAST\0");
    }

    #[test]
    fn ctx_layout_offsets_terminate_at_size() {
        assert_eq!(OFF_RESERVED + 44, MATCHER_CTX_SIZE);
        assert_eq!(OFF_ALLOWLIST + 4 * ALLOWLIST_SLOT, OFF_RESERVED);
        // Sanity that the runtime-state region ends where allowlist starts.
        assert_eq!(OFF_MAX_INVENTORY + 16, OFF_ALLOWLIST_COUNT);
    }

    #[test]
    fn init_payload_offsets_terminate_at_init_len() {
        assert_eq!(INIT_OFF_ALLOWLIST + 4 * ALLOWLIST_SLOT, INIT_LEN);
    }

    // ---- parse_init_payload — structural ----

    #[test]
    fn init_minimal_zero_buf_parses() {
        let buf = empty_init_buf();
        let p = parse_init_payload(&buf).unwrap();
        assert_eq!(p.kind, KIND_PASSIVE);
        assert_eq!(p.allow_trade_cpi_fills, ALLOW_FILLS_NEVER);
        assert_eq!(p.trading_fee_bps, 0);
        assert_eq!(p.base_spread_bps, 0);
        assert_eq!(p.max_total_bps, 0);
        assert_eq!(p.impact_k_bps, 0);
        assert_eq!(p.liquidity_notional_e6, 0);
        assert_eq!(p.max_fill_abs, 0);
        assert_eq!(p.max_inventory_abs, 0);
        assert_eq!(p.allowlist_count, 0);
        assert_eq!(p.allowlist, [[0u8; 32]; 4]);
    }

    #[test]
    fn init_rejects_wrong_length() {
        assert!(parse_init_payload(&vec![0u8; INIT_LEN - 1]).is_err());
        assert!(parse_init_payload(&vec![0u8; INIT_LEN + 1]).is_err());
    }

    #[test]
    fn init_rejects_bad_tag() {
        let mut buf = empty_init_buf();
        buf[0] = 1;
        assert!(parse_init_payload(&buf).is_err());
    }

    #[test]
    fn init_rejects_bad_kind() {
        let mut buf = empty_init_buf();
        buf[INIT_OFF_KIND] = 2; // 0 and 1 are valid; anything else rejected
        assert!(parse_init_payload(&buf).is_err());
        buf[INIT_OFF_KIND] = 0xff;
        assert!(parse_init_payload(&buf).is_err());
    }

    #[test]
    fn init_accepts_vamm_kind_with_liquidity() {
        let mut buf = empty_init_buf();
        buf[INIT_OFF_KIND] = KIND_VAMM;
        buf[INIT_OFF_LIQUIDITY..INIT_OFF_LIQUIDITY + 16]
            .copy_from_slice(&1_000_000_000u128.to_le_bytes());
        let p = parse_init_payload(&buf).unwrap();
        assert_eq!(p.kind, KIND_VAMM);
        assert_eq!(p.liquidity_notional_e6, 1_000_000_000);
    }

    #[test]
    fn init_rejects_bad_allow_fills_flag() {
        let mut buf = empty_init_buf();
        buf[INIT_OFF_ALLOW_FILLS] = 2;
        assert!(parse_init_payload(&buf).is_err());
        buf[INIT_OFF_ALLOW_FILLS] = 0xff;
        assert!(parse_init_payload(&buf).is_err());
    }

    #[test]
    fn init_accepts_passive_fill_flag() {
        let mut buf = empty_init_buf();
        buf[INIT_OFF_ALLOW_FILLS] = ALLOW_FILLS_FILL;
        assert!(parse_init_payload(&buf).is_ok());
    }

    #[test]
    fn init_rejects_nonzero_pad0() {
        let mut buf = empty_init_buf();
        buf[INIT_OFF_PAD0] = 0xa5;
        assert!(parse_init_payload(&buf).is_err());
    }

    #[test]
    fn init_rejects_nonzero_pad1() {
        for offset in 0..3 {
            let mut buf = empty_init_buf();
            buf[INIT_OFF_PAD1 + offset] = 1;
            assert!(parse_init_payload(&buf).is_err());
        }
    }

    // ---- parse_init_payload — fee/spread/max_total rules ----

    #[test]
    fn init_total_bps_at_cap_accepted() {
        let mut buf = empty_init_buf();
        buf[INIT_OFF_FEE..INIT_OFF_FEE + 4].copy_from_slice(&5_000u32.to_le_bytes());
        buf[INIT_OFF_SPREAD..INIT_OFF_SPREAD + 4].copy_from_slice(&4_000u32.to_le_bytes());
        buf[INIT_OFF_MAX_TOTAL_BPS..INIT_OFF_MAX_TOTAL_BPS + 4]
            .copy_from_slice(&9_000u32.to_le_bytes());
        let p = parse_init_payload(&buf).unwrap();
        assert_eq!(p.trading_fee_bps, 5_000);
        assert_eq!(p.base_spread_bps, 4_000);
        assert_eq!(p.max_total_bps, 9_000);
    }

    #[test]
    fn init_rejects_max_total_above_9000() {
        let mut buf = empty_init_buf();
        buf[INIT_OFF_MAX_TOTAL_BPS..INIT_OFF_MAX_TOTAL_BPS + 4]
            .copy_from_slice(&9_001u32.to_le_bytes());
        assert!(parse_init_payload(&buf).is_err());
    }

    #[test]
    fn init_rejects_fee_plus_spread_above_max_total() {
        let mut buf = empty_init_buf();
        // max_total = 500, but fee + spread = 600 → reject
        buf[INIT_OFF_MAX_TOTAL_BPS..INIT_OFF_MAX_TOTAL_BPS + 4]
            .copy_from_slice(&500u32.to_le_bytes());
        buf[INIT_OFF_FEE..INIT_OFF_FEE + 4].copy_from_slice(&300u32.to_le_bytes());
        buf[INIT_OFF_SPREAD..INIT_OFF_SPREAD + 4].copy_from_slice(&300u32.to_le_bytes());
        assert!(parse_init_payload(&buf).is_err());
    }

    // ---- parse_init_payload — kind/mode consistency ----

    #[test]
    fn init_rejects_passive_with_nonzero_impact_k() {
        let mut buf = empty_init_buf();
        buf[INIT_OFF_KIND] = KIND_PASSIVE;
        buf[INIT_OFF_IMPACT_K..INIT_OFF_IMPACT_K + 4].copy_from_slice(&1u32.to_le_bytes());
        assert!(parse_init_payload(&buf).is_err());
    }

    #[test]
    fn init_rejects_passive_with_nonzero_liquidity() {
        let mut buf = empty_init_buf();
        buf[INIT_OFF_KIND] = KIND_PASSIVE;
        buf[INIT_OFF_LIQUIDITY..INIT_OFF_LIQUIDITY + 16].copy_from_slice(&1u128.to_le_bytes());
        assert!(parse_init_payload(&buf).is_err());
    }

    #[test]
    fn init_rejects_vamm_with_zero_liquidity() {
        let mut buf = empty_init_buf();
        buf[INIT_OFF_KIND] = KIND_VAMM;
        // liquidity is zero by default; should be rejected for vAMM
        assert!(parse_init_payload(&buf).is_err());
    }

    // ---- parse_init_payload — allowlist ----

    #[test]
    fn init_rejects_count_above_max() {
        let mut buf = empty_init_buf();
        buf[INIT_OFF_COUNT] = 5;
        assert!(parse_init_payload(&buf).is_err());
    }

    #[test]
    fn init_rejects_unused_slot_nonzero() {
        let mut buf = empty_init_buf();
        buf[INIT_OFF_COUNT] = 1;
        buf[INIT_OFF_ALLOWLIST] = 0xaa;
        buf[INIT_OFF_ALLOWLIST + ALLOWLIST_SLOT] = 0x01; // slot 1 must be zero
        assert!(parse_init_payload(&buf).is_err());
    }

    #[test]
    fn init_accepts_full_allowlist() {
        let mut buf = empty_init_buf();
        buf[INIT_OFF_COUNT] = 4;
        for i in 0..4 {
            let off = INIT_OFF_ALLOWLIST + i * ALLOWLIST_SLOT;
            buf[off..off + ALLOWLIST_SLOT].fill((i + 1) as u8);
        }
        let p = parse_init_payload(&buf).unwrap();
        assert_eq!(p.allowlist_count, 4);
        for i in 0..4 {
            assert_eq!(p.allowlist[i], [(i + 1) as u8; 32]);
        }
    }

    // ---- parse_match_call ----

    #[test]
    fn match_minimal_zero_payload_parses() {
        let buf = empty_match_buf();
        let c = parse_match_call(&buf).unwrap();
        assert_eq!(c.req_id, 0);
        assert_eq!(c.req_size, 0);
    }

    #[test]
    fn match_rejects_short() {
        assert!(parse_match_call(&vec![0u8; MATCH_LEN - 1]).is_err());
    }

    #[test]
    fn match_accepts_longer_payload() {
        let mut buf = vec![0u8; MATCH_LEN + 32];
        buf[0] = TAG_MATCH;
        // bytes [43..67] still zero — extra bytes after MATCH_LEN are forward-compat slack
        assert!(parse_match_call(&buf).is_ok());
    }

    #[test]
    fn match_rejects_bad_tag() {
        let mut buf = empty_match_buf();
        buf[0] = TAG_INIT;
        assert!(parse_match_call(&buf).is_err());
    }

    #[test]
    fn match_rejects_nonzero_reserved() {
        for offset in 0..(MATCH_LEN - MATCH_OFF_RESERVED) {
            let mut buf = empty_match_buf();
            buf[MATCH_OFF_RESERVED + offset] = 1;
            assert!(parse_match_call(&buf).is_err(), "offset {offset}");
        }
    }

    #[test]
    fn match_decodes_negative_size() {
        let mut buf = empty_match_buf();
        let neg: i128 = -42_000_000;
        buf[MATCH_OFF_REQ_SIZE..MATCH_OFF_REQ_SIZE + 16].copy_from_slice(&neg.to_le_bytes());
        let c = parse_match_call(&buf).unwrap();
        assert_eq!(c.req_size, neg);
    }

    #[test]
    fn match_decodes_all_fields() {
        let mut buf = empty_match_buf();
        buf[MATCH_OFF_REQ_ID..MATCH_OFF_REQ_ID + 8]
            .copy_from_slice(&0xdead_beef_cafe_babeu64.to_le_bytes());
        buf[MATCH_OFF_LP_IDX..MATCH_OFF_LP_IDX + 2].copy_from_slice(&7u16.to_le_bytes());
        buf[MATCH_OFF_LP_ACCT..MATCH_OFF_LP_ACCT + 8]
            .copy_from_slice(&0x1122_3344_5566_7788u64.to_le_bytes());
        buf[MATCH_OFF_ORACLE..MATCH_OFF_ORACLE + 8].copy_from_slice(&100_000_000u64.to_le_bytes());
        buf[MATCH_OFF_REQ_SIZE..MATCH_OFF_REQ_SIZE + 16]
            .copy_from_slice(&1_000_000i128.to_le_bytes());
        let c = parse_match_call(&buf).unwrap();
        assert_eq!(c.req_id, 0xdead_beef_cafe_babe);
        assert_eq!(c.lp_idx, 7);
        assert_eq!(c.lp_account_id, 0x1122_3344_5566_7788);
        assert_eq!(c.oracle_price_e6, 100_000_000);
        assert_eq!(c.req_size, 1_000_000);
    }

    // ---- MatchReturn ----

    #[test]
    fn rejected_shape() {
        let r = MatchReturn::rejected(7, 11, 100_000_000);
        assert_eq!(r.flags, FLAG_VALID | FLAG_REJECTED);
        assert_eq!(r.exec_price_e6, REJECTED_EXEC_PRICE);
        assert_eq!(r.exec_size, 0);
        assert_eq!(r.req_id, 7);
        assert_eq!(r.lp_account_id, 11);
        assert_eq!(r.oracle_price_e6, 100_000_000);
    }

    #[test]
    fn zero_fill_shape() {
        let r = MatchReturn::zero_fill(7, 11, 100_000_000);
        assert_eq!(r.flags, FLAG_VALID | FLAG_PARTIAL_OK);
        assert_eq!(r.exec_price_e6, 100_000_000); // exec_price = oracle, not 1
        assert_eq!(r.exec_size, 0);
    }

    #[test]
    fn write_to_layout_correct() {
        let r = MatchReturn::rejected(0xAABB, 0xCCDD, 0xEEFF);
        let mut buf = [0u8; 64];
        r.write_to(&mut buf).unwrap();
        assert_eq!(u32::from_le_bytes(buf[0..4].try_into().unwrap()), ABI_VERSION);
        assert_eq!(
            u32::from_le_bytes(buf[4..8].try_into().unwrap()),
            FLAG_VALID | FLAG_REJECTED,
        );
        assert_eq!(
            u64::from_le_bytes(buf[8..16].try_into().unwrap()),
            REJECTED_EXEC_PRICE,
        );
        assert_eq!(i128::from_le_bytes(buf[16..32].try_into().unwrap()), 0);
        assert_eq!(u64::from_le_bytes(buf[32..40].try_into().unwrap()), 0xAABB);
        assert_eq!(u64::from_le_bytes(buf[40..48].try_into().unwrap()), 0xCCDD);
        assert_eq!(u64::from_le_bytes(buf[48..56].try_into().unwrap()), 0xEEFF);
        assert_eq!(u64::from_le_bytes(buf[56..64].try_into().unwrap()), 0);
    }

    #[test]
    fn write_to_rejects_short_buffer() {
        let r = MatchReturn::rejected(0, 0, 0);
        let mut buf = [0u8; 63];
        assert!(r.write_to(&mut buf).is_err());
    }

    // ---- MatcherState read/write round-trip ----

    #[test]
    fn matcher_state_roundtrip() {
        let mut buf = [0u8; MATCHER_CTX_SIZE];
        let s = MatcherState {
            magic: BALLAST_MAGIC,
            version: BALLAST_VERSION,
            kind: KIND_VAMM,
            allow_trade_cpi_fills: ALLOW_FILLS_FILL,
            lp_pda: [0xab; 32],
            trading_fee_bps: 10,
            base_spread_bps: 40,
            max_total_bps: 500,
            impact_k_bps: 25,
            liquidity_notional_e6: 1_000_000_000,
            max_fill_abs: 5_000_000,
            inventory_base: -42_000,
            last_oracle_price_e6: 100_000_000,
            last_exec_price_e6: 100_500_000,
            max_inventory_abs: 1_000_000,
        };
        s.write_to(&mut buf).unwrap();
        let r = MatcherState::read_from(&buf).unwrap();
        assert_eq!(r.magic, s.magic);
        assert_eq!(r.kind, s.kind);
        assert_eq!(r.allow_trade_cpi_fills, s.allow_trade_cpi_fills);
        assert_eq!(r.lp_pda, s.lp_pda);
        assert_eq!(r.trading_fee_bps, s.trading_fee_bps);
        assert_eq!(r.base_spread_bps, s.base_spread_bps);
        assert_eq!(r.max_total_bps, s.max_total_bps);
        assert_eq!(r.impact_k_bps, s.impact_k_bps);
        assert_eq!(r.liquidity_notional_e6, s.liquidity_notional_e6);
        assert_eq!(r.max_fill_abs, s.max_fill_abs);
        assert_eq!(r.inventory_base, s.inventory_base);
        assert_eq!(r.last_oracle_price_e6, s.last_oracle_price_e6);
        assert_eq!(r.last_exec_price_e6, s.last_exec_price_e6);
        assert_eq!(r.max_inventory_abs, s.max_inventory_abs);
    }

    #[test]
    fn matcher_state_rejects_wrong_magic() {
        let buf = [0u8; MATCHER_CTX_SIZE];
        // magic is zero ⇒ uninitialized
        assert!(MatcherState::read_from(&buf).is_err());
    }

    #[test]
    fn state_validate_rejects_max_total_above_ceiling() {
        let mut s = passive_state();
        s.max_total_bps = 9_001;
        assert!(s.validate().is_err());
    }

    #[test]
    fn state_validate_rejects_fee_plus_spread_above_max_total() {
        let mut s = passive_state();
        s.max_total_bps = 100;
        s.trading_fee_bps = 60;
        s.base_spread_bps = 60;
        assert!(s.validate().is_err());
    }

    #[test]
    fn state_validate_rejects_vamm_zero_liquidity() {
        let mut s = passive_state();
        s.kind = KIND_VAMM;
        s.liquidity_notional_e6 = 0;
        assert!(s.validate().is_err());
    }

    #[test]
    fn state_validate_rejects_zero_lp_pda() {
        let mut s = passive_state();
        s.lp_pda = [0u8; 32];
        assert!(s.validate().is_err());
    }

    // ---- compute_passive_fill — pricing ----

    #[test]
    fn passive_buy_marks_up() {
        // 50 bps total (10 fee + 40 spread): exec = 100_000_000 * 10050 / 10000 = 100_500_000
        let mut s = passive_state();
        s.trading_fee_bps = 10;
        s.base_spread_bps = 40;
        s.max_total_bps = 50;
        let (r, size) = compute_passive_fill(&s, &match_call(100_000_000, 1_000)).unwrap();
        assert_eq!(r.flags, FLAG_VALID);
        assert_eq!(r.exec_price_e6, 100_500_000);
        assert_eq!(r.exec_size, 1_000);
        assert_eq!(size, 1_000);
    }

    #[test]
    fn passive_sell_marks_down() {
        let mut s = passive_state();
        s.trading_fee_bps = 10;
        s.base_spread_bps = 40;
        s.max_total_bps = 50;
        let (r, size) = compute_passive_fill(&s, &match_call(100_000_000, -1_000)).unwrap();
        assert_eq!(r.flags, FLAG_VALID);
        assert_eq!(r.exec_price_e6, 99_500_000);
        assert_eq!(r.exec_size, -1_000);
        assert_eq!(size, -1_000);
    }

    #[test]
    fn passive_zero_fee_zero_spread_returns_oracle() {
        let s = passive_state();
        let (r_buy, _) =
            compute_passive_fill(&s, &match_call(123_456_789, 1)).unwrap();
        assert_eq!(r_buy.exec_price_e6, 123_456_789);
        let (r_sell, _) =
            compute_passive_fill(&s, &match_call(123_456_789, -1)).unwrap();
        assert_eq!(r_sell.exec_price_e6, 123_456_789);
    }

    #[test]
    fn passive_at_cap_buy_works() {
        let mut s = passive_state();
        s.trading_fee_bps = 5_000;
        s.base_spread_bps = 4_000;
        s.max_total_bps = 9_000;
        // total_bps = 9000: buy mark-up to oracle * 19000 / 10000 = 1.9× oracle
        let (r, _) = compute_passive_fill(&s, &match_call(100_000_000, 1)).unwrap();
        assert_eq!(r.exec_price_e6, 190_000_000);
    }

    #[test]
    fn passive_at_cap_sell_works() {
        let mut s = passive_state();
        s.trading_fee_bps = 5_000;
        s.base_spread_bps = 4_000;
        s.max_total_bps = 9_000;
        let (r, _) = compute_passive_fill(&s, &match_call(100_000_000, -1)).unwrap();
        assert_eq!(r.exec_price_e6, 10_000_000);
    }

    #[test]
    fn passive_rejects_state_with_max_total_above_ceiling() {
        // Execution-time defense-in-depth: state corrupted past the parser → validate() trips.
        let mut s = passive_state();
        s.max_total_bps = 9_001;
        s.trading_fee_bps = 0;
        s.base_spread_bps = 0;
        assert!(compute_passive_fill(&s, &match_call(100_000_000, 1)).is_err());
    }

    #[test]
    fn passive_rejects_imin_size() {
        let s = passive_state();
        assert!(compute_passive_fill(&s, &match_call(100_000_000, i128::MIN)).is_err());
    }

    #[test]
    fn passive_rejects_zero_oracle() {
        let s = passive_state();
        assert!(compute_passive_fill(&s, &match_call(0, 1)).is_err());
    }

    #[test]
    fn passive_rejects_oracle_overflow() {
        let mut s = passive_state();
        s.trading_fee_bps = 1_000;
        s.max_total_bps = 1_000;
        // u64::MAX * 11000 / 10000 overflows the u128 multiplicand check.
        assert!(compute_passive_fill(&s, &match_call(u64::MAX, 1)).is_err());
    }

    // ---- compute_passive_fill — max_fill_abs cap ----

    #[test]
    fn passive_caps_at_max_fill_abs() {
        let mut s = passive_state();
        s.max_fill_abs = 500;
        // Taker wants 1000; cap clamps to 500. Partial fill returns FLAG_VALID (not PARTIAL_OK).
        let (r, size) = compute_passive_fill(&s, &match_call(100_000_000, 1_000)).unwrap();
        assert_eq!(r.flags, FLAG_VALID);
        assert_eq!(r.exec_size, 500);
        assert_eq!(size, 500);
    }

    #[test]
    fn passive_zero_fill_when_max_fill_zero() {
        let mut s = passive_state();
        s.max_fill_abs = 0;
        // max_fill_abs = 0 ⇒ zero-fill (per upstream convention: 0 means "disabled").
        let (r, size) = compute_passive_fill(&s, &match_call(100_000_000, 1_000)).unwrap();
        assert_eq!(r.flags, FLAG_VALID | FLAG_PARTIAL_OK);
        assert_eq!(r.exec_price_e6, 100_000_000); // oracle, not 1
        assert_eq!(r.exec_size, 0);
        assert_eq!(size, 0);
    }

    // ---- compute_passive_fill — inventory cap ----

    #[test]
    fn passive_inventory_cap_clamps_fill() {
        let mut s = passive_state();
        s.max_inventory_abs = 100;
        s.inventory_base = -50; // room to short 50 more before pinning at -100
        // Taker buys 200 (would push LP to -250); clamp to 50.
        let (r, size) = compute_passive_fill(&s, &match_call(100_000_000, 200)).unwrap();
        assert_eq!(r.flags, FLAG_VALID);
        assert_eq!(r.exec_size, 50);
        assert_eq!(size, 50);
    }

    #[test]
    fn passive_inventory_cap_blocks_when_at_boundary() {
        let mut s = passive_state();
        s.max_inventory_abs = 100;
        s.inventory_base = -100; // already at the short wall
        // Taker buys (would push LP further short); block ⇒ zero-fill.
        let (r, size) = compute_passive_fill(&s, &match_call(100_000_000, 50)).unwrap();
        assert_eq!(r.flags, FLAG_VALID | FLAG_PARTIAL_OK);
        assert_eq!(r.exec_price_e6, 100_000_000);
        assert_eq!(r.exec_size, 0);
        assert_eq!(size, 0);
    }

    #[test]
    fn passive_inventory_cap_allows_risk_reducing_fill() {
        let mut s = passive_state();
        s.max_inventory_abs = 100;
        s.inventory_base = -100; // already at the short wall
        // Taker sells (would push LP from -100 toward 0 — risk-reducing). Full fill.
        let (r, size) = compute_passive_fill(&s, &match_call(100_000_000, -50)).unwrap();
        assert_eq!(r.flags, FLAG_VALID);
        assert_eq!(r.exec_size, -50);
        assert_eq!(size, -50);
    }

    // ---- compute_vamm_fill ----

    #[test]
    fn vamm_buy_adds_impact() {
        let mut s = vamm_state();
        s.trading_fee_bps = 10;
        s.base_spread_bps = 40;
        s.max_total_bps = 1_000; // 10% cap, way above what we expect to need
        s.impact_k_bps = 100;
        s.liquidity_notional_e6 = 100_000_000; // 100e6 notional
        // notional = fill_abs * oracle / 1e6. With fill_abs = 1000 and oracle = 100_000_000
        // (price = $100), abs_notional_e6 = 1000 * 100_000_000 / 1_000_000 = 100_000.
        // impact_bps = 100_000 * 100 / 100_000_000 = 0 (rounds down)
        // To get a meaningful impact: use fill_abs = 1_000_000, oracle = 100_000_000.
        // abs_notional_e6 = 1_000_000 * 100_000_000 / 1_000_000 = 100_000_000.
        // impact_bps = 100_000_000 * 100 / 100_000_000 = 100.
        // total = min(1000, 40 + 10 + 100) = 150. exec = oracle * 10150 / 10000 = 101_500_000.
        let (r, _) = compute_vamm_fill(&s, &match_call(100_000_000, 1_000_000)).unwrap();
        assert_eq!(r.flags, FLAG_VALID);
        assert_eq!(r.exec_price_e6, 101_500_000);
        assert_eq!(r.exec_size, 1_000_000);
    }

    #[test]
    fn vamm_sell_subtracts_impact() {
        let mut s = vamm_state();
        s.trading_fee_bps = 10;
        s.base_spread_bps = 40;
        s.max_total_bps = 1_000;
        s.impact_k_bps = 100;
        s.liquidity_notional_e6 = 100_000_000;
        // Same impact math, sell direction: exec = oracle * 9850 / 10000 = 98_500_000.
        let (r, _) = compute_vamm_fill(&s, &match_call(100_000_000, -1_000_000)).unwrap();
        assert_eq!(r.exec_price_e6, 98_500_000);
        assert_eq!(r.exec_size, -1_000_000);
    }

    #[test]
    fn vamm_bigger_size_more_impact() {
        let mut s = vamm_state();
        s.trading_fee_bps = 0;
        s.base_spread_bps = 0;
        s.max_total_bps = 5_000;
        s.impact_k_bps = 100;
        s.liquidity_notional_e6 = 100_000_000;
        let (small, _) = compute_vamm_fill(&s, &match_call(100_000_000, 500_000)).unwrap();
        let (large, _) = compute_vamm_fill(&s, &match_call(100_000_000, 2_000_000)).unwrap();
        assert!(large.exec_price_e6 > small.exec_price_e6);
    }

    #[test]
    fn vamm_total_capped_at_max_total_bps() {
        let mut s = vamm_state();
        s.trading_fee_bps = 0;
        s.base_spread_bps = 0;
        s.max_total_bps = 100; // 1% cap
        s.impact_k_bps = 10_000; // huge impact coefficient
        s.liquidity_notional_e6 = 1_000_000; // tiny liquidity
        // Without the cap, impact would be enormous. Cap clamps total_bps to 100.
        // exec_price = oracle * 10100 / 10000 = 101_000_000.
        let (r, _) = compute_vamm_fill(&s, &match_call(100_000_000, 1_000_000)).unwrap();
        assert_eq!(r.exec_price_e6, 101_000_000);
    }

    #[test]
    fn vamm_zero_fill_when_max_fill_zero() {
        let mut s = vamm_state();
        s.max_fill_abs = 0;
        let (r, size) = compute_vamm_fill(&s, &match_call(100_000_000, 1_000)).unwrap();
        assert_eq!(r.flags, FLAG_VALID | FLAG_PARTIAL_OK);
        assert_eq!(r.exec_size, 0);
        assert_eq!(size, 0);
    }

    // ---- check_inventory_limit (direct) ----

    #[test]
    fn inventory_uncapped_when_max_zero() {
        assert_eq!(check_inventory_limit(0, 0, 1_000_000, true), 1_000_000);
        assert_eq!(check_inventory_limit(0, 0, 1_000_000, false), 1_000_000);
    }

    #[test]
    fn inventory_no_clamp_when_well_within() {
        // LP at 0, cap 100, fill 50 in either direction → full fill.
        assert_eq!(check_inventory_limit(0, 100, 50, true), 50);
        assert_eq!(check_inventory_limit(0, 100, 50, false), 50);
    }

    #[test]
    fn inventory_clamps_buy_to_remaining_room() {
        // LP at +50, cap 100. Taker buys 200 ⇒ LP would go to -150 (past -100 cap). Clamp
        // to make LP_new = -100: max_fill = (50 + 100) = 150.
        assert_eq!(check_inventory_limit(50, 100, 200, true), 150);
    }

    #[test]
    fn inventory_blocks_buy_at_short_wall() {
        // LP already at -100. Any buy would push past wall → 0.
        assert_eq!(check_inventory_limit(-100, 100, 50, true), 0);
        assert_eq!(check_inventory_limit(-101, 100, 50, true), 0);
    }

    #[test]
    fn inventory_blocks_sell_at_long_wall() {
        assert_eq!(check_inventory_limit(100, 100, 50, false), 0);
        assert_eq!(check_inventory_limit(101, 100, 50, false), 0);
    }
}
