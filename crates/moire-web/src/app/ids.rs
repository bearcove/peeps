use moire_trace_types::JS_SAFE_INT_MAX_U64;
pub use moire_types::ConnectionId;
use moire_types::CutId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SessionOrdinal(u64);

impl SessionOrdinal {
    pub const ONE: Self = Self(1);

    pub fn to_session_id(self) -> moire_types::SessionId {
        moire_types::SessionId::from_ordinal(self.0)
    }

    pub fn next(self) -> Self {
        let next = self.0.saturating_add(1);
        assert!(
            next > 0 && next <= JS_SAFE_INT_MAX_U64,
            "invariant violated: session ordinal must be in 1..={JS_SAFE_INT_MAX_U64}, got {next}"
        );
        Self(next)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CutOrdinal(u64);

impl CutOrdinal {
    pub const ONE: Self = Self(1);

    pub fn to_cut_id(self) -> CutId {
        CutId::from_ordinal(self.0)
    }

    pub fn next(self) -> Self {
        let next = self.0.saturating_add(1);
        assert!(
            next > 0 && next <= JS_SAFE_INT_MAX_U64,
            "invariant violated: cut ordinal must be in 1..={JS_SAFE_INT_MAX_U64}, got {next}"
        );
        Self(next)
    }
}
