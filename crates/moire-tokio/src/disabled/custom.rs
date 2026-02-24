pub use moire_types::{CustomEntity, CustomEventKind, EntityBody, EventTarget, Json};

/// No-op entity handle for custom entities when diagnostics are disabled.
#[derive(Clone)]
pub struct CustomEntityHandle;

impl CustomEntityHandle {
    pub fn new(_name: impl Into<String>, _body: CustomEntity) -> Self {
        Self
    }

    pub fn mutate(&self, _f: impl FnOnce(&mut CustomEntity)) -> bool {
        false
    }

    pub fn emit_event(
        &self,
        _kind: impl Into<String>,
        _display_name: impl Into<String>,
        _payload: Json,
    ) {
    }
}

pub fn record_custom_event(
    _target: EventTarget,
    _kind: impl Into<String>,
    _display_name: impl Into<String>,
    _payload: Json,
) {
}
