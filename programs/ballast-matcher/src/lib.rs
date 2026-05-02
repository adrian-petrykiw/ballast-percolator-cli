//! Ballast allowlist matcher — Percolator-compatible matcher program (devnet POC).
//!
//! Implements the allowlist gate described in PRD §4.7 (FM-1..FM-6):
//! - LP PDA must be a signer (Percolator security invariant).
//! - Counterparty wallet must be in the on-chain allowlist (matcher context).
//! - Passive pricing mode (oracle ± fixed spread) is sufficient for Phase 0/1.
//!
//! Scaffold only — implementation lands in PRD Phase 0 Step 0.4.
//! See docs/prd.md and the upstream `percolator-match` reference repo.

#![allow(unexpected_cfgs)]
