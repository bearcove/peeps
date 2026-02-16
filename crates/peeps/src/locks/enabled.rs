use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex as StdMutex, Weak};
use std::time::Instant;

use peeps_types::{GraphSnapshot, Node};

// ── Lock info registry ─────────────────────────────────

static LOCK_INFOS: LazyLock<StdMutex<Vec<Weak<LockInfo>>>> =
    LazyLock::new(|| StdMutex::new(Vec::new()));

pub(crate) fn emit_into_graph(graph: &mut GraphSnapshot) {
    let Ok(lock_infos) = LOCK_INFOS.lock() else {
        return;
    };

    for weak in lock_infos.iter() {
        let Some(info) = weak.upgrade() else {
            continue;
        };

        let (Ok(waiters), Ok(holders)) = (info.waiters.lock(), info.holders.lock()) else {
            continue;
        };

        let acquires = info.total_acquires.load(Ordering::SeqCst);
        let releases = info.total_releases.load(Ordering::SeqCst);
        let holder_count = holders.len() as u64;
        let waiter_count = waiters.len() as u64;

        let lock_kind = {
            let first_kind = holders.first().or(waiters.first()).map(|e| e.kind);
            match first_kind {
                Some(AcquireKind::Mutex) => "mutex",
                Some(AcquireKind::Write) => "rwlock_write",
                Some(AcquireKind::Read) => {
                    if holders
                        .iter()
                        .any(|h| matches!(h.kind, AcquireKind::Write))
                        || waiters
                            .iter()
                            .any(|w| matches!(w.kind, AcquireKind::Write))
                    {
                        "rwlock_write"
                    } else {
                        "rwlock_read"
                    }
                }
                None => "mutex",
            }
        };

        let mut attrs = String::with_capacity(256);
        attrs.push('{');
        write_json_kv_str(&mut attrs, "name", info.name, true);
        write_json_kv_str(&mut attrs, "lock_kind", lock_kind, false);
        write_json_kv_u64(&mut attrs, "acquires", acquires, false);
        write_json_kv_u64(&mut attrs, "releases", releases, false);
        write_json_kv_u64(&mut attrs, "holder_count", holder_count, false);
        write_json_kv_u64(&mut attrs, "waiter_count", waiter_count, false);
        attrs.push_str(",\"meta\":{");
        write_json_kv_str(
            &mut attrs,
            peeps_types::meta_key::CTX_LOCATION,
            &info.location,
            true,
        );
        attrs.push('}');
        attrs.push('}');

        graph.nodes.push(Node {
            id: info.endpoint_id.clone(),
            kind: peeps_types::NodeKind::Lock,
            label: Some(info.name.to_string()),
            attrs_json: attrs,
        });
    }
}

fn write_json_kv_str(out: &mut String, key: &str, value: &str, first: bool) {
    if !first {
        out.push(',');
    }
    out.push('"');
    out.push_str(key);
    out.push_str("\":\"");
    peeps_types::json_escape_into(out, value);
    out.push('"');
}

fn write_json_kv_u64(out: &mut String, key: &str, value: u64, first: bool) {
    use std::io::Write;
    if !first {
        out.push(',');
    }
    out.push('"');
    out.push_str(key);
    out.push_str("\":");
    let mut buf = [0u8; 20];
    let mut cursor = std::io::Cursor::new(&mut buf[..]);
    let _ = write!(cursor, "{value}");
    let len = cursor.position() as usize;
    out.push_str(std::str::from_utf8(&buf[..len]).unwrap_or("0"));
}

// ── Internal types ─────────────────────────────────────

#[derive(Debug, Clone, Copy)]
enum AcquireKind {
    Read,
    Write,
    Mutex,
}

struct WaiterOrHolder {
    id: u64,
    kind: AcquireKind,
    #[allow(dead_code)]
    since: Instant,
}

struct LockInfo {
    name: &'static str,
    endpoint_id: String,
    location: String,
    next_id: AtomicU64,
    waiters: StdMutex<Vec<WaiterOrHolder>>,
    holders: StdMutex<Vec<WaiterOrHolder>>,
    total_acquires: AtomicU64,
    total_releases: AtomicU64,
}

