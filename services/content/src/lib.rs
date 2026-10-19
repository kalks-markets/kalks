//! Kalks content service (:8105): the media library (images with an optional dark-theme file, resized WebP
//! variants, alt text per locale) and editable content slots for the website, the Client Area ("app"), Kalks
//! Trader and emails, with draft → publish versions, rollback and English fallback. The public read
//! (`/v1/public/content`, `/v1/public/media/*`) needs no token; everything else is internal (`X-Kalks-Internal`)
//! and called by the Back Office BFF. API contract in `api.rs`; docs/CONTENT.md has the slot catalogue.

pub mod api;
pub mod config;
pub mod content;
pub mod media;
pub mod preview;
pub mod storage;
pub mod store;
