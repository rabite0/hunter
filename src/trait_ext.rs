use std::path::PathBuf;

use crate::fail::{HResult, MimeError};
use crate::files::File;

// This makes using short-circuiting iterators more convenient
pub trait ExtractResult<T> {
    fn extract(self) -> T;
}

impl<T> ExtractResult<T> for Result<T, T> {
    fn extract(self) -> T {
        match self {
            Ok(val) => val,
            Err(val) => val,
        }
    }
}

// To get MIME from Path without hassle
pub trait PathBufMime {
    fn get_mime(&self) -> HResult<String>;
}

impl PathBufMime for PathBuf {
    fn get_mime(&self) -> HResult<String> {
        let file = File::new_from_path(&self).map_err(|e| MimeError::AccessFailed(Box::new(e)))?;

        file.get_mime()
            .map(|mime| Ok(format!("{}", mime)))
            .map_err(|_| MimeError::NoMimeFound)?
    }
}
