use crate::error::{Error, Result};
use memmap2::Mmap;
use std::fs::File;
use std::path::Path;
use std::sync::Arc;

pub enum Source {
    Mapped(Mmap),
    Owned(Arc<[u8]>),
}

impl Source {
    /// Maps the file read-only into memory. The caller must not truncate or rewrite the file while the source is alive.
    pub fn map(path: &Path) -> Result<Source> {
        let file = File::open(path)?;
        if file.metadata()?.len() == 0 {
            return Err(Error::Empty);
        }
        let mapping = unsafe { Mmap::map(&file) }?;
        Ok(Source::Mapped(mapping))
    }

    pub fn read(path: &Path) -> Result<Source> {
        let bytes = std::fs::read(path)?;
        Ok(Source::Owned(bytes.into()))
    }

    pub fn from_bytes(bytes: impl Into<Arc<[u8]>>) -> Source {
        Source::Owned(bytes.into())
    }

    pub fn bytes(&self) -> &[u8] {
        match self {
            Source::Mapped(mapping) => mapping,
            Source::Owned(bytes) => bytes,
        }
    }

    pub fn len(&self) -> usize {
        self.bytes().len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes().is_empty()
    }
}

impl std::fmt::Debug for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let variant = match self {
            Source::Mapped(_) => "Mapped",
            Source::Owned(_) => "Owned",
        };
        write!(f, "Source::{variant}({} bytes)", self.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("cadboss-core-{}-{}", std::process::id(), name))
    }

    #[test]
    fn from_bytes_exposes_the_same_bytes() {
        let source = Source::from_bytes(vec![1u8, 2, 3]);
        assert_eq!(source.bytes(), &[1, 2, 3]);
        assert_eq!(source.len(), 3);
        assert!(!source.is_empty());
        assert_eq!(Source::from_bytes(&[9u8][..]).bytes(), &[9]);
    }

    #[test]
    fn read_copies_the_file() {
        let path = temp_path("read");
        std::fs::write(&path, b"0\nEOF\n").unwrap();
        let source = Source::read(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(source.bytes(), b"0\nEOF\n");
    }

    #[test]
    fn map_exposes_the_file_bytes() {
        let path = temp_path("map");
        std::fs::write(&path, b"0\nEOF\n").unwrap();
        let source = Source::map(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(source.bytes(), b"0\nEOF\n");
    }

    #[test]
    fn map_rejects_an_empty_file_with_error_empty() {
        let path = temp_path("empty");
        std::fs::write(&path, b"").unwrap();
        let result = Source::map(&path);
        std::fs::remove_file(&path).unwrap();
        assert!(matches!(result, Err(crate::Error::Empty)));
    }
}
