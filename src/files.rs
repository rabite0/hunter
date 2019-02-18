use std::cmp::{Ord, Ordering};
use std::ops::Index;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use lscolors::LsColors;
use mime_detective;
use users;
use chrono::TimeZone;
use failure::Error;

use crate::fail::HResult;

use std::sync::{Arc, Mutex};


lazy_static! {
    static ref COLORS: LsColors = LsColors::from_env().unwrap();
}

#[derive(PartialEq, Clone)]
pub struct Files {
    pub directory: File,
    pub files: Vec<File>,
    pub sort: SortBy,
    pub dirs_first: bool,
    pub reverse: bool,
    pub show_hidden: bool
}

impl Index<usize> for Files {
    type Output = File;
    fn index(&self, pos: usize) -> &File {
        &self.files[pos]
    }
}





impl Files {
    pub fn new_from_path(path: &Path) -> Result<Files, Error> {
        let direntries: Result<Vec<_>, _> = std::fs::read_dir(&path)?.collect();

        let files: Vec<_> = direntries?
            .iter()
            .map(|file| {
                let name = file.file_name();
                let name = name.to_string_lossy();
                let path = file.path();
                File::new(&name, path)
            })
            .collect();

        let mut files = Files {
            directory: File::new_from_path(&path)?,
            files: files,
            sort: SortBy::Name,
            dirs_first: true,
            reverse: false,
            show_hidden: true
        };

        files.sort();

        if files.files.len() == 0 {
            files.files = vec![File::new_placeholder(&path)?];
        }

        Ok(files)
    }

    pub fn new_from_path_cancellable(path: &Path, stale: Arc<Mutex<bool>>) -> Result<Files, Error> {
        let direntries: Result<Vec<_>, _> = std::fs::read_dir(&path)?.collect();

        let files: Vec<_> = direntries?
            .iter()
            .map(|file| {
                if crate::preview::is_stale(&stale).unwrap() {
                    None
                } else {
                    let name = file.file_name();
                    let name = name.to_string_lossy();
                    let path = file.path();
                    Some(File::new(&name, path))
                }
            })
            .fuse()
            .flatten()
            .collect();

        if crate::preview::is_stale(&stale).unwrap() {
            return Err(crate::fail::HError::StalePreviewError {
                file: path.to_string_lossy().to_string()
            })?;
        }

        let mut files = Files {
            directory: File::new_from_path(&path)?,
            files: files,
            sort: SortBy::Name,
            dirs_first: true,
            reverse: false,
            show_hidden: true
        };

        files.sort();

        if files.files.len() == 0 {
            files.files = vec![File::new_placeholder(&path)?];
        }

        Ok(files)
    }

    pub fn sort(&mut self) {
        match self.sort {
            SortBy::Name => self
                .files
                .sort_by(|a, b| alphanumeric_sort::compare_str(&a.name, &b.name)),
            SortBy::Size => {
                self.meta_all();
                self.files.sort_by(|a, b| {
                    if a.meta().unwrap().size() == b.meta().unwrap().size() {
                        return alphanumeric_sort::compare_str(&b.name, &a.name);
                    }
                    a.meta().unwrap().size().cmp(&b.meta().unwrap().size()).reverse()
                });
            }
            SortBy::MTime => {
                self.meta_all();
                self.files.sort_by(|a, b| {
                    if a.meta().unwrap().mtime() == b.meta().unwrap().mtime() {
                        return alphanumeric_sort::compare_str(&a.name, &b.name);
                    }
                    a.meta().unwrap().mtime().cmp(&b.meta().unwrap().mtime())
                });
            }
        };

        if self.dirs_first {
            self.files.sort_by(|a, b| {
                if a.is_dir() && !b.is_dir() {
                    Ordering::Less
                } else {
                    Ordering::Equal
                }
            });
            self.files.sort_by(|a, b| {
                if a.name.starts_with(".") && !b.name.starts_with(".") {
                    Ordering::Less
                } else {
                    Ordering::Equal
                }
            });
        }

        if self.reverse {
            self.files.reverse();
        }
    }

    pub fn cycle_sort(&mut self) {
        self.sort = match self.sort {
            SortBy::Name => SortBy::Size,
            SortBy::Size => SortBy::MTime,
            SortBy::MTime => SortBy::Name,
        };
    }

    pub fn reverse_sort(&mut self) {
        self.reverse = !self.reverse
    }

    pub fn toggle_hidden(&mut self) {
        self.show_hidden = !self.show_hidden
    }

    pub fn reload_files(&mut self) {
        let dir = self.directory.clone();
        let files = Files::new_from_path(&dir.path()).unwrap();
        let files = files
            .files
            .into_iter()
            .skip_while(|f| f.name.starts_with(".") && !self.show_hidden )
            .collect();

        self.files = files;
    }

    pub fn meta_all(&mut self) {
        let len = self.files.len();
        self.meta_upto(len);
    }

