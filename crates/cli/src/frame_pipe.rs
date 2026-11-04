//! The shared frame pipe's broker side: its private endpoint, and the
//! service that owns capture and authority for one controller and helper.
pub(crate) mod auth;
pub(crate) mod broker;
pub(crate) mod client;
pub(crate) mod endpoint;
pub(crate) mod service;
pub(crate) mod subscription;
