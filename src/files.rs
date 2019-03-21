use std::cmp::{Ord, Ordering};
use std::ops::Index;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::hash::{Hash, Hasher};
use std::os::unix::ffi::{OsStringExt, OsStrExt};
use std::ffi::{OsStr, OsString};

use lscolors::LsColors;
use mime_detective;
use users::{get_current_username,
            get_current_groupname,
            get_user_by_uid,
            get_group_by_gid};
use chrono::TimeZone;
use failure::Error;
use notify::DebouncedEvent;

use crate::fail::{HResult, HError};
use crate::dirty::{DirtyBit, Dirtyable};




lazy_static! {
    static ref COLORS: LsColors = LsColors::from_env().unwrap();
    static ref TAGS: Mutex<(bool, Vec<PathBuf>)> = Mutex::new((false, vec![]));
}

pub fn load_tags() -> HResult<()> {
    std::thread::spawn(|| -> HResult<()> {
        let tag_path = crate::paths::tagfile_path()?;
        let tags = std::fs::read_to_string(tag_path)?;
        let mut tags = tags.lines().map(|f| PathBuf::from(f)).collect::<Vec<PathBuf>>();
        let mut tag_lock = TAGS.lock()?;
        tag_lock.0 = true;
        tag_lock.1.append(&mut tags);
        Ok(())
    });
    Ok(())
}

pub fn check_tag(path: &PathBuf) -> HResult<bool> {
    tags_loaded()?;
    let tagged = TAGS.lock()?.1.contains(path);
    Ok(tagged)
}

pub fn tags_loaded() -> HResult<()> {
    let loaded = TAGS.lock()?.0;
    if loaded { Ok(()) }
    else { HError::tags_not_loaded() }
}

#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub struct Files {
    pub directory: File,
    pub files: Vec<File>,
    pub sort: SortBy,
    pub dirs_first: bool,
    pub reverse: bool,
    pub show_hidden: bool,
    pub filter: Option<String>,
    pub dirty: DirtyBit
}

impl Index<usize> for Files {
    type Output = File;
    fn index(&self, pos: usize) -> &File {
        &self.files[pos]
    }
}


impl Dirtyable for Files {
    fn get_bit(&self) -> &DirtyBit {
        &self.dirty
    }

    fn get_bit_mut(&mut self) -> &mut DirtyBit {
        &mut self.dirty
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
            show_hidden: true,
            filter: None,
            dirty: DirtyBit::new()
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
            show_hidden: true,
            filter: None,
            dirty: DirtyBit::new()
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
        self.set_dirty();
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
        self.set_dirty();
    }

    pub fn handle_event(&mut self, event: &DebouncedEvent) -> HResult<()> {
        match event {
            DebouncedEvent::Create(path) => {
                self.path_in_here(&path)?;
                let file = File::new_from_path(&path)?;
                self.files.push(file);
            },
            DebouncedEvent::Write(path) | DebouncedEvent::Chmod(path) => {
                self.path_in_here(&path)?;
                let file = self.find_file_with_path(&path)?;
                file.reload_meta()?;
            },
            DebouncedEvent::Remove(path) => {
                self.path_in_here(&path)?;
                let file = self.find_file_with_path(&path)?.clone();
                self.files.remove_item(&file);
            },
            DebouncedEvent::Rename(old_path, new_path) => {
                self.path_in_here(&new_path)?;
                let mut file = self.find_file_with_path(&old_path)?;
                file.name = new_path.file_name()?.to_string_lossy().to_string();
                file.path = new_path.into();
            },
            DebouncedEvent::Error(err, path) => {
                dbg!(err);
                dbg!(path);
            },
            _ => {},
        }
        self.set_dirty();
        Ok(())
    }

    pub fn path_in_here(&self, path: &Path) -> HResult<bool> {
        let dir = self.directory.path();
        let path = if path.is_dir() { path } else { path.parent().unwrap() };
        if dir == path {
            Ok(true)
        } else {
            HError::wrong_directory(path.into(), dir)?
        }
    }

