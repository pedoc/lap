//! Standard model services are independent of the existing ONNX task contracts.
pub mod client;
pub mod commands;
pub mod config;
pub mod requests;
pub mod store;
#[cfg(test)]
mod tests;
mod vendors;
pub mod visual;
pub mod workflow;
