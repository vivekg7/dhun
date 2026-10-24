//! Everything in the Linux app that needs no display: the API client, the
//! working copy, the library pull, the catalogue and search
//! (docs/plans/039_linux_app.md). The Mac's DhunKit and the phone's `data`
//! package are the specification; where this is silent, they decide.

pub mod api;
pub mod base64;
pub mod cache;
pub mod catalog;
pub mod covers;
pub mod db;
pub mod downloads;
pub mod lyrics;
pub mod model;
pub mod prefs;
pub mod queue;
pub mod search;
pub mod spoken;
pub mod store;
pub mod stream;
pub mod sync;
pub mod synced;
pub mod tempo;
pub mod time;
