//! Diagnostic wrappers for locks (both sync and async).
//!
//! When the `diagnostics` feature is **disabled**, these are zero-cost wrappers
//! that compile down to plain locks. When **enabled**, every lock registers
//! itself in a global registry and tracks:
//!
//! - Who is waiting to acquire the lock (with backtrace + duration)
//! - Who currently holds the lock (with backtrace + duration)
//!
//! Call [`dump_lock_diagnostics()`] (e.g. from a SIGUSR1 handler) to get a
//! human-readable snapshot of all contention.
//!
//! ## Sync locks (`DiagnosticRwLock`, `DiagnosticMutex`)
//! Wrappers around `parking_lot::RwLock` and `parking_lot::Mutex`.
//!
//! ## Async locks (`DiagnosticAsyncRwLock`, `DiagnosticAsyncMutex`)
//! Wrappers around `tokio::sync::RwLock` and `tokio::sync::Mutex`.
//! Uses a `WaiterToken` to handle cancellation — if a `.await` is dropped,
//! the waiter entry is automatically cleaned up.

#[cfg(feature = "diagnostics")]
mod inner {
    use std::backtrace::Backtrace;
    use std::ops::{Deref, DerefMut};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, LazyLock, Mutex as StdMutex, Weak};
    use std::time::Instant;

    // ── Global registry ──────────────────────────────────────────

    static LOCK_REGISTRY: LazyLock<StdMutex<Vec<Weak<LockInfo>>>> =
        LazyLock::new(|| StdMutex::new(Vec::new()));

    /// Snapshot all tracked locks and return a human-readable report.
    pub fn dump_lock_diagnostics() -> String {
        let Ok(registry) = LOCK_REGISTRY.lock() else {
            return "(lock registry poisoned)\n".to_string();
        };
        let mut out = String::new();

        for weak in registry.iter() {
            let Some(info) = weak.upgrade() else {
                continue;
            };

            let (Ok(waiters), Ok(holders)) = (info.waiters.lock(), info.holders.lock()) else {
                out.push_str(&format!(
                    "Lock \"{}\": (diagnostics mutex poisoned)\n",
                    info.name
                ));
                continue;
            };

            // Skip quiescent locks
            if waiters.is_empty() && holders.is_empty() {
                continue;
            }

            let acquires = info.total_acquires.load(Ordering::SeqCst);
            let releases = info.total_releases.load(Ordering::SeqCst);
            let held_count = holders.len() as u64;
            let balanced = acquires == releases + held_count;
            out.push_str(&format!(
                "Lock \"{}\" (acquires={}, releases={}, holding={}{}):\n",
                info.name,
                acquires,
                releases,
                held_count,
                if balanced { "" } else { " *** MISMATCH ***" }
            ));

            if holders.is_empty() {
                out.push_str("  Holders: (none)\n");
            } else {
                out.push_str(&format!("  Holders ({}):\n", holders.len()));
                for h in holders.iter() {
                    let elapsed = h.since.elapsed();
                    let task_info = format_task_info(h.task_id, h.task_name);
                    out.push_str(&format!(
                        "    [{}]{} held for {:.3}s\n",
                        h.kind,
                        task_info,
                        elapsed.as_secs_f64()
                    ));
                    format_backtrace(&h.backtrace, &mut out);
                }
            }

            if waiters.is_empty() {
                out.push_str("  Waiters: (none)\n");
            } else {
                out.push_str(&format!("  Waiters ({}):\n", waiters.len()));
                for w in waiters.iter() {
                    let elapsed = w.since.elapsed();
                    let task_info = format_task_info(w.task_id, w.task_name);
                    out.push_str(&format!(
                        "    [{}]{} waiting for {:.3}s\n",
                        w.kind,
                        task_info,
                        elapsed.as_secs_f64()
                    ));
                    format_backtrace(&w.backtrace, &mut out);
                }
            }
        }

        out
    }

    /// Snapshot all tracked locks and return structured data.
    pub fn snapshot_lock_diagnostics() -> crate::LockSnapshot {
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

    fn format_task_info(task_id: Option<tokio::task::Id>, task_name: Option<&str>) -> String {
        match (task_id, task_name) {
            (Some(id), Some(name)) => format!(" task={id}({name})"),
            (Some(id), None) => format!(" task={id}"),
            _ => String::new(),
        }
    }

    fn format_backtrace(bt: &Backtrace, out: &mut String) {
        let text = format!("{bt}");
        let mut shown = 0;
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.contains("vx_runner")
                || trimmed.contains("vx_vfs")
                || trimmed.contains("fuse3")
                || trimmed.contains("cas_vfs")
                || trimmed.contains("diagnostic_lock")
            {
                out.push_str(&format!("      {trimmed}\n"));
                shown += 1;
                if shown >= 8 {
                    break;
                }
            }
        }
        if shown == 0 {
            for line in text.lines().take(6) {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    out.push_str(&format!("      {trimmed}\n"));
                }
            }
        }
    }

    // ── Shared types ─────────────────────────────────────────────

    #[derive(Debug, Clone, Copy)]
    enum AcquireKind {
        Read,
        Write,
        Mutex,
    }

    impl std::fmt::Display for AcquireKind {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                AcquireKind::Read => write!(f, "read"),
                AcquireKind::Write => write!(f, "write"),
                AcquireKind::Mutex => write!(f, "mutex"),
            }
        }
    }

    struct WaiterOrHolder {
        id: u64,
        kind: AcquireKind,
        since: Instant,
        backtrace: Backtrace,
        task_id: Option<tokio::task::Id>,
        task_name: Option<&'static str>,
    }

    struct LockInfo {
        name: &'static str,
        next_id: AtomicU64,
        waiters: StdMutex<Vec<WaiterOrHolder>>,
        holders: StdMutex<Vec<WaiterOrHolder>>,
        total_acquires: AtomicU64,
        total_releases: AtomicU64,
    }

    impl LockInfo {
        fn new(name: &'static str) -> Arc<Self> {
            let info = Arc::new(Self {
                name,
                next_id: AtomicU64::new(0),
                waiters: StdMutex::new(Vec::new()),
                holders: StdMutex::new(Vec::new()),
                total_acquires: AtomicU64::new(0),
                total_releases: AtomicU64::new(0),
            });
            LOCK_REGISTRY.lock().unwrap().push(Arc::downgrade(&info));
            info
        }

        fn capture_task_info() -> (Option<tokio::task::Id>, Option<&'static str>) {
            (tokio::task::try_id(), None)
        }

        fn add_waiter(&self, kind: AcquireKind) -> u64 {
            let id = self.next_id.fetch_add(1, Ordering::Relaxed);
            let (task_id, task_name) = Self::capture_task_info();
            self.waiters.lock().unwrap().push(WaiterOrHolder {
                id,
                kind,
                since: Instant::now(),
                backtrace: Backtrace::force_capture(),
                task_id,
                task_name,
            });
            id
        }

        fn remove_waiter(&self, id: u64) {
            self.waiters.lock().unwrap().retain(|w| w.id != id);
        }

        fn remove_holder(&self, id: u64) {
            self.holders.lock().unwrap().retain(|h| h.id != id);
            self.total_releases.fetch_add(1, Ordering::Relaxed);
        }

        fn promote_waiter_to_holder(&self, waiter_id: u64, kind: AcquireKind) -> u64 {
            let holder_id = self.next_id.fetch_add(1, Ordering::Relaxed);
            let (task_id, task_name) = Self::capture_task_info();

            let mut waiters = self.waiters.lock().unwrap();
            let mut holders = self.holders.lock().unwrap();

            waiters.retain(|w| w.id != waiter_id);
            holders.push(WaiterOrHolder {
                id: holder_id,
                kind,
                since: Instant::now(),
                backtrace: Backtrace::force_capture(),
                task_id,
                task_name,
            });

            self.total_acquires.fetch_add(1, Ordering::Relaxed);
            holder_id
        }
    }

    // ── DiagnosticRwLock ─────────────────────────────────────────

    pub struct DiagnosticRwLock<T> {
        inner: parking_lot::RwLock<T>,
        info: Arc<LockInfo>,
    }

    impl<T> DiagnosticRwLock<T> {
        pub fn new(name: &'static str, value: T) -> Self {
            Self {
                inner: parking_lot::RwLock::new(value),
                info: LockInfo::new(name),
            }
        }

        pub fn read(&self) -> DiagnosticRwLockReadGuard<'_, T> {
            let waiter_id = self.info.add_waiter(AcquireKind::Read);
            let guard = self.inner.read();
            let holder_id = self
                .info
                .promote_waiter_to_holder(waiter_id, AcquireKind::Read);
            DiagnosticRwLockReadGuard {
                guard: std::mem::ManuallyDrop::new(guard),
                info: &self.info,
                holder_id,
            }
        }

        pub fn write(&self) -> DiagnosticRwLockWriteGuard<'_, T> {
            let waiter_id = self.info.add_waiter(AcquireKind::Write);
            let guard = self.inner.write();
            let holder_id = self
                .info
                .promote_waiter_to_holder(waiter_id, AcquireKind::Write);
            DiagnosticRwLockWriteGuard {
                guard: std::mem::ManuallyDrop::new(guard),
                info: &self.info,
                holder_id,
            }
        }
    }

    pub struct DiagnosticRwLockReadGuard<'a, T> {
        guard: std::mem::ManuallyDrop<parking_lot::RwLockReadGuard<'a, T>>,
        info: &'a Arc<LockInfo>,
        holder_id: u64,
    }

    impl<T> Drop for DiagnosticRwLockReadGuard<'_, T> {
        fn drop(&mut self) {
            // Drop the parking_lot guard first (releases the lock),
            // then update bookkeeping.
            unsafe { std::mem::ManuallyDrop::drop(&mut self.guard) };
            self.info.remove_holder(self.holder_id);
        }
    }

    impl<T> Deref for DiagnosticRwLockReadGuard<'_, T> {
        type Target = T;
        fn deref(&self) -> &T {
            &self.guard
        }
    }

    pub struct DiagnosticRwLockWriteGuard<'a, T> {
        guard: std::mem::ManuallyDrop<parking_lot::RwLockWriteGuard<'a, T>>,
        info: &'a Arc<LockInfo>,
        holder_id: u64,
    }

    impl<T> Drop for DiagnosticRwLockWriteGuard<'_, T> {
        fn drop(&mut self) {
            unsafe { std::mem::ManuallyDrop::drop(&mut self.guard) };
            self.info.remove_holder(self.holder_id);
        }
    }

    impl<T> Deref for DiagnosticRwLockWriteGuard<'_, T> {
        type Target = T;
        fn deref(&self) -> &T {
            &self.guard
        }
    }

    impl<T> DerefMut for DiagnosticRwLockWriteGuard<'_, T> {
        fn deref_mut(&mut self) -> &mut T {
            &mut self.guard
        }
    }

    // ── DiagnosticMutex ──────────────────────────────────────────

    pub struct DiagnosticMutex<T> {
        inner: parking_lot::Mutex<T>,
        info: Arc<LockInfo>,
    }

    impl<T> DiagnosticMutex<T> {
        pub fn new(name: &'static str, value: T) -> Self {
            Self {
                inner: parking_lot::Mutex::new(value),
                info: LockInfo::new(name),
            }
        }

        pub fn lock(&self) -> DiagnosticMutexGuard<'_, T> {
            let waiter_id = self.info.add_waiter(AcquireKind::Mutex);
            let guard = self.inner.lock();
            let holder_id = self
                .info
                .promote_waiter_to_holder(waiter_id, AcquireKind::Mutex);
            DiagnosticMutexGuard {
                guard: std::mem::ManuallyDrop::new(guard),
                info: &self.info,
                holder_id,
            }
        }
    }

    pub struct DiagnosticMutexGuard<'a, T> {
        guard: std::mem::ManuallyDrop<parking_lot::MutexGuard<'a, T>>,
        info: &'a Arc<LockInfo>,
        holder_id: u64,
    }

    impl<T> Drop for DiagnosticMutexGuard<'_, T> {
        fn drop(&mut self) {
            unsafe { std::mem::ManuallyDrop::drop(&mut self.guard) };
            self.info.remove_holder(self.holder_id);
        }
    }

    impl<T> Deref for DiagnosticMutexGuard<'_, T> {
        type Target = T;
        fn deref(&self) -> &T {
            &self.guard
        }
    }

    impl<T> DerefMut for DiagnosticMutexGuard<'_, T> {
        fn deref_mut(&mut self) -> &mut T {
            &mut self.guard
        }
    }

    // ── WaiterToken (cancellation-safe waiter cleanup) ───────────

    /// RAII guard that removes a waiter entry from the registry on drop,
    /// unless `disarm()` is called first (meaning the waiter was promoted
    /// to a holder). This handles the case where an async `.await` is
    /// cancelled — the waiter entry gets cleaned up automatically.
    struct WaiterToken {
        info: Arc<LockInfo>,
        id: u64,
        armed: bool,
    }

    impl WaiterToken {
        fn new(info: &Arc<LockInfo>, kind: AcquireKind) -> Self {
            let id = info.add_waiter(kind);
            Self {
                info: Arc::clone(info),
                id,
                armed: true,
            }
        }

        /// Disarm the token — the waiter was successfully promoted to a holder.
        fn disarm(&mut self) {
            self.armed = false;
        }
    }

    impl Drop for WaiterToken {
        fn drop(&mut self) {
            if self.armed {
                self.info.remove_waiter(self.id);
            }
        }
    }

    // ── DiagnosticAsyncRwLock ────────────────────────────────────

    pub struct DiagnosticAsyncRwLock<T> {
        inner: tokio::sync::RwLock<T>,
        info: Arc<LockInfo>,
    }

    impl<T> DiagnosticAsyncRwLock<T> {
        pub fn new(name: &'static str, value: T) -> Self {
            Self {
                inner: tokio::sync::RwLock::new(value),
                info: LockInfo::new(name),
            }
        }

        pub async fn read(&self) -> DiagnosticAsyncRwLockReadGuard<'_, T> {
            let mut token = WaiterToken::new(&self.info, AcquireKind::Read);
            let guard = self.inner.read().await;
            let holder_id = self
                .info
                .promote_waiter_to_holder(token.id, AcquireKind::Read);
            token.disarm();
            DiagnosticAsyncRwLockReadGuard {
                guard: std::mem::ManuallyDrop::new(guard),
                info: Arc::clone(&self.info),
                holder_id,
            }
        }

        pub async fn write(&self) -> DiagnosticAsyncRwLockWriteGuard<'_, T> {
            let mut token = WaiterToken::new(&self.info, AcquireKind::Write);
            let guard = self.inner.write().await;
            let holder_id = self
                .info
                .promote_waiter_to_holder(token.id, AcquireKind::Write);
            token.disarm();
            DiagnosticAsyncRwLockWriteGuard {
                guard: std::mem::ManuallyDrop::new(guard),
                info: Arc::clone(&self.info),
                holder_id,
            }
        }

        pub fn try_read(
            &self,
        ) -> Result<DiagnosticAsyncRwLockReadGuard<'_, T>, tokio::sync::TryLockError> {
            match self.inner.try_read() {
                Ok(guard) => {
                    let holder_id = {
                        let id = self.info.next_id.fetch_add(1, Ordering::Relaxed);
                        let (task_id, task_name) = LockInfo::capture_task_info();
                        let mut holders = self.info.holders.lock().unwrap();
                        holders.push(WaiterOrHolder {
                            id,
                            kind: AcquireKind::Read,
                            since: Instant::now(),
                            backtrace: Backtrace::force_capture(),
                            task_id,
                            task_name,
                        });
                        self.info.total_acquires.fetch_add(1, Ordering::Relaxed);
                        id
                    };
                    Ok(DiagnosticAsyncRwLockReadGuard {
                        guard: std::mem::ManuallyDrop::new(guard),
                        info: Arc::clone(&self.info),
                        holder_id,
                    })
                }
                Err(e) => Err(e),
            }
        }

        pub fn try_write(
            &self,
        ) -> Result<DiagnosticAsyncRwLockWriteGuard<'_, T>, tokio::sync::TryLockError> {
            match self.inner.try_write() {
                Ok(guard) => {
                    let holder_id = {
                        let id = self.info.next_id.fetch_add(1, Ordering::Relaxed);
                        let (task_id, task_name) = LockInfo::capture_task_info();
                        let mut holders = self.info.holders.lock().unwrap();
                        holders.push(WaiterOrHolder {
                            id,
                            kind: AcquireKind::Write,
                            since: Instant::now(),
                            backtrace: Backtrace::force_capture(),
                            task_id,
                            task_name,
                        });
                        self.info.total_acquires.fetch_add(1, Ordering::Relaxed);
                        id
                    };
                    Ok(DiagnosticAsyncRwLockWriteGuard {
                        guard: std::mem::ManuallyDrop::new(guard),
                        info: Arc::clone(&self.info),
                        holder_id,
                    })
                }
                Err(e) => Err(e),
            }
        }
    }

    pub struct DiagnosticAsyncRwLockReadGuard<'a, T> {
        guard: std::mem::ManuallyDrop<tokio::sync::RwLockReadGuard<'a, T>>,
        info: Arc<LockInfo>,
        holder_id: u64,
    }

    impl<T> Drop for DiagnosticAsyncRwLockReadGuard<'_, T> {
        fn drop(&mut self) {
            unsafe { std::mem::ManuallyDrop::drop(&mut self.guard) };
            self.info.remove_holder(self.holder_id);
        }
    }

    impl<T> Deref for DiagnosticAsyncRwLockReadGuard<'_, T> {
        type Target = T;
        fn deref(&self) -> &T {
            &self.guard
        }
    }

    pub struct DiagnosticAsyncRwLockWriteGuard<'a, T> {
        guard: std::mem::ManuallyDrop<tokio::sync::RwLockWriteGuard<'a, T>>,
        info: Arc<LockInfo>,
        holder_id: u64,
    }

    impl<T> Drop for DiagnosticAsyncRwLockWriteGuard<'_, T> {
        fn drop(&mut self) {
            unsafe { std::mem::ManuallyDrop::drop(&mut self.guard) };
            self.info.remove_holder(self.holder_id);
        }
    }

    impl<T> Deref for DiagnosticAsyncRwLockWriteGuard<'_, T> {
        type Target = T;
        fn deref(&self) -> &T {
            &self.guard
        }
    }

    impl<T> DerefMut for DiagnosticAsyncRwLockWriteGuard<'_, T> {
        fn deref_mut(&mut self) -> &mut T {
            &mut self.guard
        }
    }

    // ── DiagnosticAsyncMutex ─────────────────────────────────────

    pub struct DiagnosticAsyncMutex<T> {
        inner: tokio::sync::Mutex<T>,
        info: Arc<LockInfo>,
    }

    impl<T> DiagnosticAsyncMutex<T> {
        pub fn new(name: &'static str, value: T) -> Self {
            Self {
                inner: tokio::sync::Mutex::new(value),
                info: LockInfo::new(name),
            }
        }

        pub async fn lock(&self) -> DiagnosticAsyncMutexGuard<'_, T> {
            let mut token = WaiterToken::new(&self.info, AcquireKind::Mutex);
            let guard = self.inner.lock().await;
            let holder_id = self
                .info
                .promote_waiter_to_holder(token.id, AcquireKind::Mutex);
            token.disarm();
            DiagnosticAsyncMutexGuard {
                guard: std::mem::ManuallyDrop::new(guard),
                info: Arc::clone(&self.info),
                holder_id,
            }
        }

        pub fn try_lock(
            &self,
        ) -> Result<DiagnosticAsyncMutexGuard<'_, T>, tokio::sync::TryLockError> {
            match self.inner.try_lock() {
                Ok(guard) => {
                    let holder_id = {
                        let id = self.info.next_id.fetch_add(1, Ordering::Relaxed);
                        let (task_id, task_name) = LockInfo::capture_task_info();
                        let mut holders = self.info.holders.lock().unwrap();
                        holders.push(WaiterOrHolder {
                            id,
                            kind: AcquireKind::Mutex,
                            since: Instant::now(),
                            backtrace: Backtrace::force_capture(),
                            task_id,
                            task_name,
                        });
                        self.info.total_acquires.fetch_add(1, Ordering::Relaxed);
                        id
                    };
                    Ok(DiagnosticAsyncMutexGuard {
                        guard: std::mem::ManuallyDrop::new(guard),
                        info: Arc::clone(&self.info),
                        holder_id,
                    })
                }
                Err(e) => Err(e),
            }
        }
    }

    pub struct DiagnosticAsyncMutexGuard<'a, T> {
        guard: std::mem::ManuallyDrop<tokio::sync::MutexGuard<'a, T>>,
        info: Arc<LockInfo>,
        holder_id: u64,
    }

    impl<T> Drop for DiagnosticAsyncMutexGuard<'_, T> {
        fn drop(&mut self) {
            unsafe { std::mem::ManuallyDrop::drop(&mut self.guard) };
            self.info.remove_holder(self.holder_id);
        }
    }

    impl<T> Deref for DiagnosticAsyncMutexGuard<'_, T> {
        type Target = T;
        fn deref(&self) -> &T {
            &self.guard
        }
    }

    impl<T> DerefMut for DiagnosticAsyncMutexGuard<'_, T> {
        fn deref_mut(&mut self) -> &mut T {
            &mut self.guard
        }
    }
}

