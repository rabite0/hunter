use std::ops::Index;
use std::error::Error;
use std::path::PathBuf;
use std::ffi::OsStr;

pub struct Files(Vec<File>);

impl Index<usize> for Files {
    type Output = File;
    fn index(&self, pos: usize) -> &Self::Output {
        &self.0[pos]
    }
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
            let path = file.path();
            let size = file.metadata()?.len() / 1024;
            files.push(File::new(&name, path, size as usize));
        }
        Ok(Files(files))
    }

    
    pub fn iter(&self) -> std::slice::Iter<File> {
        self.0.iter()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }
}

#[derive(Debug)]
pub struct File {
    pub name: String,
    pub path: PathBuf,
    pub size: Option<usize>,
    // owner: Option<String>,
    // group: Option<String>,
    // flags: Option<String>,
    // ctime: Option<u32>,
    // mtime: Option<u32>,
}


impl File {
    pub fn new(name: &str, path: PathBuf, size: usize) -> File {
        File {
            name: name.to_string(),
            path: path,
            size: Some(size),
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

