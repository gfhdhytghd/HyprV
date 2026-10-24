//! Reusable, bounded positioned reads for procfs/sysfs sampling.
use std::{
    fs::{File, OpenOptions},
    io,
    os::unix::{
        fs::MetadataExt,
        fs::{FileExt, OpenOptionsExt},
    },
    path::{Path, PathBuf},
};

pub struct SampleFile {
    path: PathBuf,
    file: Option<File>,
    bytes: Vec<u8>,
    verify_unlinked: bool,
    limit: usize,
    keep_open: bool,
}
impl SampleFile {
    pub fn new(path: impl Into<PathBuf>, limit: usize) -> Self {
        Self {
            path: path.into(),
            file: None,
            bytes: vec![0; limit],
            verify_unlinked: false,
            limit,
            keep_open: true,
        }
    }
    pub fn growable(path: impl Into<PathBuf>, initial: usize, limit: usize) -> Self {
        let mut file = Self::new(path, initial.min(limit));
        file.limit = limit;
        file
    }
    /// Fixture files can be atomically replaced without kernel hotplug events.
    pub fn verify_unlinked(mut self, value: bool) -> Self {
        self.verify_unlinked = value;
        self
    }
    /// Cache buffers but release the handle for processes beyond the FD budget.
    pub fn keep_open(mut self, value: bool) -> Self {
        self.keep_open = value;
        self
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn invalidate(&mut self) {
        self.file = None;
    }
    fn open(&mut self) -> io::Result<()> {
        let f = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
            .open(&self.path)?;
        if !f.metadata()?.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "sample source is not a regular file",
            ));
        }
        self.file = Some(f);
        Ok(())
    }
    /// A bounded prefix, used for CPU rows before the large interrupt table.
    pub fn prefix(&mut self) -> io::Result<&str> {
        self.read(false).and_then(text)
    }
    pub fn bytes(&mut self) -> io::Result<&[u8]> {
        self.read(true)
    }
    pub fn text(&mut self) -> io::Result<&str> {
        self.read(true).and_then(text)
    }
    fn read(&mut self, complete: bool) -> io::Result<&[u8]> {
        if self.verify_unlinked
            && self
                .file
                .as_ref()
                .is_some_and(|f| f.metadata().is_ok_and(|m| m.nlink() == 0))
        {
            self.invalidate();
        }
        let mut attempt = 0;
        let length = loop {
            if self.file.is_none() {
                self.open()?;
            }
            let f = self.file.as_ref().unwrap();
            let mut offset = 0;
            let result = loop {
                let n = match f.read_at(&mut self.bytes[offset..], offset as u64) {
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    other => other,
                };
                match n {
                    Err(e) => break Err(e),
                    Ok(0) => break Ok(offset),
                    Ok(n) => {
                        offset += n;
                        if !complete {
                            break Ok(offset);
                        }
                        if offset == self.bytes.len() {
                            let mut extra = [0u8; 1];
                            break match f.read_at(&mut extra, offset as u64) {
                                Ok(0) => Ok(offset),
                                Ok(_) if self.bytes.len() < self.limit => {
                                    self.bytes.resize((self.bytes.len() * 2).min(self.limit), 0);
                                    continue;
                                }
                                Ok(_) => Err(io::Error::new(
                                    io::ErrorKind::InvalidData,
                                    "sample exceeds bounded buffer",
                                )),
                                Err(e) => Err(e),
                            };
                        }
                    }
                }
            };
            match result {
                Ok(n) => break n,
                Err(e) => {
                    self.invalidate();
                    if attempt != 0 {
                        return Err(e);
                    }
                    attempt += 1;
                }
            }
        };
        if !self.keep_open {
            self.file = None;
        }
        Ok(&self.bytes[..length])
    }
}
fn text(bytes: &[u8]) -> io::Result<&str> {
    std::str::from_utf8(bytes)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "sample is not UTF-8"))
}

/// Retained proc directory descriptor and getdents buffer for reconciliation.
pub struct PidDirectory {
    dir: File,
    bytes: Vec<u8>,
}
impl PidDirectory {
    pub fn new(path: &Path) -> io::Result<Self> {
        let dir = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_CLOEXEC)
            .open(path)?;
        Ok(Self {
            dir,
            bytes: vec![0; 32768],
        })
    }
    pub fn unlinked(&self) -> bool {
        self.dir.metadata().is_ok_and(|m| m.nlink() == 0)
    }
    pub fn pids(&mut self, out: &mut Vec<i64>) -> io::Result<()> {
        use std::os::fd::AsRawFd;
        if unsafe { libc::lseek(self.dir.as_raw_fd(), 0, libc::SEEK_SET) } < 0 {
            return Err(io::Error::last_os_error());
        }
        loop {
            let n = unsafe {
                libc::syscall(
                    libc::SYS_getdents64,
                    self.dir.as_raw_fd(),
                    self.bytes.as_mut_ptr(),
                    self.bytes.len(),
                )
            };
            if n < 0 {
                let e = io::Error::last_os_error();
                if e.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(e);
            }
            if n == 0 {
                return Ok(());
            }
            let mut offset = 0;
            while offset < n as usize {
                let header = self
                    .bytes
                    .get(offset..offset + 19)
                    .filter(|_| offset + 19 <= n as usize)
                    .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "short dirent"))?;
                let len = u16::from_ne_bytes([header[16], header[17]]) as usize;
                if len < 20 || offset + len > n as usize {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "invalid dirent length",
                    ));
                }
                let raw = &self.bytes[offset + 19..offset + len];
                let name = &raw[..raw.iter().position(|b| *b == 0).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "unterminated dirent")
                })?];
                if let Some(pid) = std::str::from_utf8(name)
                    .ok()
                    .and_then(|s| s.parse::<i32>().ok())
                {
                    out.push(pid as i64)
                }
                offset += len;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn positioned_reads_reuse_and_can_rearm_after_replacement() {
        let path = std::env::temp_dir().join(format!("hyprv-sample-{}", std::process::id()));
        std::fs::write(&path, "first").unwrap();
        let mut f = SampleFile::new(&path, 8).verify_unlinked(true);
        assert_eq!(f.text().unwrap(), "first");
        std::fs::write(&path, "next").unwrap();
        assert_eq!(f.text().unwrap(), "next");
        std::fs::remove_file(&path).unwrap();
        std::fs::write(&path, "again").unwrap();
        assert_eq!(f.text().unwrap(), "again");
        std::fs::write(&path, "123456789").unwrap();
        assert!(f.text().is_err());
        assert_eq!(f.prefix().unwrap(), "12345678");
        std::fs::remove_file(path).unwrap();
    }
}

