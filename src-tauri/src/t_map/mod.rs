//! Global map rendering and geocoding providers. Originals/GPS are never rewritten.
mod cancellation;
pub mod commands;
mod config;
mod geo;
mod tiles;
pub use config::Settings;
pub use geo::metadata_location;
