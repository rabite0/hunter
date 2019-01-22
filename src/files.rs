use std::ops::Index;
use std::error::Error;
use std::path::PathBuf;
use std::ffi::OsStr;
use std::cmp::{Ord, Ordering};

pub struct Files(Vec<File>);

impl Index<usize> for Files {
    type Output = File;
    fn index(&self, pos: usize) -> &Self::Output {
        &self.0[pos]
    }
}

impl PartialOrd for File {
    fn partial_cmp(&self, other: &File) -> Option<Ordering> {
        Some(alphanumeric_sort::compare_str(&self.name, &other.name))
    }
}

impl Ord for File {
    fn cmp(&self, other: &File) -> Ordering {
        alphanumeric_sort::compare_str(&self.name, &other.name)
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
        let mut dirs = Vec::new();
        for file in std::fs::read_dir(path)? {
            let file = file?;
            let name = file.file_name();
            let name = name.to_string_lossy();
            let kind = get_kind(&file);
            let path = file.path();
            let size = file.metadata()?.len() / 1024;
            let file = File::new(&name, path, kind, size as usize);
            match kind {
                Kind::Directory => dirs.push(file),
                _ => files.push(file),
            }
        }
        files.sort();
        dirs.sort();
        dirs.append(&mut files);
        
        let files = dirs;
        
        Ok(Files(files))
    }
    
    pub fn iter(&self) -> std::slice::Iter<File> {
        self.0.iter()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Kind {
    Directory,
    File,
    Link,
    Pipe
}

#[derive(Debug, PartialEq, Eq)]
pub struct File {
    pub name: String,
    pub path: PathBuf,
    pub size: Option<usize>,
    pub kind: Kind,
    // owner: Option<String>,
    // group: Option<String>,
    // flags: Option<String>,
    // ctime: Option<u32>,
    // mtime: Option<u32>,
}


impl File {
    pub fn new(name: &str, path: PathBuf, kind: Kind, size: usize) -> File {
        File {
            name: name.to_string(),
            path: path,
            size: Some(size),
            kind: kind
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

    pub fn path(&self) -> PathBuf {
        self.path.clone()
    }
}

