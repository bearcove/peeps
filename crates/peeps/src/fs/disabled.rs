use std::io;
use std::path::{Path, PathBuf};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

pub struct OpenOptions {
    inner: tokio::fs::OpenOptions,
}

impl OpenOptions {
    #[inline]
    pub fn new() -> Self {
        Self {
            inner: tokio::fs::OpenOptions::new(),
        }
    }

    #[inline]
    pub fn read(&mut self, read: bool) -> &mut Self {
        self.inner.read(read);
        self
    }

    #[inline]
    pub fn write(&mut self, write: bool) -> &mut Self {
        self.inner.write(write);
        self
    }

    #[inline]
    pub fn append(&mut self, append: bool) -> &mut Self {
        self.inner.append(append);
        self
    }

    #[inline]
    pub fn truncate(&mut self, truncate: bool) -> &mut Self {
        self.inner.truncate(truncate);
        self
    }

    #[inline]
    pub fn create(&mut self, create: bool) -> &mut Self {
        self.inner.create(create);
        self
    }

    #[inline]
    pub fn create_new(&mut self, create_new: bool) -> &mut Self {
        self.inner.create_new(create_new);
        self
    }

    #[inline]
    pub async fn open(&self, path: impl AsRef<Path>) -> io::Result<File> {
        let inner = self.inner.open(path).await?;
        Ok(File {
            inner,
            path: None,
        })
    }

    #[inline]
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
    path: Option<String>,
}

impl File {
    #[inline]
    pub async fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let inner = tokio::fs::File::open(path).await?;
        Ok(Self {
            inner,
            path: None,
        })
    }

    #[inline]
    pub async fn create(path: impl AsRef<Path>) -> io::Result<Self> {
        let inner = tokio::fs::File::create(path).await?;
        Ok(Self {
            inner,
            path: None,
        })
    }

    #[inline]
    pub fn from_tokio(file: tokio::fs::File) -> Self {
        Self {
            inner: file,
            path: None,
        }
    }

    #[inline]
    pub fn into_inner(self) -> tokio::fs::File {
        self.inner
    }

    #[inline]
    pub fn as_mut_tokio(&mut self) -> &mut tokio::fs::File {
        &mut self.inner
    }

    #[inline]
    pub async fn sync_all(&self) -> io::Result<()> {
        self.inner.sync_all().await
    }

    #[inline]
    pub async fn sync_data(&self) -> io::Result<()> {
        self.inner.sync_data().await
    }

    #[inline]
    pub async fn set_len(&self, size: u64) -> io::Result<()> {
        self.inner.set_len(size).await
    }

    #[inline]
    pub async fn metadata(&self) -> io::Result<std::fs::Metadata> {
        self.inner.metadata().await
    }

    #[inline]
    pub async fn try_clone(&self) -> io::Result<Self> {
        let inner = self.inner.try_clone().await?;
        Ok(Self {
            inner,
            path: self.path.clone(),
        })
    }

    #[inline]
    pub async fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buf).await
    }

    #[inline]
    pub async fn read_exact(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.read_exact(buf).await
    }

    #[inline]
    pub async fn read_to_end(&mut self, buf: &mut Vec<u8>) -> io::Result<usize> {
        self.inner.read_to_end(buf).await
    }

    #[inline]
    pub async fn read_to_string(&mut self, buf: &mut String) -> io::Result<usize> {
        self.inner.read_to_string(buf).await
    }

    #[inline]
    pub async fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.write(buf).await
    }

    #[inline]
    pub async fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        self.inner.write_all(buf).await
    }

    #[inline]
    pub async fn flush(&mut self) -> io::Result<()> {
        self.inner.flush().await
    }

    #[inline]
    pub async fn seek(&mut self, pos: std::io::SeekFrom) -> io::Result<u64> {
        self.inner.seek(pos).await
    }
}

#[inline]
pub async fn create_dir_all(path: impl AsRef<Path>) -> io::Result<()> {
    tokio::fs::create_dir_all(path).await
}

#[inline]
pub async fn write(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> io::Result<()> {
    tokio::fs::write(path, contents).await
}

#[inline]
pub async fn read_to_string(path: impl AsRef<Path>) -> io::Result<String> {
    tokio::fs::read_to_string(path).await
}

#[inline]
pub async fn metadata(path: impl AsRef<Path>) -> io::Result<std::fs::Metadata> {
    tokio::fs::metadata(path).await
}

#[inline]
pub async fn set_permissions(
    path: impl AsRef<Path>,
    perm: std::fs::Permissions,
) -> io::Result<()> {
    tokio::fs::set_permissions(path, perm).await
}

#[inline]
pub async fn rename(from: impl AsRef<Path>, to: impl AsRef<Path>) -> io::Result<()> {
    tokio::fs::rename(from, to).await
}

#[inline]
pub async fn try_exists(path: impl AsRef<Path>) -> io::Result<bool> {
    tokio::fs::try_exists(path).await
}

#[inline]
pub async fn read(path: impl AsRef<Path>) -> io::Result<Vec<u8>> {
    tokio::fs::read(path).await
}

#[inline]
pub async fn remove_file(path: impl AsRef<Path>) -> io::Result<()> {
    tokio::fs::remove_file(path).await
}

#[inline]
pub async fn remove_dir(path: impl AsRef<Path>) -> io::Result<()> {
    tokio::fs::remove_dir(path).await
}

#[inline]
pub async fn remove_dir_all(path: impl AsRef<Path>) -> io::Result<()> {
    tokio::fs::remove_dir_all(path).await
}

#[inline]
pub async fn canonicalize(path: impl AsRef<Path>) -> io::Result<PathBuf> {
    tokio::fs::canonicalize(path).await
}

#[cfg(unix)]
#[inline]
pub async fn symlink(original: impl AsRef<Path>, link: impl AsRef<Path>) -> io::Result<()> {
    tokio::fs::symlink(original, link).await
}

#[cfg(windows)]
#[inline]
pub async fn symlink_file(original: impl AsRef<Path>, link: impl AsRef<Path>) -> io::Result<()> {
    tokio::fs::symlink_file(original, link).await
}
