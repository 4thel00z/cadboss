use std::fs::File;
use std::io::Read;
use std::path::Path;

use memmap2::Mmap;

use crate::budget::Budget;
use crate::error::{Error, Result};

/// Input bytes: a read-only memory map of a file, or a buffer the source owns exclusively.
pub enum Source {
    Mapped(Mmap),
    Owned(Vec<u8>),
}

impl Source {
    /// Maps the file read-only after checking its length against `budget.max_bytes`. The caller must not truncate or rewrite the file while the source is alive.
    pub fn map(path: &Path, budget: &Budget) -> Result<Source> {
        let file = File::open(path)?;
        let len = file.metadata()?.len();
        if len == 0 {
            return Err(Error::Empty);
        }
        budget.check_bytes(len)?;
        let mapping = unsafe { Mmap::map(&file) }?;
        Ok(Source::Mapped(mapping))
    }

    /// Reads the file into one buffer sized from its length, after checking that length against `budget.max_bytes`; reading stops one byte past the budget.
    pub fn read(path: &Path, budget: &Budget) -> Result<Source> {
        let file = File::open(path)?;
        let len = file.metadata()?.len();
        budget.check_bytes(len)?;
        let mut bytes = Vec::with_capacity(usize::try_from(len).unwrap_or(0));
        file.take(budget.max_bytes.saturating_add(1))
            .read_to_end(&mut bytes)?;
        Ok(Source::Owned(bytes))
    }

    pub fn from_bytes(bytes: impl Into<Vec<u8>>) -> Source {
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

    fn small_budget(max_bytes: u64) -> Budget {
        Budget {
            max_bytes,
            ..Budget::default()
        }
    }

    #[test]
    fn read_fills_one_exactly_sized_buffer() {
        let path = temp_path("read");
        std::fs::write(&path, b"0\nEOF\n").unwrap();
        let source = Source::read(&path, &Budget::default()).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(source.bytes(), b"0\nEOF\n");
        assert!(matches!(&source, Source::Owned(bytes) if bytes.capacity() == bytes.len()));
    }

    #[test]
    fn map_exposes_the_file_bytes() {
        let path = temp_path("map");
        std::fs::write(&path, b"0\nEOF\n").unwrap();
        let source = Source::map(&path, &Budget::default()).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(source.bytes(), b"0\nEOF\n");
    }

    #[test]
    fn map_rejects_an_empty_file_with_error_empty() {
        let path = temp_path("empty");
        std::fs::write(&path, b"").unwrap();
        let result = Source::map(&path, &Budget::default());
        std::fs::remove_file(&path).unwrap();
        assert!(matches!(result, Err(crate::Error::Empty)));
    }

    #[test]
    fn read_and_map_check_max_bytes_from_the_file_length_first() {
        let path = temp_path("budget");
        std::fs::write(&path, b"0\nEOF\n").unwrap();
        let read = Source::read(&path, &small_budget(5));
        let mapped = Source::map(&path, &small_budget(5));
        let fits = Source::read(&path, &small_budget(6));
        std::fs::remove_file(&path).unwrap();
        for result in [read, mapped] {
            assert!(matches!(
                result,
                Err(crate::Error::BudgetExceeded {
                    budget: "max_bytes",
                    limit: 5
                })
            ));
        }
        assert_eq!(fits.unwrap().len(), 6);
    }
}
