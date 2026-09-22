extern crate self as netbox_rs;

pub mod api;
pub mod implementation;
pub mod transport;

pub use netbox_rs_macros::{netbox_api, netbox_impl};