#[cfg(not(feature = "diagnostics"))]
mod inner {
    // Zero-cost wrappers — the compiler eliminates these entirely.

    pub struct DiagnosticRwLock<T>(parking_lot::RwLock<T>);

    impl<T> DiagnosticRwLock<T> {
        #[inline]
        pub fn new(_name: &'static str, value: T) -> Self {
            Self(parking_lot::RwLock::new(value))
        }

        #[inline]
        pub fn read(&self) -> parking_lot::RwLockReadGuard<'_, T> {
            self.0.read()
        }

        #[inline]
        pub fn write(&self) -> parking_lot::RwLockWriteGuard<'_, T> {
            self.0.write()
        }
    }

    pub struct DiagnosticMutex<T>(parking_lot::Mutex<T>);

    impl<T> DiagnosticMutex<T> {
        #[inline]
        pub fn new(_name: &'static str, value: T) -> Self {
            Self(parking_lot::Mutex::new(value))
        }

        #[inline]
        pub fn lock(&self) -> parking_lot::MutexGuard<'_, T> {
            self.0.lock()
        }
    }

    pub struct DiagnosticAsyncRwLock<T>(tokio::sync::RwLock<T>);

    impl<T> DiagnosticAsyncRwLock<T> {
        #[inline]
        pub fn new(_name: &'static str, value: T) -> Self {
            Self(tokio::sync::RwLock::new(value))
        }

        #[inline]
        pub async fn read(&self) -> tokio::sync::RwLockReadGuard<'_, T> {
            self.0.read().await
        }

        #[inline]
        pub async fn write(&self) -> tokio::sync::RwLockWriteGuard<'_, T> {
            self.0.write().await
        }

        #[inline]
        pub fn try_read(
            &self,
        ) -> Result<tokio::sync::RwLockReadGuard<'_, T>, tokio::sync::TryLockError> {
            self.0.try_read()
        }

        #[inline]
        pub fn try_write(
            &self,
        ) -> Result<tokio::sync::RwLockWriteGuard<'_, T>, tokio::sync::TryLockError> {
            self.0.try_write()
        }
    }

    pub struct DiagnosticAsyncMutex<T>(tokio::sync::Mutex<T>);

    impl<T> DiagnosticAsyncMutex<T> {
        #[inline]
        pub fn new(_name: &'static str, value: T) -> Self {
            Self(tokio::sync::Mutex::new(value))
        }

        #[inline]
        pub async fn lock(&self) -> tokio::sync::MutexGuard<'_, T> {
            self.0.lock().await
        }

        #[inline]
        pub fn try_lock(
            &self,
        ) -> Result<tokio::sync::MutexGuard<'_, T>, tokio::sync::TryLockError> {
            self.0.try_lock()
        }
    }

    #[inline]
    pub fn dump_lock_diagnostics() -> String {
        String::new()
    }

    #[inline]
    pub fn snapshot_lock_diagnostics() -> crate::LockSnapshot {
        crate::LockSnapshot { locks: Vec::new() }
    }
}

pub use inner::*;

// ── Structured snapshot types (for JSON dumps) ───────────────

/// Snapshot of all tracked locks.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "diagnostics", derive(facet::Facet))]
pub struct LockSnapshot {
    pub locks: Vec<LockInfoSnapshot>,
}

/// Snapshot of a single tracked lock.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "diagnostics", derive(facet::Facet))]
pub struct LockInfoSnapshot {
    pub name: String,
    pub acquires: u64,
    pub releases: u64,
    pub holders: Vec<LockHolderSnapshot>,
    pub waiters: Vec<LockWaiterSnapshot>,
}

/// Kind of lock acquisition.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "diagnostics", derive(facet::Facet))]
#[repr(u8)]
pub enum LockAcquireKind {
    Read,
    Write,
    Mutex,
}

/// A current lock holder.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "diagnostics", derive(facet::Facet))]
pub struct LockHolderSnapshot {
    pub kind: LockAcquireKind,
    pub held_secs: f64,
    pub backtrace: Option<String>,
}

/// A lock waiter.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "diagnostics", derive(facet::Facet))]
pub struct LockWaiterSnapshot {
    pub kind: LockAcquireKind,
    pub waiting_secs: f64,
    pub backtrace: Option<String>,
}
