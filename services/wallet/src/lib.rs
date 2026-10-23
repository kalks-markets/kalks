//! Kalks wallet service (see README.md): central client wallet on a double-entry ledger, on-chain USDT
//! deposits and withdrawals (BNB Chain BEP20, TRON TRC20), NOWPayments crypto deposits, wallet <-> trading account
//! transfers.

pub mod api;
pub mod audit;
pub mod chain;
pub mod config;
pub mod db;
pub mod engine;
pub mod error;
pub mod ib;
pub mod ledger;
pub mod money;
pub mod notifier;
pub mod nowpayments;
pub mod ops;
pub mod secrets;
pub mod settings;
pub mod state;
pub mod users;
pub mod watcher;
