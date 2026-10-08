//! Kalks ALGO service (see README.md): strategies (visual spec + DSL), backtester, 24/7 strategy runtime,
//! webhook signals, public API keys, marketplace and the AI strategy assistant.

pub mod ai;
pub mod api;
pub mod backtest;
pub mod clients;
pub mod config;
pub mod db;
pub mod dsl;
pub mod error;
pub mod house;
pub mod indicators;
pub mod modules;
pub mod runtime;
pub mod security;
pub mod spec;
pub mod specs;
pub mod state;
pub mod strategy;
