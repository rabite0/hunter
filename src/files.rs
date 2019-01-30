use std::ops::Index;
use std::error::Error;
use std::path::PathBuf;
use std::ffi::OsStr;
use std::cmp::{Ord, Ordering};
use std::time::SystemTime;

use lscolors::{LsColors, Style};
use mime_detective;

lazy_static! {
    static ref COLORS: LsColors = LsColors::from_env().unwrap();
}

#[derive(PartialEq)]
pub struct Files {
    pub files: Vec<File>,
    pub sort: SortBy,
    pub dirs_first: bool,
}

impl Index<usize> for Files {
    type Output = File;
    fn index(&self, pos: usize) -> &Self::Output {
        &self.files[pos]
    }
}

fn get_kind(file: &std::fs::DirEntry) -> Kind {
    let file = file.file_type().unwrap();
    if file.is_file() { return Kind::File; }
    if file.is_dir() { return Kind::Directory; }
    if file.is_symlink() { return Kind::Link; }
    Kind::Pipe
}

impl Files {
    pub fn new_from_path<S: AsRef<OsStr> + Sized>(path: S)
                                              -> Result<Files, Box<dyn Error>>
    where S: std::convert::AsRef<std::path::Path> {
        let mut files = Vec::new();

        for file in std::fs::read_dir(path)? {
            let file = file?;
            let name = file.file_name();
            let name = name.to_string_lossy();
            let kind = get_kind(&file);
            let path = file.path();
            let meta = file.metadata()?;
            let size = meta.len() / 1024;
            let mtime = meta.modified()?;

            let color
                = match COLORS.style_for_path_with_metadata(file.path(), Some(&meta)) {
                    Some(style) => { style.clone().foreground },
                    None => None
                };
            let file = File::new(&name, path, kind, size as usize, mtime, color);
            files.push(file)
        }

        let mut files = Files { files: files,
                                sort: SortBy::Name,
                                dirs_first: true };

        files.sort();
        Ok(files)
    }

    pub fn sort(&mut self) {
        match self.sort {
            SortBy::Name => {
                self.files.sort_by(|a,b| {
                    alphanumeric_sort::compare_str(&a.name, &b.name)
                })
            },
            SortBy::Size => {
                self.files.sort_by(|a,b| {
                    if a.size == b.size {
                        return alphanumeric_sort::compare_str(&b.name, &a.name)
                    }
                    a.size.cmp(&b.size).reverse()
                });
            },
            SortBy::MTime => {
                self.files.sort_by(|a,b| {
                    if a.mtime == b.mtime {
                        return alphanumeric_sort::compare_str(&a.name, &b.name)
                    }
                    a.mtime.cmp(&b.mtime)
                });
            }
        };

        if self.dirs_first {
            self.files.sort_by(|a,b| {
                if a.is_dir() && !b.is_dir() {
                    Ordering::Less
                } else { Ordering::Equal }
            });
        }
    }

    pub fn cycle_sort(&mut self) {
        self.sort = match self.sort {
            SortBy::Name => SortBy::Size,
            SortBy::Size => SortBy::MTime,
            SortBy::MTime => SortBy::Name
        };
    }

    pub fn iter(&self) -> std::slice::Iter<File> {
        self.files.iter()
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Kind {
    Directory,
    File,
    Link,
    Pipe
}

impl std::fmt::Display for SortBy {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_> )
           -> Result<(), std::fmt::Error>  {
        let text = match self {
            SortBy::Name => "name",
            SortBy::Size => "size",
            SortBy::MTime => "mtime"
        };
        write!(formatter, "{}", text)
    }
}

#[derive(Debug,Copy,Clone,PartialEq)]
pub enum SortBy {
    Name,
    Size,
    MTime,
}

#[derive(Debug, PartialEq, Clone)]
pub struct File {
    pub name: String,
    pub path: PathBuf,
    pub size: Option<usize>,
    pub kind: Kind,
    pub mtime: SystemTime,
    pub color: Option<lscolors::Color>,
    // owner: Option<String>,
    // group: Option<String>,
    // flags: Option<String>,
}


impl File {
    pub fn new(name: &str,
               path: PathBuf,
               kind: Kind,
               size: usize,
               mtime: SystemTime,
               color: Option<lscolors::Color>) -> File {
        File {
            name: name.to_string(),
            path: path,
            size: Some(size),
            kind: kind,
            mtime: mtime,
            color: color
            // owner: None,
            // group: None,
            // flags: None,
        }
    }
    pub fn calculate_size(&self) -> (usize, String) {
        let mut unit = 0;
        let mut size = self.size.unwrap();
        while size > 1024 {
            size /= 1024;
            unit += 1;
        }
        let unit = match unit {
            0 => "",
            1 => " KB",
            2 => " GB",
            3 => " TB",
            4 => "wtf are you doing",
            _ => ""
        }.to_string();
        (size, unit)
    }

    pub fn get_mime(&self) -> Option<String> {
        let detective = mime_detective::MimeDetective::new().ok()?;
        let mime = detective.detect_filepath(&self.path).ok()?;
        Some(mime.type_().as_str().to_string())
    }

    pub fn grand_parent(&self) -> Option<PathBuf> {
        Some(self.path.parent()?.parent()?.to_path_buf())
    }

    pub fn is_dir(&self) -> bool {
        self.kind == Kind::Directory
    }

    pub fn path(&self) -> PathBuf {
        self.path.clone()
    }
}
