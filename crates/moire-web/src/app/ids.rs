use moire_trace_types::JS_SAFE_INT_MAX_U64;
use moire_types::CutId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConnectionId(u64);

impl ConnectionId {
    pub fn new(value: u64) -> Self {
        assert!(
            value > 0 && value <= JS_SAFE_INT_MAX_U64,
            "invariant violated: connection id must be in 1..={JS_SAFE_INT_MAX_U64}, got {value}"
        );
        Self(value)
    }

    pub fn get(self) -> u64 {
        self.0
    }
}

impl core::fmt::Display for ConnectionId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SessionOrdinal(u64);

impl SessionOrdinal {
    pub const ONE: Self = Self(1);

    pub fn get(self) -> u64 {
        self.0
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
        CutId(format!("cut:{}", self.0))
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
