use crate::{instrument_future_on, EntityHandle};
use compact_str::CompactString;
use peeps_types::{EntityBody, FileOpEntity, FileOpKind};
use std::future::Future;
use std::io;
use std::path::{Path, PathBuf};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

fn path_string(path: &Path) -> CompactString {
    CompactString::from(path.to_string_lossy().as_ref())
}

fn join_paths(from: &Path, to: &Path) -> CompactString {
    CompactString::from(format!(
        "{} -> {}",
        from.to_string_lossy(),
        to.to_string_lossy()
    ))
}

fn fallback_path(path: &Option<CompactString>) -> CompactString {
    path.clone()
        .unwrap_or_else(|| CompactString::from("<unknown>"))
}

async fn file_op<T, F>(name: &str, op: FileOpKind, path: CompactString, fut: F) -> io::Result<T>
where
    F: Future<Output = io::Result<T>>,
{
    let handle = EntityHandle::new(
        CompactString::from(format!("fs.{name}")),
        EntityBody::FileOp(FileOpEntity { op, path }),
    );
    instrument_future_on(CompactString::from(format!("fs.{name}")), &handle, fut).await
}

pub struct OpenOptions {
    inner: tokio::fs::OpenOptions,
}

impl OpenOptions {
    pub fn new() -> Self {
        Self {
            inner: tokio::fs::OpenOptions::new(),
        }
    }

    pub fn read(&mut self, read: bool) -> &mut Self {
        self.inner.read(read);
        self
    }

    pub fn write(&mut self, write: bool) -> &mut Self {
        self.inner.write(write);
        self
    }

    pub fn append(&mut self, append: bool) -> &mut Self {
        self.inner.append(append);
        self
    }

    pub fn truncate(&mut self, truncate: bool) -> &mut Self {
        self.inner.truncate(truncate);
        self
    }

    pub fn create(&mut self, create: bool) -> &mut Self {
        self.inner.create(create);
        self
    }

    pub fn create_new(&mut self, create_new: bool) -> &mut Self {
        self.inner.create_new(create_new);
        self
    }

    pub async fn open(&self, path: impl AsRef<Path>) -> io::Result<File> {
        let path_buf = path.as_ref().to_path_buf();
        let path_str = path_string(&path_buf);
        let inner = file_op(
            "open_options.open",
            FileOpKind::Open,
            path_str.clone(),
            self.inner.open(&path_buf),
        )
        .await?;
        Ok(File {
            inner,
            path: Some(path_str),
        })
    }

    pub fn into_inner(self) -> tokio::fs::OpenOptions {
        self.inner
    }
}

impl Default for OpenOptions {
    fn default() -> Self {
        Self::new()
    }
}

pub struct File {
    inner: tokio::fs::File,
    path: Option<CompactString>,
}

