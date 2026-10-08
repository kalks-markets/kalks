//! Kalks growth: rewards (loyalty points, tiers, catalogue, cashback, contests) and marketing (bonus
//! campaigns with per-lot release, promo codes, targeted banners, share P&L cards). D29, D121, D135, D136, D144.
//! Reads closed deals from the trading engine, posts bonus / credit through the engine's admin account API and
//! pays cash rewards through the wallet service. See README.md.

pub mod api;
pub mod audit;
pub mod bonus;
pub mod calc;
pub mod cashback;
pub mod clients;
pub mod config;
pub mod contests;
pub mod db;
pub mod deals;
pub mod error;
pub mod journeys;
pub mod loyalty;
pub mod model;
pub mod modules;
pub mod money;
pub mod payouts;
pub mod profiles;
pub mod promos;
pub mod shares;
pub mod state;
pub mod workers;
