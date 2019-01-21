use std::ops::Index;

pub struct Files(Vec<File>);

impl Index<usize> for Files {
    type Output = File;
    fn index(&self, pos: usize) -> &Self::Output {
        &self.0[pos]
    }
}

impl Files {
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
    pub path: String,
    pub size: Option<usize>,
    // owner: Option<String>,
    // group: Option<String>,
    // flags: Option<String>,
    // ctime: Option<u32>,
    // mtime: Option<u32>,
}


impl File {
    pub fn new(name: &str, path: &str, size: usize) -> File {
        File {
            name: name.to_string(),
            path: path.to_string(),
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
}

pub fn get_files(dir: &str) -> Result<Files, std::io::Error> {
    let mut files = Vec::new();
    for file in std::fs::read_dir(dir)? {
        let name = file.as_ref().unwrap().file_name().into_string().unwrap();
        file.as_ref().unwrap().path().pop();
        let path = file.as_ref().unwrap().path().into_os_string().into_string().unwrap();
        let size = file.unwrap().metadata()?.len() / 1024;
        files.push(File::new(&name, &path, size as usize));
    }
    Ok(Files(files))
}


