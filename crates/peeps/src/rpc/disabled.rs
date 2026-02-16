use peeps_types::MetaBuilder;

use super::RpcEvent;

#[inline(always)]
pub fn record_request(_event: RpcEvent<'_>) {}

#[inline(always)]
pub fn record_response(_event: RpcEvent<'_>) {}

#[inline(always)]
#[doc(hidden)]
pub fn __peeps_track_rpc_request_with_meta(
    _entity_id: &str,
    _name: &str,
    _meta: MetaBuilder<'_>,
    _parent_entity_id: Option<&str>,
) {
}

#[inline(always)]
#[doc(hidden)]
pub fn __peeps_track_rpc_response_with_meta(
    _entity_id: &str,
    _name: &str,
    _meta: MetaBuilder<'_>,
    _parent_entity_id: Option<&str>,
) {
}
