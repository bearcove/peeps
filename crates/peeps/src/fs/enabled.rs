use std::io;
use std::path::{Path, PathBuf};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

#[inline]
fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
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
        let inner = crate::peep!(
            self.inner.open(&path_buf),
            "fs.open_options.open",
            kind = peeps_types::NodeKind::FileOp,
            {
                "fs.op" => "open_options.open",
                "resource.path" => path_str.as_str(),
            }
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
    path: Option<String>,
}

impl File {
    pub async fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let path_buf = path.as_ref().to_path_buf();
        let path_str = path_string(&path_buf);
        let inner = crate::peep!(
            tokio::fs::File::open(&path_buf),
            "fs.file.open",
            kind = peeps_types::NodeKind::FileOp,
            {
                "fs.op" => "file.open",
                "resource.path" => path_str.as_str(),
            }
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
        let inner = crate::peep!(
            tokio::fs::File::create(&path_buf),
            "fs.file.create",
            kind = peeps_types::NodeKind::FileOp,
            {
                "fs.op" => "file.create",
                "resource.path" => path_str.as_str(),
            }
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
        let path = self.path.clone().unwrap_or_else(|| "<unknown>".to_string());
        crate::peep!(
            self.inner.sync_all(),
            "fs.file.sync_all",
            kind = peeps_types::NodeKind::FileOp,
            {
                "fs.op" => "file.sync_all",
                "resource.path" => path.as_str(),
            }
        )
        .await
    }

    pub async fn sync_data(&self) -> io::Result<()> {
        let path = self.path.clone().unwrap_or_else(|| "<unknown>".to_string());
        crate::peep!(
            self.inner.sync_data(),
            "fs.file.sync_data",
            kind = peeps_types::NodeKind::FileOp,
            {
                "fs.op" => "file.sync_data",
                "resource.path" => path.as_str(),
            }
        )
        .await
    }

    pub async fn set_len(&self, size: u64) -> io::Result<()> {
        let path = self.path.clone().unwrap_or_else(|| "<unknown>".to_string());
        crate::peep!(
            self.inner.set_len(size),
            "fs.file.set_len",
            kind = peeps_types::NodeKind::FileOp,
            {
                "fs.op" => "file.set_len",
                "resource.path" => path.as_str(),
                "write.bytes" => size,
            }
        )
        .await
    }

    pub async fn metadata(&self) -> io::Result<std::fs::Metadata> {
        let path = self.path.clone().unwrap_or_else(|| "<unknown>".to_string());
        crate::peep!(
            self.inner.metadata(),
            "fs.file.metadata",
            kind = peeps_types::NodeKind::FileOp,
            {
                "fs.op" => "file.metadata",
                "resource.path" => path.as_str(),
            }
        )
        .await
    }

    pub async fn try_clone(&self) -> io::Result<Self> {
        let path = self.path.clone().unwrap_or_else(|| "<unknown>".to_string());
        let inner = crate::peep!(
            self.inner.try_clone(),
            "fs.file.try_clone",
            kind = peeps_types::NodeKind::FileOp,
            {
                "fs.op" => "file.try_clone",
                "resource.path" => path.as_str(),
            }
        )
        .await?;
        Ok(Self {
            inner,
            path: self.path.clone(),
        })
    }

    pub async fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let path = self.path.clone().unwrap_or_else(|| "<unknown>".to_string());
        crate::peep!(
            self.inner.read(buf),
            "fs.file.read",
            kind = peeps_types::NodeKind::FileOp,
            {
                "fs.op" => "file.read",
                "resource.path" => path.as_str(),
            }
        )
        .await
    }

    pub async fn read_exact(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let path = self.path.clone().unwrap_or_else(|| "<unknown>".to_string());
        crate::peep!(
            self.inner.read_exact(buf),
            "fs.file.read_exact",
            kind = peeps_types::NodeKind::FileOp,
            {
                "fs.op" => "file.read_exact",
                "resource.path" => path.as_str(),
            }
        )
        .await
    }

    pub async fn read_to_end(&mut self, buf: &mut Vec<u8>) -> io::Result<usize> {
        let path = self.path.clone().unwrap_or_else(|| "<unknown>".to_string());
        crate::peep!(
            self.inner.read_to_end(buf),
            "fs.file.read_to_end",
            kind = peeps_types::NodeKind::FileOp,
            {
                "fs.op" => "file.read_to_end",
                "resource.path" => path.as_str(),
            }
        )
        .await
    }

