use std::cmp::{Ord, Ordering};
use std::error::Error;
use std::ops::Index;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use lscolors::LsColors;
use mime_detective;
use users;
use chrono::TimeZone;


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
    fn index(&self, pos: usize) -> &Self::Output {
        &self.files[pos]
    }
}

fn get_kind(file: &std::fs::DirEntry) -> Kind {
    let file = file.file_type().unwrap();
    if file.is_file() {
        return Kind::File;
    }
    if file.is_dir() {
        return Kind::Directory;
    }
    if file.is_symlink() {
        return Kind::Link;
    }
    Kind::Pipe
}

fn get_color(path: &Path, meta: &std::fs::Metadata) -> Option<lscolors::Color> {
    match COLORS.style_for_path_with_metadata(path, Some(&meta)) {
        Some(style) => style.clone().foreground,
        None => None,
    }
}

impl Files {
    pub fn new_from_path(path: &Path) -> Result<Files, Box<dyn Error>> {
        let direntries: Result<Vec<_>, _> = std::fs::read_dir(&path)?.collect();

        let files: Vec<_> = direntries?
            .iter()
            .map(|file| {
                //let file = file?;
                let name = file.file_name();
                let name = name.to_string_lossy();
                let kind = get_kind(&file);
                let path = file.path();
                let meta = file.metadata().unwrap();
                let mode = meta.mode();
                let size = meta.len();
                let mtime = meta.mtime();
                let user = meta.uid();
                let group = meta.gid();
                let color = get_color(&path, &meta);
                File::new(&name, path, kind, size as usize, mtime, color, mode,
                          user, group)
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

    pub fn sort(&mut self) {
        match self.sort {
            SortBy::Name => self
                .files
                .sort_by(|a, b| alphanumeric_sort::compare_str(&a.name, &b.name)),
            SortBy::Size => {
                self.files.sort_by(|a, b| {
                    if a.size == b.size {
                        return alphanumeric_sort::compare_str(&b.name, &a.name);
                    }
                    a.size.cmp(&b.size).reverse()
                });
            }
            SortBy::MTime => {
                self.files.sort_by(|a, b| {
                    if a.mtime == b.mtime {
                        return alphanumeric_sort::compare_str(&a.name, &b.name);
                    }
                    a.mtime.cmp(&b.mtime)
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
    Link,
    Pipe,
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

#[derive(Debug, PartialEq, Clone)]
pub struct File {
    pub name: String,
    pub path: PathBuf,
    pub size: Option<usize>,
    pub kind: Kind,
    pub mtime: i64,
    pub color: Option<lscolors::Color>,
    pub mode: u32,
    pub user: u32,
    pub group: u32,
    pub selected: bool
    // flags: Option<String>,
}

impl File {
    pub fn new(
        name: &str,
        path: PathBuf,
        kind: Kind,
        size: usize,
        mtime: i64,
        color: Option<lscolors::Color>,
        mode: u32,
        user: u32,
        group: u32
    ) -> File {
        File {
            name: name.to_string(),
            path: path,
            size: Some(size),
            kind: kind,
            mtime: mtime,
            color: color,
            mode: mode,
            user: user,
            group: group,
            selected: false
        }
    }

    pub fn new_from_path(path: &Path) -> Result<File, Box<Error>> {
        let pathbuf = path.to_path_buf();
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or("/".to_string());

        let kind = Kind::Directory; //get_kind(&path);
        let meta = &path.metadata()?;
        let size = meta.len();
        let user = meta.uid();
        let group = meta.gid();
        let color = get_color(&path, meta);
        let mode = meta.mode();
        let mtime = meta.mtime();
        Ok(
            File::new(&name, pathbuf, kind, size as usize, mtime, color, mode, user
                     , group)
        )
    }

    pub fn new_placeholder(path: &Path) -> Result<File, Box<Error>> {
        let mut file = File::new_from_path(path)?;
        file.name = "<empty>".to_string();
        file.kind = Kind::Placeholder;
        Ok(file)
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
            2 => " MB",
            3 => " GB",
            4 => " TB",
            5 => " wtf are you doing",
            _ => "",
        }
        .to_string();
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

    pub fn read_dir(&self) -> Result<Files, Box<Error>> {
        match self.kind {
            Kind::Placeholder => {
                let e: Box<Error>
                    = From::from("placeholder".to_string());
                Err(e)
            },
            _ => Files::new_from_path(&self.path)
        }

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

    pub fn pretty_print_permissions(&self) -> String {
        let perms: usize = format!("{:o}", self.mode).parse().unwrap();
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

        perms
    }

    pub fn pretty_user(&self) -> Option<String> {
        let uid = self.user;
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
        let gid = self.group;
        let file_group = users::get_group_by_gid(gid)?;
        let cur_group = users::get_current_groupname()?;
        let color =
            if file_group.name() == cur_group {
                crate::term::color_green()
            } else {
                crate::term::color_yellow()  };
        Some(format!("{}{}", color, file_group.name().to_string_lossy()))
    }

    pub fn pretty_mtime(&self) -> String {
        //let time = chrono::DateTime::from_timestamp(self.mtime, 0);
        let time: chrono::DateTime<chrono::Local>
            = chrono::Local.timestamp(self.mtime, 0);
        time.format("%F %R").to_string()
    }
}
