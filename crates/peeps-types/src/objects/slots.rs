use crate::{EdgeKind, EntityBody, EventKind, EventTarget, ScopeBody};

pub trait EntityBodySlot {
    type Value;
    const KIND_NAME: &'static str;

    fn project(body: &EntityBody) -> Option<&Self::Value>;
    fn project_mut(body: &mut EntityBody) -> Option<&mut Self::Value>;
}

pub trait ScopeBodySlot {
    type Value;
    const KIND_NAME: &'static str;

    fn project(body: &ScopeBody) -> Option<&Self::Value>;
    fn project_mut(body: &mut ScopeBody) -> Option<&mut Self::Value>;
}

pub trait EventTargetSlot {
    type Value;
    const KIND_NAME: &'static str;

    fn project(target: &EventTarget) -> Option<&Self::Value>;
    fn project_mut(target: &mut EventTarget) -> Option<&mut Self::Value>;
}

pub trait EventKindSlot {
    const KIND: EventKind;
    const KIND_NAME: &'static str;
}

pub trait EdgeKindSlot {
    const KIND: EdgeKind;
    const KIND_NAME: &'static str;
}