    pub async fn read_to_string(&mut self, buf: &mut String) -> io::Result<usize> {
        let path = self.path.clone().unwrap_or_else(|| "<unknown>".to_string());
        crate::peep!(
            self.inner.read_to_string(buf),
            "fs.file.read_to_string",
            kind = peeps_types::NodeKind::FileOp,
            {
                "fs.op" => "file.read_to_string",
                "resource.path" => path.as_str(),
            }
        )
        .await
    }

    pub async fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let path = self.path.clone().unwrap_or_else(|| "<unknown>".to_string());
        let write_bytes = buf.len() as u64;
        crate::peep!(
            self.inner.write(buf),
            "fs.file.write",
            kind = peeps_types::NodeKind::FileOp,
            {
                "fs.op" => "file.write",
                "resource.path" => path.as_str(),
                "write.bytes" => write_bytes,
            }
        )
        .await
    }

    pub async fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        let path = self.path.clone().unwrap_or_else(|| "<unknown>".to_string());
        let write_bytes = buf.len() as u64;
        crate::peep!(
            self.inner.write_all(buf),
            "fs.file.write_all",
            kind = peeps_types::NodeKind::FileOp,
            {
                "fs.op" => "file.write_all",
                "resource.path" => path.as_str(),
                "write.bytes" => write_bytes,
            }
        )
        .await
    }

    pub async fn flush(&mut self) -> io::Result<()> {
        let path = self.path.clone().unwrap_or_else(|| "<unknown>".to_string());
        crate::peep!(
            self.inner.flush(),
            "fs.file.flush",
            kind = peeps_types::NodeKind::FileOp,
            {
                "fs.op" => "file.flush",
                "resource.path" => path.as_str(),
            }
        )
        .await
    }

    pub async fn seek(&mut self, pos: std::io::SeekFrom) -> io::Result<u64> {
        let path = self.path.clone().unwrap_or_else(|| "<unknown>".to_string());
        crate::peep!(
            self.inner.seek(pos),
            "fs.file.seek",
            kind = peeps_types::NodeKind::FileOp,
            {
                "fs.op" => "file.seek",
                "resource.path" => path.as_str(),
            }
        )
        .await
    }
}

pub async fn create_dir_all(path: impl AsRef<Path>) -> io::Result<()> {
    let path_buf = path.as_ref().to_path_buf();
    let path_str = path_string(&path_buf);
    crate::peep!(
        tokio::fs::create_dir_all(&path_buf),
        "fs.create_dir_all",
        kind = peeps_types::NodeKind::FileOp,
        {
            "fs.op" => "create_dir_all",
            "resource.path" => path_str.as_str(),
        }
    )
    .await
}

