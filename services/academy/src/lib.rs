//! Kalks Academy service (:8098): versioned course content (core phases 1-8 x fundamental / technical tracks,
//! product electives such as phase 9 Kalks FX Options with a single `options` track, exams, glossary) seeded
//! from `content/academy`, per-tenant CMS overrides, learner progress, quizzes, exams and
//! verifiable phase certificates. Internal only: the Client Area and Back Office BFFs call it with
//! `X-Kalks-Internal`; API contract in `api.rs`.

pub mod api;
pub mod cert;
pub mod config;
pub mod content;
pub mod modules;
pub mod store;
