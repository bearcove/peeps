//! RPC request/response hooks for external wrapper crates.
//!
//! This module provides stable, minimal hooks for wrappers (for example, roam
//! transport/client/server layers) to register request/response entities in the
//! peeps runtime graph and wire parent causality.
//!
//! # Intended Wrapper Usage
//!
//! ```no_run
//! use peeps::rpc::RpcEvent;
//! use peeps::types::canonical_id;
//!
//! let span_id = "01J9Z8QK9D4HT9EXAMPLE12345";
//! let request_id = canonical_id::request_from_span_id(span_id);
//! let response_id = format!("response:{span_id}");
//!
//! peeps::rpc_request_event!(&request_id, "GetUser", {
//!     "rpc.connection" => "conn_42",
//!     "method" => "GetUser",
//!     "correlation" => span_id,
//! });
//!
//! // Responder side: explicitly attach to caller request when metadata provides it.
//! peeps::rpc_response_event!(&response_id, "GetUser", parent = &request_id, {
//!     "rpc.peer" => "users-service",
//! });
//!
//! // Optional raw attrs_json path if the wrapper already built structured attrs.
//! let attrs_json = r#"{"method":"GetUser","source":"/srv/api.rs:42","rpc.status":"ok"}"#;
//! peeps::rpc::record_response(RpcEvent {
//!     entity_id: &response_id,
//!     name: "GetUser",
//!     attrs_json,
//!     parent_entity_id: Some(&request_id),
//! });
//! ```
//!
//! When `diagnostics` is disabled, all hooks compile to no-ops. The macros are
//! preferred because they also skip metadata construction on disabled builds.

#[cfg(not(feature = "diagnostics"))]
mod disabled;
#[cfg(feature = "diagnostics")]
mod enabled;

/// Required fields for recording a request/response entity.
#[derive(Debug, Clone, Copy)]
pub struct RpcEvent<'a> {
    /// Stable canonical node id (for example `request:{span_id}` or `response:{span_id}`).
    pub entity_id: &'a str,
    /// Human-readable RPC name (method/endpoint).
    pub name: &'a str,
    /// Full JSON attributes object for this entity.
    ///
    /// Canonical-only convention: emit `created_at` + `source`, and optional
    /// `method`/`correlation` for shared inspector fields.
    pub attrs_json: &'a str,
    /// Optional explicit parent entity id.
    ///
    /// If omitted, peeps uses the current stack top (if any).
    pub parent_entity_id: Option<&'a str>,
}

#[cfg(not(feature = "diagnostics"))]
#[doc(hidden)]
pub use disabled::{
    __peeps_track_rpc_request_with_meta, __peeps_track_rpc_response_with_meta, record_request,
    record_response,
};
#[cfg(feature = "diagnostics")]
#[doc(hidden)]
pub use enabled::{
    __peeps_track_rpc_request_with_meta, __peeps_track_rpc_response_with_meta, record_request,
    record_response,
};
