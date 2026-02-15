#[inline]
pub fn set_process_info(_process_name: impl Into<String>, _proc_key: impl Into<String>) {
    // No-op when diagnostics disabled
}

#[inline]
pub fn snapshot_lock_diagnostics() -> crate::LockSnapshot {
    crate::LockSnapshot { locks: Vec::new() }
}

#[inline]
pub fn dump_lock_diagnostics() -> String {
    String::new()
}

#[inline(always)]
pub fn emit_lock_graph() -> peeps_types::GraphSnapshot {
    peeps_types::GraphSnapshot::empty()
}