impl LockInfo {
    #[track_caller]
    fn new(name: &'static str) -> Arc<Self> {
        let caller = std::panic::Location::caller();
        let location = crate::caller_location(caller);
        let info = Arc::new(Self {
            name,
            endpoint_id: peeps_types::new_node_id("lock"),
            location,
            next_id: AtomicU64::new(0),
            waiters: StdMutex::new(Vec::new()),
            holders: StdMutex::new(Vec::new()),
            total_acquires: AtomicU64::new(0),
            total_releases: AtomicU64::new(0),
        });
        LOCK_INFOS.lock().unwrap().push(Arc::downgrade(&info));
        info
    }

    fn add_waiter(&self, kind: AcquireKind) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.waiters.lock().unwrap().push(WaiterOrHolder {
            id,
            kind,
            since: Instant::now(),
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

        let mut waiters = self.waiters.lock().unwrap();
        let mut holders = self.holders.lock().unwrap();

        waiters.retain(|w| w.id != waiter_id);
        holders.push(WaiterOrHolder {
            id: holder_id,
            kind,
            since: Instant::now(),
        });

        self.total_acquires.fetch_add(1, Ordering::Relaxed);
        holder_id
    }

    fn add_holder(&self, kind: AcquireKind) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.holders.lock().unwrap().push(WaiterOrHolder {
            id,
            kind,
            since: Instant::now(),
        });
        self.total_acquires.fetch_add(1, Ordering::Relaxed);
        id
    }
}

// ── WaiterToken (cancellation-safe waiter cleanup) ─────

struct WaiterToken {
    info: Arc<LockInfo>,
    id: u64,
    edge_src: Option<String>,
    armed: bool,
}

impl WaiterToken {
    fn new(info: &Arc<LockInfo>, kind: AcquireKind) -> Self {
        let id = info.add_waiter(kind);
        let mut edge_src = None;
        crate::stack::with_top(|src| {
            crate::registry::edge(src, &info.endpoint_id);
            edge_src = Some(src.to_string());
        });
        Self {
            info: Arc::clone(info),
            id,
            edge_src,
            armed: true,
        }
    }

    fn disarm(&mut self) {
        if let Some(ref src) = self.edge_src {
            crate::registry::remove_edge(src, &self.info.endpoint_id);
        }
        self.armed = false;
    }
}

impl Drop for WaiterToken {
    fn drop(&mut self) {
        if self.armed {
            self.info.remove_waiter(self.id);
            if let Some(ref src) = self.edge_src {
                crate::registry::remove_edge(src, &self.info.endpoint_id);
            }
        }
    }
}

// ── Sync edge helpers ──────────────────────────────────

fn emit_wait_edge(endpoint_id: &str) -> Option<String> {
    let mut src = None;
    crate::stack::with_top(|s| {
        crate::registry::edge(s, endpoint_id);
        src = Some(s.to_string());
    });
    src
}

fn remove_wait_edge(src: &Option<String>, endpoint_id: &str) {
    if let Some(ref s) = src {
        crate::registry::remove_edge(s, endpoint_id);
    }
}

// ── DiagnosticRwLock ───────────────────────────────────

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
        let edge_src = emit_wait_edge(&self.info.endpoint_id);
        let guard = self.inner.read();
        remove_wait_edge(&edge_src, &self.info.endpoint_id);
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
        let edge_src = emit_wait_edge(&self.info.endpoint_id);
        let guard = self.inner.write();
        remove_wait_edge(&edge_src, &self.info.endpoint_id);
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

// ── DiagnosticMutex ────────────────────────────────────

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
        let edge_src = emit_wait_edge(&self.info.endpoint_id);
        let guard = self.inner.lock();
        remove_wait_edge(&edge_src, &self.info.endpoint_id);
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

// ── DiagnosticAsyncRwLock ──────────────────────────────

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
                let holder_id = self.info.add_holder(AcquireKind::Read);
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
                let holder_id = self.info.add_holder(AcquireKind::Write);
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

// ── DiagnosticAsyncMutex ───────────────────────────────

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
                let holder_id = self.info.add_holder(AcquireKind::Mutex);
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