impl File {
    pub async fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let path_buf = path.as_ref().to_path_buf();
        let path_str = path_string(&path_buf);
        let inner = file_op(
            "file.open",
            FileOpKind::Open,
            path_str.clone(),
            tokio::fs::File::open(&path_buf),
        )
        .await?;
        Ok(Self {
            inner,
            path: Some(path_str),
        })
    }

    pub async fn create(path: impl AsRef<Path>) -> io::Result<Self> {
        let path_buf = path.as_ref().to_path_buf();
        let path_str = path_string(&path_buf);
        let inner = file_op(
            "file.create",
            FileOpKind::Open,
            path_str.clone(),
            tokio::fs::File::create(&path_buf),
        )
        .await?;
        Ok(Self {
            inner,
            path: Some(path_str),
        })
    }

    pub fn from_tokio(file: tokio::fs::File) -> Self {
        Self {
            inner: file,
            path: None,
        }
    }

    pub fn into_inner(self) -> tokio::fs::File {
        self.inner
    }

    pub fn as_mut_tokio(&mut self) -> &mut tokio::fs::File {
        &mut self.inner
    }

    pub async fn sync_all(&self) -> io::Result<()> {
        file_op(
            "file.sync_all",
            FileOpKind::Sync,
            fallback_path(&self.path),
            self.inner.sync_all(),
        )
        .await
    }

    pub async fn sync_data(&self) -> io::Result<()> {
        file_op(
            "file.sync_data",
            FileOpKind::Sync,
            fallback_path(&self.path),
            self.inner.sync_data(),
        )
        .await
    }

    pub async fn set_len(&self, size: u64) -> io::Result<()> {
        let _ = size;
        file_op(
            "file.set_len",
            FileOpKind::Write,
            fallback_path(&self.path),
            self.inner.set_len(size),
        )
        .await
    }

    pub async fn metadata(&self) -> io::Result<std::fs::Metadata> {
        file_op(
            "file.metadata",
            FileOpKind::Metadata,
            fallback_path(&self.path),
            self.inner.metadata(),
        )
        .await
    }

    pub async fn try_clone(&self) -> io::Result<Self> {
        let inner = file_op(
            "file.try_clone",
            FileOpKind::Open,
            fallback_path(&self.path),
            self.inner.try_clone(),
        )
        .await?;
        Ok(Self {
            inner,
            path: self.path.clone(),
        })
    }

    pub async fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        file_op(
            "file.read",
            FileOpKind::Read,
            fallback_path(&self.path),
            self.inner.read(buf),
        )
        .await
    }

    pub async fn read_exact(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        file_op(
            "file.read_exact",
            FileOpKind::Read,
            fallback_path(&self.path),
            self.inner.read_exact(buf),
        )
        .await
    }

    pub async fn read_to_end(&mut self, buf: &mut Vec<u8>) -> io::Result<usize> {
        file_op(
            "file.read_to_end",
            FileOpKind::Read,
            fallback_path(&self.path),
            self.inner.read_to_end(buf),
        )
        .await
    }

    pub async fn read_to_string(&mut self, buf: &mut String) -> io::Result<usize> {
        file_op(
            "file.read_to_string",
            FileOpKind::Read,
            fallback_path(&self.path),
            self.inner.read_to_string(buf),
        )
        .await
    }

    pub async fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        file_op(
            "file.write",
            FileOpKind::Write,
            fallback_path(&self.path),
            self.inner.write(buf),
        )
        .await
    }

    pub async fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        file_op(
            "file.write_all",
            FileOpKind::Write,
            fallback_path(&self.path),
            self.inner.write_all(buf),
        )
        .await
    }

    pub async fn flush(&mut self) -> io::Result<()> {
        file_op(
            "file.flush",
            FileOpKind::Write,
            fallback_path(&self.path),
            self.inner.flush(),
        )
        .await
    }

    pub async fn seek(&mut self, pos: std::io::SeekFrom) -> io::Result<u64> {
        file_op(
            "file.seek",
            FileOpKind::Read,
            fallback_path(&self.path),
            self.inner.seek(pos),
        )
        .await
    }
}

pub async fn create_dir_all(path: impl AsRef<Path>) -> io::Result<()> {
    let path_buf = path.as_ref().to_path_buf();
    file_op(
        "create_dir_all",
        FileOpKind::Open,
        path_string(&path_buf),
        tokio::fs::create_dir_all(path_buf),
    )
    .await
}

