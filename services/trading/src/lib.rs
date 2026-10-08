//! Kalks trading engine (see README.md): accounts, orders, positions, margin, swaps, double-entry ledger,
//! dealing desk. Single writer per account shard, event-sourced to PostgreSQL.

pub mod api;
pub mod auth;
pub mod book;
pub mod catalogue;
pub mod config;
pub mod controls;
pub mod corporate;
pub mod engine;
pub mod feed;
pub mod model;
pub mod modules;
pub mod money;
pub mod notify;
pub mod options;
pub mod persist;
pub mod rules;
pub mod shard;
pub mod social;
pub mod specs;
pub mod state;
pub mod tenants;
pub mod views;