    pub fn find_file_with_path(&mut self, path: &Path) -> Option<&mut File> {
        self.files.iter_mut().find(|file| file.path == path)
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

    pub fn set_filter(&mut self, filter: Option<String>) {
        self.filter = filter;
        self.set_dirty();
    }

    pub fn get_filter(&self) -> Option<String> {
        self.filter.clone()
    }

    pub fn len(&self) -> usize {
        match &self.filter {
            None => self.files.len(),
            Some(filter) => {
                self.files
                    .iter()
                    .filter(|f| f.name.contains(filter))
                    .count()
            }
        }
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

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
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

impl Hash for File {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state);
        self.path.hash(state);
        self.selected.hash(state);
    }
}

impl Eq for File {}

#[derive(Debug, Clone)]
pub struct File {
    pub name: String,
    pub path: PathBuf,
    pub kind: Kind,
    pub target: Option<PathBuf>,
    pub color: Option<lscolors::Color>,
    pub meta: Option<std::fs::Metadata>,
    pub selected: bool,
    pub tag: Option<bool>
    // flags: Option<String>,
}

impl File {
    pub fn new(
        name: &str,
        path: PathBuf,
    ) -> File {
        let tag = check_tag(&path).ok();

        File {
            name: name.to_string(),
            kind: if path.is_dir() { Kind::Directory } else { Kind::File },
            path: path,
            target: None,
            meta: None,
            color: None,
            selected: false,
            tag: tag,
        }
    }

    pub fn new_from_path(path: &Path) -> HResult<File> {
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
            None => { Ok(std::fs::symlink_metadata(&self.path)?) }
        }
    }

    pub fn get_meta(&mut self) -> HResult<()> {
        if let Some(_) = self.meta { return Ok(()) }

        let meta = std::fs::symlink_metadata(&self.path)?;
        let color = self.get_color(&meta);
        let target = if meta.file_type().is_symlink() {
            self.path.read_link().ok()
        } else { None };

        self.meta = Some(meta);
        self.color = color;
        self.target = target;
        Ok(())
    }

