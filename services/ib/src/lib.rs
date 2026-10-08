//! Kalks IB / referral programme (Round 3, D53–D64).
//!
//! Every client is an IB from sign-up (level 1). The service mirrors the referral tree from the gateway,
//! consumes closed live deals from the trading engine, computes multi-tier per-lot commissions with rebates
//! and sub-IB splits, CPA bonuses, monthly level upgrades, campaign click funnels, fraud flags, and pays
//! approved batches into client wallets through the wallet service. See README.md.

pub mod api;
pub mod audit;
pub mod calc;
pub mod clients;
pub mod config;
pub mod db;
pub mod deals;
pub mod error;
pub mod model;
pub mod modules;
pub mod money;
pub mod notifier;
pub mod payouts;
pub mod state;
pub mod stats;
pub mod sync;
pub mod workers;