    pub fn meta_upto(&mut self, to: usize) {
        for file in self.files.iter_mut().take(to) {
            file.get_meta().ok();
        }
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn get_selected(&self) -> Vec<&File> {
        self.files.iter().filter(|f| f.is_selected()).collect()
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Kind {
    Directory,
    File,
    Placeholder
}

impl std::fmt::Display for SortBy {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        let text = match self {
            SortBy::Name => "name",
            SortBy::Size => "size",
            SortBy::MTime => "mtime",
        };
        write!(formatter, "{}", text)
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum SortBy {
    Name,
    Size,
    MTime,
}


impl PartialEq for File {
    fn eq(&self, other: &File) -> bool {
        if self.path == other.path {
            true
        } else {
            false
        }
    }
}

#[derive(Debug, Clone)]
pub struct File {
    pub name: String,
    pub path: PathBuf,
    pub kind: Kind,
    pub color: Option<lscolors::Color>,
    pub meta: Option<std::fs::Metadata>,
    pub selected: bool
    // flags: Option<String>,
}

impl File {
    pub fn new(
        name: &str,
        path: PathBuf,
    ) -> File {
        File {
            name: name.to_string(),
            kind: if path.is_dir() { Kind::Directory } else { Kind::File },
            path: path,
            meta: None,
            color: None,
            selected: false
        }
    }

    pub fn new_from_path(path: &Path) -> Result<File, Error> {
        let pathbuf = path.to_path_buf();
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or("/".to_string());

        Ok(File::new(&name, pathbuf))
    }

    pub fn new_placeholder(path: &Path) -> Result<File, Error> {
        let mut file = File::new_from_path(path)?;
        file.name = "<empty>".to_string();
        file.kind = Kind::Placeholder;
        Ok(file)
    }

    pub fn meta(&self) -> HResult<std::fs::Metadata> {
        match &self.meta {
            Some(meta) => Ok(meta.clone()),
            None => { Ok(std::fs::metadata(&self.path)?) }
        }
    }

    pub fn get_meta(&mut self) -> HResult<()> {
        if let Some(_) = self.meta { return Ok(()) }

        let meta = std::fs::metadata(&self.path)?;
        let color = self.get_color(&meta);

        self.meta = Some(meta);
        self.color = color;
        Ok(())
    }

    fn get_color(&self, meta: &std::fs::Metadata) -> Option<lscolors::Color> {
        match COLORS.style_for_path_with_metadata(&self.path, Some(&meta)) {
            Some(style) => style.clone().foreground,
            None => None,
        }
    }

    pub fn calculate_size(&self) -> HResult<(u64, String)> {
        if self.is_dir() {
            let dir_iterator = std::fs::read_dir(&self.path);
            match dir_iterator {
                Ok(dir_iterator) => return Ok((dir_iterator.count() as u64,
                                               "".to_string())),
                Err(_) => return Ok((0, "".to_string()))
            }
        }


        let mut unit = 0;
        let mut size = self.meta()?.size();
        while size > 1024 {
            size /= 1024;
            unit += 1;
        }
        let unit = match unit {
            0 => "",
            1 => " KB",
            2 => " MB",
            3 => " GB",
            4 => " TB",
            5 => " wtf are you doing",
            _ => "",
        }
        .to_string();
        Ok((size, unit))
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

    pub fn read_dir(&self) -> Result<Files, Error> {
        Files::new_from_path(&self.path)
    }


    pub fn path(&self) -> PathBuf {
        self.path.clone()
    }

    pub fn toggle_selection(&mut self) {
        self.selected = !self.selected
    }

    pub fn is_selected(&self) -> bool {
        self.selected
    }

    pub fn pretty_print_permissions(&self) -> HResult<String> {
        let perms: usize = format!("{:o}", self.meta()?.mode()).parse().unwrap();
        let perms: usize  = perms % 800;
        let perms = format!("{}", perms);

        let r = format!("{}r", crate::term::color_green());
        let w = format!("{}w", crate::term::color_yellow());
        let x = format!("{}x", crate::term::color_red());
        let n = format!("{}-", crate::term::highlight_color());

        let perms = perms.chars().map(|c| match c.to_string().parse().unwrap() {
            1 => format!("{}{}{}", n,n,x),
            2 => format!("{}{}{}", n,w,n),
            3 => format!("{}{}{}", n,w,x),
            4 => format!("{}{}{}", r,n,n),
            5 => format!("{}{}{}", r,n,x),
            6 => format!("{}{}{}", r,w,n),
            7 => format!("{}{}{}", r,w,x),
            _ => format!("---")
        }).collect();

        Ok(perms)
    }

    pub fn pretty_user(&self) -> Option<String> {
        if self.meta().is_err() { return None }
        let uid = self.meta().unwrap().uid();
        let file_user = users::get_user_by_uid(uid)?;
        let cur_user = users::get_current_username()?;
        let color =
            if file_user.name() == cur_user {
                crate::term::color_green()
            } else {
                crate::term::color_yellow()  };
        Some(format!("{}{}", color, file_user.name().to_string_lossy()))
    }

    pub fn pretty_group(&self) -> Option<String> {
        if self.meta().is_err() { return None }
        let gid = self.meta().unwrap().gid();
        let file_group = users::get_group_by_gid(gid)?;
        let cur_group = users::get_current_groupname()?;
        let color =
            if file_group.name() == cur_group {
                crate::term::color_green()
            } else {
                crate::term::color_yellow()  };
        Some(format!("{}{}", color, file_group.name().to_string_lossy()))
    }

    pub fn pretty_mtime(&self) -> Option<String> {
        if self.meta().is_err() { return None }
        //let time = chrono::DateTime::from_timestamp(self.mtime, 0);
        let time: chrono::DateTime<chrono::Local>
            = chrono::Local.timestamp(self.meta().unwrap().mtime(), 0);
        Some(time.format("%F %R").to_string())
    }
}