pub async fn write(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> io::Result<()> {
    let path_buf = path.as_ref().to_path_buf();
    let path_str = path_string(&path_buf);
    let write_bytes = contents.as_ref().len() as u64;
    crate::peep!(
        tokio::fs::write(&path_buf, contents),
        "fs.write",
        kind = peeps_types::NodeKind::FileOp,
        {
            "fs.op" => "write",
            "resource.path" => path_str.as_str(),
            "write.bytes" => write_bytes,
        }
    )
    .await
}

pub async fn read_to_string(path: impl AsRef<Path>) -> io::Result<String> {
    let path_buf = path.as_ref().to_path_buf();
    let path_str = path_string(&path_buf);
    crate::peep!(
        tokio::fs::read_to_string(&path_buf),
        "fs.read_to_string",
        kind = peeps_types::NodeKind::FileOp,
        {
            "fs.op" => "read_to_string",
            "resource.path" => path_str.as_str(),
        }
    )
    .await
}

pub async fn metadata(path: impl AsRef<Path>) -> io::Result<std::fs::Metadata> {
    let path_buf = path.as_ref().to_path_buf();
    let path_str = path_string(&path_buf);
    crate::peep!(
        tokio::fs::metadata(&path_buf),
        "fs.metadata",
        kind = peeps_types::NodeKind::FileOp,
        {
            "fs.op" => "metadata",
            "resource.path" => path_str.as_str(),
        }
    )
    .await
}

pub async fn set_permissions(
    path: impl AsRef<Path>,
    perm: std::fs::Permissions,
) -> io::Result<()> {
    let path_buf = path.as_ref().to_path_buf();
    let path_str = path_string(&path_buf);
    crate::peep!(
        tokio::fs::set_permissions(&path_buf, perm),
        "fs.set_permissions",
        kind = peeps_types::NodeKind::FileOp,
        {
            "fs.op" => "set_permissions",
            "resource.path" => path_str.as_str(),
        }
    )
    .await
}

pub async fn rename(from: impl AsRef<Path>, to: impl AsRef<Path>) -> io::Result<()> {
    let from_buf = from.as_ref().to_path_buf();
    let to_buf = to.as_ref().to_path_buf();
    let from_str = path_string(&from_buf);
    let to_str = path_string(&to_buf);
    crate::peep!(
        tokio::fs::rename(&from_buf, &to_buf),
        "fs.rename",
        kind = peeps_types::NodeKind::FileOp,
        {
            "fs.op" => "rename",
            "resource.from_path" => from_str.as_str(),
            "resource.to_path" => to_str.as_str(),
        }
    )
    .await
}

pub async fn try_exists(path: impl AsRef<Path>) -> io::Result<bool> {
    let path_buf = path.as_ref().to_path_buf();
    let path_str = path_string(&path_buf);
    crate::peep!(
        tokio::fs::try_exists(&path_buf),
        "fs.try_exists",
        kind = peeps_types::NodeKind::FileOp,
        {
            "fs.op" => "try_exists",
            "resource.path" => path_str.as_str(),
        }
    )
    .await
}

pub async fn read(path: impl AsRef<Path>) -> io::Result<Vec<u8>> {
    let path_buf = path.as_ref().to_path_buf();
    let path_str = path_string(&path_buf);
    crate::peep!(
        tokio::fs::read(&path_buf),
        "fs.read",
        kind = peeps_types::NodeKind::FileOp,
        {
            "fs.op" => "read",
            "resource.path" => path_str.as_str(),
        }
    )
    .await
}

pub async fn remove_file(path: impl AsRef<Path>) -> io::Result<()> {
    let path_buf = path.as_ref().to_path_buf();
    let path_str = path_string(&path_buf);
    crate::peep!(
        tokio::fs::remove_file(&path_buf),
        "fs.remove_file",
        kind = peeps_types::NodeKind::FileOp,
        {
            "fs.op" => "remove_file",
            "resource.path" => path_str.as_str(),
        }
    )
    .await
}

pub async fn remove_dir(path: impl AsRef<Path>) -> io::Result<()> {
    let path_buf = path.as_ref().to_path_buf();
    let path_str = path_string(&path_buf);
    crate::peep!(
        tokio::fs::remove_dir(&path_buf),
        "fs.remove_dir",
        kind = peeps_types::NodeKind::FileOp,
        {
            "fs.op" => "remove_dir",
            "resource.path" => path_str.as_str(),
        }
    )
    .await
}

pub async fn remove_dir_all(path: impl AsRef<Path>) -> io::Result<()> {
    let path_buf = path.as_ref().to_path_buf();
    let path_str = path_string(&path_buf);
    crate::peep!(
        tokio::fs::remove_dir_all(&path_buf),
        "fs.remove_dir_all",
        kind = peeps_types::NodeKind::FileOp,
        {
            "fs.op" => "remove_dir_all",
            "resource.path" => path_str.as_str(),
        }
    )
    .await
}

pub async fn canonicalize(path: impl AsRef<Path>) -> io::Result<PathBuf> {
    let path_buf = path.as_ref().to_path_buf();
    let path_str = path_string(&path_buf);
    crate::peep!(
        tokio::fs::canonicalize(&path_buf),
        "fs.canonicalize",
        kind = peeps_types::NodeKind::FileOp,
        {
            "fs.op" => "canonicalize",
            "resource.path" => path_str.as_str(),
        }
    )
    .await
}

#[cfg(unix)]
pub async fn symlink(original: impl AsRef<Path>, link: impl AsRef<Path>) -> io::Result<()> {
    let original_buf = original.as_ref().to_path_buf();
    let link_buf = link.as_ref().to_path_buf();
    let original_str = path_string(&original_buf);
    let link_str = path_string(&link_buf);
    crate::peep!(
        tokio::fs::symlink(&original_buf, &link_buf),
        "fs.symlink",
        kind = peeps_types::NodeKind::FileOp,
        {
            "fs.op" => "symlink",
            "resource.from_path" => original_str.as_str(),
            "resource.to_path" => link_str.as_str(),
        }
    )
    .await
}

#[cfg(windows)]
pub async fn symlink_file(original: impl AsRef<Path>, link: impl AsRef<Path>) -> io::Result<()> {
    let original_buf = original.as_ref().to_path_buf();
    let link_buf = link.as_ref().to_path_buf();
    let original_str = path_string(&original_buf);
    let link_str = path_string(&link_buf);
    crate::peep!(
        tokio::fs::symlink_file(&original_buf, &link_buf),
        "fs.symlink_file",
        kind = peeps_types::NodeKind::FileOp,
        {
            "fs.op" => "symlink_file",
            "resource.from_path" => original_str.as_str(),
            "resource.to_path" => link_str.as_str(),
        }
    )
    .await
}
