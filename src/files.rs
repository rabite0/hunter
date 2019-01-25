use std::ops::Index;
use std::error::Error;
use std::path::PathBuf;
use std::ffi::OsStr;
use std::cmp::{Ord, Ordering};

use lscolors::{LsColors, Style};

lazy_static! {
    static ref COLORS: LsColors = LsColors::from_env().unwrap();
}

#[derive(PartialEq)]
pub struct Files {
    pub files: Vec<File>,
    pub sort: SortBy,
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
            let style
                = match COLORS.style_for_path_with_metadata(file.path(), Some(&meta)) {
                    Some(style) => Some(style.clone()),
                    None => None
                };
            let file = File::new(&name, path, kind, size as usize, style);
            files.push(file)
        }
                
        let mut files = Files { files: files,
                                sort: SortBy::Name };

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
            _ => {}
        };

        // Direcories first
        self.files.sort_by(|a,b| {
            if a.is_dir() && !b.is_dir() {
                Ordering::Less
            } else { Ordering::Equal }
        });
    }

    pub fn cycle_sort(&mut self) -> SortBy {
        self.sort = match self.sort {
            SortBy::Name => SortBy::Size,
            SortBy::Size => SortBy::Name,
            _ => { SortBy::Name }
        };
        self.sort();
        self.sort
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
            SortBy::MDate => "mdate",
            SortBy::CDate => "cdate"
        };
        write!(formatter, "{}", text)
    }
}

#[derive(Debug,Copy,Clone,PartialEq)]
pub enum SortBy {
    Name,
    Size,
    MDate,
    CDate
}

#[derive(Debug, PartialEq, Clone)]
pub struct File {
    pub name: String,
    pub path: PathBuf,
    pub size: Option<usize>,
    pub kind: Kind,
    pub style: Option<Style>
    // owner: Option<String>,
    // group: Option<String>,
    // flags: Option<String>,
    // ctime: Option<u32>,
    // mtime: Option<u32>,
}


impl File {
    pub fn new(name: &str,
               path: PathBuf,
               kind: Kind,
               size: usize,
               style: Option<Style>) -> File {
        File {
            name: name.to_string(),
            path: path,
            size: Some(size),
            kind: kind,
            style: style
            // owner: None,
            // group: None,
            // flags: None,
            // ctime: None,
            // mtime: None
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

