//! Roam diagnostics integration for peeps.
//!
//! Re-exports diagnostic snapshot types from roam-session and roam-shm.

#[cfg(feature = "roam-session")]
pub use roam_session::diagnostic_snapshot::*;

#[cfg(feature = "roam-shm")]
pub use roam_shm::diagnostic_snapshot::*;

/// Collect roam session diagnostics if the roam-session feature is enabled.
#[cfg(feature = "roam-session")]
pub fn snapshot_session() -> roam_session::diagnostic_snapshot::DiagnosticSnapshot {
    roam_session::diagnostic_snapshot::snapshot_all_diagnostics()
}

/// Collect roam SHM diagnostics if the roam-shm feature is enabled.
#[cfg(feature = "roam-shm")]
pub fn snapshot_shm() -> roam_shm::diagnostic_snapshot::ShmSnapshot {
    roam_shm::diagnostic_snapshot::snapshot_all_shm()
}