    pub fn reload_meta(&mut self) -> HResult<()> {
        self.meta = None;
        self.get_meta()
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


    pub fn parent(&self) -> Option<PathBuf> {
        Some(self.path.parent()?.to_path_buf())
    }

    pub fn parent_as_file(&self) -> HResult<File> {
        let pathbuf = self.parent()?;
        File::new_from_path(&pathbuf)
    }

    pub fn grand_parent(&self) -> Option<PathBuf> {
        Some(self.path.parent()?.parent()?.to_path_buf())
    }

    pub fn grand_parent_as_file(&self) -> HResult<File> {
        let pathbuf = self.grand_parent()?;
        File::new_from_path(&pathbuf)
    }

    pub fn is_dir(&self) -> bool {
        self.kind == Kind::Directory
    }

    pub fn read_dir(&self) -> Result<Files, Error> {
        Files::new_from_path(&self.path)
    }

    pub fn strip_prefix(&self, base: &File) -> PathBuf {
        if self == base {
            return PathBuf::from("./");
        }

        let base_path = base.path.clone();
        match self.path.strip_prefix(base_path) {
            Ok(path) => PathBuf::from(path),
            Err(_) => self.path.clone()
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

    pub fn is_tagged(&self) -> HResult<bool> {
        if let Some(tag) = self.tag {
            return Ok(tag);
        }
        let tag = check_tag(&self.path)?;
        Ok(tag)
    }

    pub fn toggle_tag(&mut self) -> HResult<()> {
        let new_state = match self.tag {
            Some(tag) => !tag,
            None => {
                let tag = check_tag(&self.path);
                !tag?
            }
        };
        self.tag = Some(new_state);

        match new_state {
            true => TAGS.lock()?.1.push(self.path.clone()),
            false => { TAGS.lock()?.1.remove_item(&self.path); },
        }
        self.save_tags()?;
        Ok(())
    }

    pub fn save_tags(&self) -> HResult<()> {
        std::thread::spawn(|| -> HResult<()> {
            let tagfile_path = crate::paths::tagfile_path()?;
            let tags = TAGS.lock()?.clone();
            let tags_str = tags.1.iter().map(|p| {
                let path = p.to_string_lossy().to_string();
                format!("{}\n", path)
            }).collect::<String>();
            std::fs::write(tagfile_path, tags_str)?;
            Ok(())
        });
        Ok(())
    }

    pub fn is_readable(&self) -> HResult<bool> {
        let meta = self.meta()?;
        let current_user = get_current_username()?.to_string_lossy().to_string();
        let current_group = get_current_groupname()?.to_string_lossy().to_string();
        let file_user = get_user_by_uid(meta.uid())?
            .name()
            .to_string_lossy()
            .to_string();
        let file_group = get_group_by_gid(meta.gid())?
            .name()
            .to_string_lossy()
            .to_string();
        let perms = meta.mode();

        let user_readable = perms & 0o400;
        let group_readable = perms & 0o040;
        let other_readable = perms & 0o004;

        if current_user == file_user && user_readable > 0 {
            Ok(true)
        } else if current_group == file_group && group_readable > 0 {
            Ok(true)
        } else if other_readable > 0 {
            Ok(true)
        } else {
            Ok(false)
        }
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

    pub fn short_path(&self) -> PathBuf {
        self.path.short_path()
    }

    pub fn short_string(&self) -> String {
        self.path.short_string()
    }
}


pub trait PathBufExt {
    fn short_path(&self) -> PathBuf;
    fn short_string(&self) -> String;
    fn name_starts_with(&self, pat: &str) -> bool;
    fn quoted_file_name(&self) -> Option<OsString>;
    fn quoted_path(&self) -> OsString;
}

impl PathBufExt for PathBuf {
    fn short_path(&self) -> PathBuf {
        if let Ok(home) = crate::paths::home_path() {
            if let Ok(short) = self.strip_prefix(home) {
                let mut path = PathBuf::from("~");
                path.push(short);
                return path
            }
        }
        return self.clone();
    }

    fn short_string(&self) -> String {
        self.short_path().to_string_lossy().to_string()
    }

    fn name_starts_with(&self, pat: &str) -> bool {
        if let Some(name) = self.file_name() {
            let nbytes = name.as_bytes();
            let pbytes = pat.as_bytes();

            if nbytes.starts_with(pbytes) {
                return true;
            } else {
                return false;
            }
        }
        false
    }

    fn quoted_file_name(&self) -> Option<OsString> {
        if let Some(name) = self.file_name() {
            let mut name = name.as_bytes().to_vec();
            let mut quote = "\"".as_bytes().to_vec();
            //let mut quote_after = "\"".as_bytes().to_vec();
            let mut quoted = vec![];
            quoted.append(&mut quote.clone());
            quoted.append(&mut name);
            quoted.append(&mut quote);

            let quoted_name = OsStr::from_bytes(&quoted).to_os_string();
            return Some(quoted_name);
        }
        None
    }

    fn quoted_path(&self) -> OsString {
        let mut path = self.clone().into_os_string().into_vec();
        let mut quote = "\"".as_bytes().to_vec();

        let mut quoted = vec![];
        quoted.append(&mut quote.clone());
        quoted.append(&mut path);
        quoted.append(&mut quote);

        OsString::from_vec(quoted)
    }
}

pub trait OsStrTools {
    fn split(&self, pat: &OsStr) -> Vec<OsString>;
    fn replace(&self, from: &OsStr, to: &OsStr) -> OsString;
    fn trim_last_space(&self) -> OsString;
    fn contains_osstr(&self, pat: &OsStr) -> bool;
    fn position(&self, pat: &OsStr) -> Option<usize>;
    fn splice_quoted(&self, from: &OsStr, to: Vec<OsString>) -> Vec<OsString>;
    fn splice_with(&self, from: &OsStr, to: Vec<OsString>) -> Vec<OsString>;
    fn quote(&self) -> OsString;
}

impl OsStrTools for OsStr {
    fn split(&self, pat: &OsStr) -> Vec<OsString> {
        let orig_string = self.as_bytes().to_vec();
        let pat = pat.as_bytes().to_vec();
        let pat_len = pat.len();

        dbg!(&self);

        let split_string = orig_string
            .windows(pat_len)
            .enumerate()
            .fold(Vec::new(), |mut split_pos, (i, chars)| {
                dbg!(&chars);
                dbg!(&split_pos);
                if chars == pat.as_slice() {
                    if split_pos.len() == 0 {
                        split_pos.push((0, i));
                    } else {
                        let len = split_pos.len();
                        let last_split = split_pos[len-1].1;
                        split_pos.push((last_split, i));
                    }
                }
                split_pos
            }).iter()
            .map(|(start, end)| {
                //let orig_string = orig_string.clone();
                OsString::from_vec(orig_string[*start..*end]
                                   .to_vec()).replace(&OsString::from_vec(pat.clone()),
                                                      &OsString::from(""))
            }).collect();
        split_string
    }


    fn quote(&self) -> OsString {
        let mut string = self.as_bytes().to_vec();
        let mut quote = "\"".as_bytes().to_vec();

        let mut quoted = vec![];
        quoted.append(&mut quote.clone());
        quoted.append(&mut string);
        quoted.append(&mut quote);

        OsString::from_vec(quoted)
    }

    fn splice_quoted(&self, from: &OsStr, to: Vec<OsString>) -> Vec<OsString> {
        let quoted_to = to.iter()
            .map(|to| to.quote())
            .collect();
        self.splice_with(from, quoted_to)
    }

    fn splice_with(&self, from: &OsStr, to: Vec<OsString>) -> Vec<OsString> {
        let pos = self.position(from);

        if pos.is_none() {
            return vec![OsString::from(self)];
        }

        dbg!(&self);

        let pos = pos.unwrap();
        let string = self.as_bytes().to_vec();
        let from = from.as_bytes().to_vec();
        let fromlen = from.len();

        let lpart = OsString::from_vec(string[0..pos].to_vec());
        let rpart = OsString::from_vec(string[pos+fromlen..].to_vec());

        dbg!(&lpart);
        dbg!(&rpart);

        let mut result = vec![
            vec![lpart.trim_last_space()],
            to,
            vec![rpart]
        ].into_iter()
            .flatten()
            .filter(|part| part.len() != 0)
            .collect::<Vec<OsString>>();

        if result.last() == Some(&OsString::from("")) {
            result.pop();
            result
        } else { result }
    }

    fn replace(&self, from: &OsStr, to: &OsStr) -> OsString {
        let orig_string = self.as_bytes().to_vec();
        let from = from.as_bytes();
        let to = to.as_bytes().to_vec();
        let from_len = from.len();

        let new_string = orig_string
            .windows(from_len)
            .enumerate()
            .fold(Vec::new(), |mut pos, (i, chars)| {
                if chars == from {
                    pos.push(i);
                }
                pos
            }).iter().rev().fold(orig_string.to_vec(), |mut string, pos| {
                let pos = *pos;
                string.splice(pos..pos+from_len, to.clone());
                string
            });

        OsString::from_vec(new_string)
    }

    fn trim_last_space(&self) -> OsString {
        let string = self.as_bytes();
        let len = string.len();

        if len > 0 {
            OsString::from_vec(string[..len-1].to_vec())
        } else {
            self.to_os_string()
        }
    }

    fn contains_osstr(&self, pat: &OsStr) -> bool {
        let string = self.as_bytes();
        let pat = pat.as_bytes();
        let pat_len = pat.len();

        string.windows(pat_len)
            .find(|chars|
                  chars == &pat
            ).is_some()
    }

    fn position(&self, pat: &OsStr) -> Option<usize> {
        let string = self.as_bytes();
        let pat = pat.as_bytes();
        let pat_len = pat.len();

        string.windows(pat_len)
            .position(|chars|
                      chars == pat
            )
    }
}
