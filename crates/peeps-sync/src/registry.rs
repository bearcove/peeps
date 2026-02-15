use std::sync::{Arc, LazyLock, Mutex, Weak};

use crate::channels::{MpscInfo, OneshotInfo, WatchInfo};
use crate::oncecell::OnceCellInfo;
use crate::semaphore::SemaphoreInfo;

pub(crate) static REGISTRY: LazyLock<Mutex<Registry>> =
    LazyLock::new(|| Mutex::new(Registry::default()));

#[derive(Default)]
pub(crate) struct Registry {
    pub(crate) mpsc: Vec<Weak<MpscInfo>>,
    pub(crate) oneshot: Vec<Weak<OneshotInfo>>,
    pub(crate) watch: Vec<Weak<WatchInfo>>,
    pub(crate) semaphore: Vec<Weak<SemaphoreInfo>>,
    pub(crate) once_cell: Vec<Weak<OnceCellInfo>>,
}

pub(crate) fn prune_and_register_mpsc(info: &Arc<MpscInfo>) {
    let mut reg = REGISTRY.lock().unwrap();
    reg.mpsc.retain(|w| w.strong_count() > 0);
    reg.mpsc.push(Arc::downgrade(info));
}

pub(crate) fn prune_and_register_oneshot(info: &Arc<OneshotInfo>) {
    let mut reg = REGISTRY.lock().unwrap();
    reg.oneshot.retain(|w| w.strong_count() > 0);
    reg.oneshot.push(Arc::downgrade(info));
}

pub(crate) fn prune_and_register_watch(info: &Arc<WatchInfo>) {
    let mut reg = REGISTRY.lock().unwrap();
    reg.watch.retain(|w| w.strong_count() > 0);
    reg.watch.push(Arc::downgrade(info));
}

pub(crate) fn prune_and_register_semaphore(info: &Arc<SemaphoreInfo>) {
    let mut reg = REGISTRY.lock().unwrap();
    reg.semaphore.retain(|w| w.strong_count() > 0);
    reg.semaphore.push(Arc::downgrade(info));
}

pub(crate) fn prune_and_register_once_cell(info: &Arc<OnceCellInfo>) {
    let mut reg = REGISTRY.lock().unwrap();
    reg.once_cell.retain(|w| w.strong_count() > 0);
    reg.once_cell.push(Arc::downgrade(info));
}
