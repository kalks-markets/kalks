//! Kalks Circle (127.0.0.1:8105): the trader community — profiles, follows, posts, stories, chat, rooms, video,
//! feeds and discovery, safety and moderation, notifications and push, AI helpers, wired into trading, copy
//! trading, the Academy and Rewards. One shared community across brokers; the broker is never shown.
//! Spec: docs/social/KALKS-CIRCLE.md. API: docs/CIRCLE-API.md.

pub mod ai;
pub mod api;
pub mod audit;
pub mod chat;
pub mod config;
pub mod db;
pub mod error;
pub mod feeds;
pub mod gamify;
pub mod imaging;
pub mod media;
pub mod moderation;
pub mod modules;
pub mod notify;
pub mod posts;
pub mod profiles;
pub mod push;
pub mod rewards;
pub mod state;
pub mod stats;
pub mod storage;
pub mod stories;
pub mod text;
pub mod tradecards;
pub mod upstream;
pub mod util;
pub mod video;
pub mod workers;
