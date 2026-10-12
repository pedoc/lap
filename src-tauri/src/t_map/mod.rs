//! Global map rendering and geocoding providers. Originals/GPS are never rewritten.
mod batch;
pub(crate) mod batch_work;
mod cancellation;
pub mod commands;
mod config;
mod geo;
mod tiles;
pub use config::Settings;
pub use geo::metadata_location;

pub(crate) mod address_store;
pub use geo::Location;
