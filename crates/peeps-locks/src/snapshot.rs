#[cfg(feature = "diagnostics")]
pub fn snapshot_lock_diagnostics() -> crate::LockSnapshot {
    use std::sync::atomic::Ordering;

    use crate::registry::{AcquireKind, LOCK_REGISTRY};

    let Ok(registry) = LOCK_REGISTRY.lock() else {
        return crate::LockSnapshot { locks: Vec::new() };
    };

    let mut locks = Vec::new();
    for weak in registry.iter() {
        let Some(info) = weak.upgrade() else {
            continue;
        };

        let (Ok(waiters), Ok(holders)) = (info.waiters.lock(), info.holders.lock()) else {
            continue;
        };

        let holder_snapshots: Vec<crate::LockHolderSnapshot> = holders
            .iter()
            .map(|h| {
                let bt = format!("{}", h.backtrace);
                crate::LockHolderSnapshot {
                    kind: match h.kind {
                        AcquireKind::Read => crate::LockAcquireKind::Read,
                        AcquireKind::Write => crate::LockAcquireKind::Write,
                        AcquireKind::Mutex => crate::LockAcquireKind::Mutex,
                    },
                    held_secs: h.since.elapsed().as_secs_f64(),
                    backtrace: if bt.is_empty() { None } else { Some(bt) },
                    task_id: h.peeps_task_id,
                    task_name: h.peeps_task_id.and_then(peeps_tasks::task_name),
                }
            })
            .collect();

        let waiter_snapshots: Vec<crate::LockWaiterSnapshot> = waiters
            .iter()
            .map(|w| {
                let bt = format!("{}", w.backtrace);
                crate::LockWaiterSnapshot {
                    kind: match w.kind {
                        AcquireKind::Read => crate::LockAcquireKind::Read,
                        AcquireKind::Write => crate::LockAcquireKind::Write,
                        AcquireKind::Mutex => crate::LockAcquireKind::Mutex,
                    },
                    waiting_secs: w.since.elapsed().as_secs_f64(),
                    backtrace: if bt.is_empty() { None } else { Some(bt) },
                    task_id: w.peeps_task_id,
                    task_name: w.peeps_task_id.and_then(peeps_tasks::task_name),
                }
            })
            .collect();

        locks.push(crate::LockInfoSnapshot {
            name: info.name.to_string(),
            acquires: info.total_acquires.load(Ordering::SeqCst),
            releases: info.total_releases.load(Ordering::SeqCst),
            holders: holder_snapshots,
            waiters: waiter_snapshots,
        });
    }

    crate::LockSnapshot { locks }
}

#[cfg(not(feature = "diagnostics"))]
#[inline]
pub fn snapshot_lock_diagnostics() -> crate::LockSnapshot {
    crate::LockSnapshot { locks: Vec::new() }
}

#[cfg(not(feature = "diagnostics"))]
#[inline]
pub fn dump_lock_diagnostics() -> String {
    String::new()
}
