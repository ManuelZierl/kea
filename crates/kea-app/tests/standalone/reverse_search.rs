//! Dependency-free harness for the same model/store/worker unit tests Cargo runs.
#![allow(dead_code)]

#[path = "../../src/reverse_search/model.rs"]
mod model;
#[path = "../../src/reverse_search/store.rs"]
pub(crate) mod store;
#[path = "../../src/reverse_search/worker.rs"]
mod worker;

// Match the library namespace without importing the GPUI surface.
mod reverse_search {
    pub use super::model::*;
    pub(crate) use super::store;
}
