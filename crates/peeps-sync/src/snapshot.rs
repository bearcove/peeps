use peeps_types::SyncSnapshot;

#[cfg(feature = "diagnostics")]
pub fn snapshot_all() -> SyncSnapshot {
    let reg = crate::registry::REGISTRY.lock().unwrap();
    let now = std::time::Instant::now();

    SyncSnapshot {
        mpsc_channels: reg
            .mpsc
            .iter()
            .filter_map(|w| w.upgrade())
            .map(|info| info.snapshot(now))
            .collect(),
        oneshot_channels: reg
            .oneshot
            .iter()
            .filter_map(|w| w.upgrade())
            .map(|info| info.snapshot(now))
            .collect(),
        watch_channels: reg
            .watch
            .iter()
            .filter_map(|w| w.upgrade())
            .map(|info| info.snapshot(now))
            .collect(),
        semaphores: reg
            .semaphore
            .iter()
            .filter_map(|w| w.upgrade())
            .map(|info| info.snapshot(now))
            .collect(),
        once_cells: reg
            .once_cell
            .iter()
            .filter_map(|w| w.upgrade())
            .map(|info| info.snapshot(now))
            .collect(),
    }
}

#[cfg(not(feature = "diagnostics"))]
pub fn snapshot_all() -> SyncSnapshot {
    SyncSnapshot {
        mpsc_channels: Vec::new(),
        oneshot_channels: Vec::new(),
        watch_channels: Vec::new(),
        semaphores: Vec::new(),
        once_cells: Vec::new(),
    }
}