pub async fn write(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> io::Result<()> {
    let path_buf = path.as_ref().to_path_buf();
    file_op(
        "write",
        FileOpKind::Write,
        path_string(&path_buf),
        tokio::fs::write(path_buf, contents),
    )
    .await
}

pub async fn read_to_string(path: impl AsRef<Path>) -> io::Result<String> {
    let path_buf = path.as_ref().to_path_buf();
    file_op(
        "read_to_string",
        FileOpKind::Read,
        path_string(&path_buf),
        tokio::fs::read_to_string(path_buf),
    )
    .await
}

pub async fn metadata(path: impl AsRef<Path>) -> io::Result<std::fs::Metadata> {
    let path_buf = path.as_ref().to_path_buf();
    file_op(
        "metadata",
        FileOpKind::Metadata,
        path_string(&path_buf),
        tokio::fs::metadata(path_buf),
    )
    .await
}

pub async fn set_permissions(path: impl AsRef<Path>, perm: std::fs::Permissions) -> io::Result<()> {
    let path_buf = path.as_ref().to_path_buf();
    file_op(
        "set_permissions",
        FileOpKind::Write,
        path_string(&path_buf),
        tokio::fs::set_permissions(path_buf, perm),
    )
    .await
}

pub async fn rename(from: impl AsRef<Path>, to: impl AsRef<Path>) -> io::Result<()> {
    let from_buf = from.as_ref().to_path_buf();
    let to_buf = to.as_ref().to_path_buf();
    file_op(
        "rename",
        FileOpKind::Rename,
        join_paths(&from_buf, &to_buf),
        tokio::fs::rename(from_buf, to_buf),
    )
    .await
}

pub async fn try_exists(path: impl AsRef<Path>) -> io::Result<bool> {
    let path_buf = path.as_ref().to_path_buf();
    file_op(
        "try_exists",
        FileOpKind::Metadata,
        path_string(&path_buf),
        tokio::fs::try_exists(path_buf),
    )
    .await
}

pub async fn read(path: impl AsRef<Path>) -> io::Result<Vec<u8>> {
    let path_buf = path.as_ref().to_path_buf();
    file_op(
        "read",
        FileOpKind::Read,
        path_string(&path_buf),
        tokio::fs::read(path_buf),
    )
    .await
}

pub async fn remove_file(path: impl AsRef<Path>) -> io::Result<()> {
    let path_buf = path.as_ref().to_path_buf();
    file_op(
        "remove_file",
        FileOpKind::Remove,
        path_string(&path_buf),
        tokio::fs::remove_file(path_buf),
    )
    .await
}

pub async fn remove_dir(path: impl AsRef<Path>) -> io::Result<()> {
    let path_buf = path.as_ref().to_path_buf();
    file_op(
        "remove_dir",
        FileOpKind::Remove,
        path_string(&path_buf),
        tokio::fs::remove_dir(path_buf),
    )
    .await
}

pub async fn remove_dir_all(path: impl AsRef<Path>) -> io::Result<()> {
    let path_buf = path.as_ref().to_path_buf();
    file_op(
        "remove_dir_all",
        FileOpKind::Remove,
        path_string(&path_buf),
        tokio::fs::remove_dir_all(path_buf),
    )
    .await
}

pub async fn canonicalize(path: impl AsRef<Path>) -> io::Result<PathBuf> {
    let path_buf = path.as_ref().to_path_buf();
    file_op(
        "canonicalize",
        FileOpKind::Metadata,
        path_string(&path_buf),
        tokio::fs::canonicalize(path_buf),
    )
    .await
}

#[cfg(unix)]
pub async fn symlink(original: impl AsRef<Path>, link: impl AsRef<Path>) -> io::Result<()> {
    let original_buf = original.as_ref().to_path_buf();
    let link_buf = link.as_ref().to_path_buf();
    file_op(
        "symlink",
        FileOpKind::Write,
        join_paths(&original_buf, &link_buf),
        tokio::fs::symlink(original_buf, link_buf),
    )
    .await
}

#[cfg(windows)]
pub async fn symlink_file(original: impl AsRef<Path>, link: impl AsRef<Path>) -> io::Result<()> {
    let original_buf = original.as_ref().to_path_buf();
    let link_buf = link.as_ref().to_path_buf();
    file_op(
        "symlink_file",
        FileOpKind::Write,
        join_paths(&original_buf, &link_buf),
        tokio::fs::symlink_file(original_buf, link_buf),
    )
    .await
}
