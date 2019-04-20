use std::cmp::{Ord, Ordering};
use std::ops::Index;
use std::fs::Metadata;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::sync::mpsc::Sender;
use std::hash::{Hash, Hasher};

use lscolors::LsColors;
use tree_magic;
use users::{get_current_username,
            get_current_groupname,
            get_user_by_uid,
            get_group_by_gid};
use chrono::TimeZone;
use failure::Error;
use notify::DebouncedEvent;
use rayon::{ThreadPool, ThreadPoolBuilder};
use alphanumeric_sort::compare_str;
use pathbuftools::PathBufTools;

use crate::fail::{HResult, HError, ErrorLog};
use crate::dirty::{AsyncDirtyBit, DirtyBit, Dirtyable};
use crate::preview::{Async, Stale};
use crate::widget::Events;


lazy_static! {
    static ref COLORS: LsColors = LsColors::from_env().unwrap_or_default();
    static ref TAGS: RwLock<(bool, Vec<PathBuf>)> = RwLock::new((false, vec![]));
}

fn make_pool(sender: Option<Sender<Events>>) -> ThreadPool {
    let sender = Arc::new(Mutex::new(sender));
    ThreadPoolBuilder::new()
        .num_threads(8)
        .exit_handler(move |thread_num| {
            if thread_num == 0 {
                if let Ok(lock) = sender.lock() {
                    if let Some(sender) = lock.as_ref() {
                        sender.send(Events::WidgetReady).ok();
                    }
                }
            }
        })
        .build()
        .expect("Failed to create thread pool")
}

pub fn load_tags() -> HResult<()> {
    std::thread::spawn(|| -> HResult<()> {
        let tag_path = crate::paths::tagfile_path()?;

        if !tag_path.exists() {
            import_tags().log();
        }

        let tags = std::fs::read_to_string(tag_path)?;
        let mut tags = tags.lines()
            .map(|f|
                 PathBuf::from(f))
            .collect::<Vec<PathBuf>>();
        let mut tag_lock = TAGS.write()?;
        tag_lock.0 = true;
        tag_lock.1.append(&mut tags);
        Ok(())
    });
    Ok(())
}

pub fn import_tags() -> HResult<()> {
    let mut ranger_tags = crate::paths::ranger_path()?;
    ranger_tags.push("tagged");

    if ranger_tags.exists() {
        let tag_path = crate::paths::tagfile_path()?;
        std::fs::copy(ranger_tags, tag_path)?;
    }
    Ok(())
}

pub fn check_tag(path: &PathBuf) -> HResult<bool> {
    tags_loaded()?;
    let tagged = TAGS.read()?.1.contains(path);
    Ok(tagged)
}

pub fn tags_loaded() -> HResult<()> {
    let loaded = TAGS.read()?.0;
    if loaded { Ok(()) }
    else { HError::tags_not_loaded() }
}


#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub struct Files {
    pub directory: File,
    pub files: Vec<File>,
    pub meta_upto: Option<usize>,
    pub meta_updated: bool,
    pub sort: SortBy,
    pub dirs_first: bool,
    pub reverse: bool,
    pub show_hidden: bool,
    pub filter: Option<String>,
    pub dirty: DirtyBit,
    pub dirty_meta: AsyncDirtyBit,
}

impl Index<usize> for Files {
    type Output = File;
    fn index(&self, pos: usize) -> &File {
        &self.files[pos]
    }
}


impl Dirtyable for Files {
    fn is_dirty(&self) -> bool {
        self.dirty.is_dirty()
    }

    fn set_dirty(&mut self) {
        self.dirty.set_dirty();
    }

    fn set_clean(&mut self) {
        self.dirty.set_clean();
    }
}


impl Files {
    pub fn new_from_path(path: &Path) -> Result<Files, Error> {
        let direntries: Result<Vec<_>, _> = std::fs::read_dir(&path)?.collect();
        let dirty = DirtyBit::new();
        let dirty_meta = AsyncDirtyBit::new();

        let files: Vec<_> = direntries?
            .iter()
            .map(|file| {
                let name = file.file_name();
                let name = name.to_string_lossy();
                let path = file.path();
                File::new(&name,
                          path,
                          Some(dirty_meta.clone()))
            })
            .collect();

        let mut files = Files {
            directory: File::new_from_path(&path, None)?,
            files: files,
            meta_upto: None,
            meta_updated: false,
            sort: SortBy::Name,
            dirs_first: true,
            reverse: false,
            show_hidden: true,
            filter: None,
            dirty: dirty,
            dirty_meta: dirty_meta,
        };

        files.sort();



        if files.files.len() == 0 {
            files.files = vec![File::new_placeholder(&path)?];
        }

        Ok(files)
    }

    pub fn new_from_path_cancellable(path: &Path,
                                     stale: Stale)
                                     -> Result<Files, Error> {
        let direntries: Result<Vec<_>, _> = std::fs::read_dir(&path)?.collect();
        let dirty = DirtyBit::new();
        let dirty_meta = AsyncDirtyBit::new();

        let files: Vec<_> = direntries?
            .iter()
            .map(|file| {
                if crate::preview::is_stale(&stale).unwrap() {
                    None
                } else {
                    let name = file.file_name();
                    let name = name.to_string_lossy();
                    let path = file.path();
                    Some(File::new_with_stale(&name,
                                              path,
                                              Some(dirty_meta.clone()),
                                              stale.clone()))
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
            directory: File::new_from_path(&path, None)?,
            files: files,
            meta_upto: None,
            meta_updated: false,
            sort: SortBy::Name,
            dirs_first: true,
            reverse: false,
            show_hidden: true,
            filter: None,
            dirty: dirty,
            dirty_meta: dirty_meta,
        };

        files.sort();

        if files.files.len() == 0 {
            files.files = vec![File::new_placeholder(&path)?];
        }

        Ok(files)
    }

    pub fn get_file_mut(&mut self, index: usize) -> Option<&mut File> {
        let filter = self.filter.clone();
        let show_hidden = self.show_hidden;

        let file = self.files
            .iter_mut()
            .filter(|f| !(filter.is_some() &&
                         !f.name.contains(filter.as_ref().unwrap())))
            .filter(|f| !(!show_hidden && f.name.starts_with(".")))
            .nth(index);
        file
    }

    pub fn get_files(&self) -> Vec<&File> {
        self.files
            .iter()
            .filter(|f| !(self.filter.is_some() &&
                         !f.name.contains(self.filter.as_ref().unwrap())))
            .filter(|f| !(!self.show_hidden && f.name.starts_with(".")))
            .collect()
    }

    pub fn get_files_mut(&mut self) -> Vec<&mut File> {
        let filter = self.filter.clone();
        let show_hidden = self.show_hidden;
        self.files
            .iter_mut()
            .filter(|f| !(filter.is_some() &&
                         !f.name.contains(filter.as_ref().unwrap())))
            .filter(|f| !(!show_hidden && f.name.starts_with(".")))
            .collect()
    }

    pub fn sort(&mut self) {
        match self.sort {
            SortBy::Name => self
                .files
                .sort_by(|a, b| {
                    compare_str(&a.name, &b.name)
                }),
            SortBy::Size => {
                self.meta_all_sync().log();
                self.files.sort_by(|a, b| {
                    match (a.meta(), b.meta()) {
                        (Ok(a_meta), Ok(b_meta)) => {
                            if a_meta.size() == b_meta.size() {
                                compare_str(&b.name, &a.name)
                            } else {
                                a_meta.size().cmp(&b_meta.size()).reverse()
                            }

                        }
                        _ => return std::cmp::Ordering::Equal
                    }


                });
            }
            SortBy::MTime => {
                self.meta_all_sync().log();
                self.files.sort_by(|a, b| {
                    match (a.meta(), b.meta()) {
                        (Ok(a_meta), Ok(b_meta)) => {
                            if a_meta.mtime() == b_meta.mtime() {
                                compare_str(&b.name, &a.name)
                            } else {
                                a_meta.mtime().cmp(&b_meta.mtime()).reverse()
                            }

                        }
                        _ => return std::cmp::Ordering::Equal
                    }
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
        self.show_hidden = !self.show_hidden;
        self.set_dirty();

        if self.show_hidden == true {
            self.remove_placeholder();
        }
    }

    pub fn replace_file(&mut self,
                        old: Option<&File>,
                        new: Option<File>) -> HResult<()> {
        let (tag, selected) = if let Some(old) = old {
            if let Some(old) = self.find_file_with_path(&old.path) {
                (old.tag, old.selected)
            } else {
                (None, false)
            }
        } else {
            (None, false)
        };
        old.map(|old| self.files.remove_item(old));
        new.map(|mut new| {
            new.tag = tag;
            new.selected = selected;
            self.files.push(new);
        });

        self.sort();

        if self.len() == 0 {
            let placeholder = File::new_placeholder(&self.directory.path)?;
            self.files.push(placeholder);
        } else {
            self.remove_placeholder();
        }

        Ok(())
    }

    fn remove_placeholder(&mut self) {
        let dirpath = self.directory.path.clone();
        self.find_file_with_path(&dirpath).cloned()
            .map(|placeholder| self.files.remove_item(&placeholder));
    }

    pub fn handle_event(&mut self,
                        event: &DebouncedEvent) -> HResult<()> {
        match event {
            DebouncedEvent::Create(path) => {
                self.path_in_here(&path)?;
                let file = File::new_from_path(&path,
                                               Some(self.dirty_meta.clone()))?;
                self.files.push(file);
                self.sort();
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
                file.reload_meta()?;
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
        let dir = &self.directory.path;
        let path = if path.is_dir() { path } else { path.parent().unwrap() };
        if dir == path {
            Ok(true)
        } else {
            HError::wrong_directory(path.into(), dir.to_path_buf())?
        }
    }

    pub fn find_file_with_path(&mut self, path: &Path) -> Option<&mut File> {
        self.files.iter_mut().find(|file| file.path == path)
    }

    pub fn meta_all_sync(&mut self) -> HResult<()> {
        for file in self.files.iter_mut() {
            if !file.meta_processed {
                file.meta_sync().log();
            }
        }
        self.set_dirty();
        self.meta_updated = true;
        Ok(())
    }

    pub fn meta_all(&mut self) {
        let len = self.len();
        self.meta_upto(len, None);
    }

    pub fn meta_upto(&mut self, to: usize, sender: Option<Sender<Events>>) {
        let meta_files = if self.meta_upto > Some(to) {
            self.meta_upto.unwrap()
        } else {
            if to > self.len() {
                self.len()
            } else {
                to
            }
        };

        if self.meta_upto >= Some(meta_files) && !self.dirty_meta.is_dirty() { return }

        self.set_dirty();
        self.dirty_meta.set_clean();

        let meta_pool = make_pool(sender.clone());
        let show_hidden = self.show_hidden;

        for file in self.files
            .iter_mut()
            .filter(|f| !(!show_hidden && f.name.starts_with(".")))
            .take(meta_files) {
            if !file.meta_processed {
                file.take_meta(&meta_pool, &mut self.meta_updated).ok();
            }
            if file.is_dir() {
                file.take_dirsize(&meta_pool, &mut self.meta_updated).ok();
            }
        }

        self.meta_upto = Some(meta_files);
    }

    pub fn meta_set_fresh(&self) -> HResult<()> {
        self.files.get(0)?.meta.set_fresh()?;
        Ok(())
    }


    pub fn set_filter(&mut self, filter: Option<String>) {
        self.filter = filter;
        self.set_dirty();
    }

    pub fn get_filter(&self) -> Option<String> {
        self.filter.clone()
    }

    pub fn len(&self) -> usize {
        self.get_files().len()
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
    }
}

impl Eq for File {}


#[derive(Clone, Debug)]
pub struct File {
    pub name: String,
    pub path: PathBuf,
    pub kind: Kind,
    pub dirsize: Option<Async<usize>>,
    pub target: Option<PathBuf>,
    pub color: Option<lscolors::Color>,
    pub meta: Async<Metadata>,
    pub dirty_meta: Option<AsyncDirtyBit>,
    pub meta_processed: bool,
    pub selected: bool,
    pub tag: Option<bool>
}

impl File {
    pub fn new(
        name: &str,
        path: PathBuf,
        dirty_meta: Option<AsyncDirtyBit>) -> File {
        let tag = check_tag(&path).ok();
        let meta = File::make_async_meta(&path, dirty_meta.clone(), None);
        let dirsize = if path.is_dir() {
            Some(File::make_async_dirsize(&path, dirty_meta.clone(), None))
        } else { None };

        File {
            name: name.to_string(),
            kind: if path.is_dir() { Kind::Directory } else { Kind::File },
            path: path,
            dirsize: dirsize,
            target: None,
            meta: meta,
            meta_processed: false,
            dirty_meta: dirty_meta,
            color: None,
            selected: false,
            tag: tag,
        }
    }

    pub fn new_with_stale(name: &str,
                          path: PathBuf,
                          dirty_meta: Option<AsyncDirtyBit>,
                          stale: Stale) -> File {
        let tag = check_tag(&path).ok();
        let meta = File::make_async_meta(&path,
                                         dirty_meta.clone(),
                                         Some(stale.clone()));
        let dirsize = if path.is_dir() {
            Some(File::make_async_dirsize(&path,
                                          dirty_meta.clone(),
                                          Some(stale)))
        } else { None };

        File {
            name: name.to_string(),
            kind: if path.is_dir() { Kind::Directory } else { Kind::File },
            path: path,
            dirsize: dirsize,
            target: None,
            meta: meta,
            meta_processed: false,
            dirty_meta: dirty_meta,
            color: None,
            selected: false,
            tag: tag,
        }
    }

    pub fn new_from_path(path: &Path,
                         dirty_meta: Option<AsyncDirtyBit>) -> HResult<File> {
        let pathbuf = path.to_path_buf();
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or("/".to_string());

        Ok(File::new(&name, pathbuf, dirty_meta))
    }

    pub fn new_placeholder(path: &Path) -> Result<File, Error> {
        let mut file = File::new_from_path(path, None)?;
        file.name = "<empty>".to_string();
        file.kind = Kind::Placeholder;
        Ok(file)
    }

    pub fn meta_sync(&mut self) -> HResult<()> {
        let stale = self.meta.get_stale();
        let meta = std::fs::metadata(&self.path)?;
        self.meta = Async::new_with_value(meta);
        self.meta.put_stale(stale);
        self.process_meta()
    }

    pub fn make_async_meta(path: &PathBuf,
                           dirty_meta: Option<AsyncDirtyBit>,
                           stale_preview: Option<Stale>) -> Async<Metadata> {
        let path = path.clone();

        let meta_closure = Box::new(move |stale: Stale| {
            if stale.is_stale()? { HError::stale()? }
            Ok(std::fs::symlink_metadata(&path)?)
        });

        let mut meta = match stale_preview {
            Some(stale) => Async::new_with_stale(meta_closure, stale),
            None => Async::new(meta_closure)
        };
        if let Some(dirty_meta) = dirty_meta {
            meta.on_ready(Box::new(move || {
                let mut dirty_meta = dirty_meta.clone();
                dirty_meta.set_dirty();

                Ok(())
            }));
        }
        meta
    }

    pub fn make_async_dirsize(path: &PathBuf,
                              dirty_meta: Option<AsyncDirtyBit>,
                              stale_preview: Option<Stale>) -> Async<usize> {
        let path = path.clone();

        let dirsize_closure = Box::new(move |stale: Stale| {
            if stale.is_stale()? { HError::stale()? }
            Ok(std::fs::read_dir(&path)?.count())
        });

        let mut dirsize = match stale_preview {
            Some(stale) => Async::new_with_stale(dirsize_closure, stale),
            None => Async::new(dirsize_closure)
        };

        if let Some(dirty_meta) = dirty_meta {
            dirsize.on_ready(Box::new(move || {
                let mut dirty_meta = dirty_meta.clone();
                dirty_meta.set_dirty();

                Ok(())
            }));
        }
        dirsize
    }

    pub fn meta(&self) -> HResult<&Metadata> {
        self.meta.get()
    }

    fn take_dirsize(&mut self,
                    pool: &ThreadPool,
                    meta_updated: &mut bool) -> HResult<()> {
        let dirsize = self.dirsize.as_mut()?;
        if let Ok(_) = dirsize.value { return Ok(()) }

        match dirsize.take_async() {
            Ok(_) => { *meta_updated = true; },
            Err(HError::AsyncNotReadyError) => { dirsize.run_pooled(&*pool).ok(); },
            Err(HError::AsyncAlreadyTakenError) => {},
            Err(HError::NoneError) => {},
            err @ Err(_) => { err?; }
        }
        Ok(())
    }

    pub fn take_meta(&mut self,
                     pool: &ThreadPool,
                     meta_updated: &mut bool) -> HResult<()> {
        if self.meta_processed { return Ok(()) }

        match self.meta.take_async() {
            Ok(_) => { *meta_updated = true; },
            Err(HError::AsyncNotReadyError) => { self.meta.run_pooled(&*pool).ok(); },
            Err(HError::AsyncAlreadyTakenError) => {},
            Err(HError::NoneError) => {},
            err @ Err(_) => { err?; }
        }

        self.process_meta()?;

        Ok(())
    }

    pub fn process_meta(&mut self) -> HResult<()> {
        if let Ok(meta) = self.meta.get() {
            let color = self.get_color(&meta);
            let target = if meta.file_type().is_symlink() {
                self.path.read_link().ok()
            } else { None };

            self.color = color;
            self.target = target;
            self.meta_processed = true;
        }
        Ok(())
    }

    pub fn reload_meta(&mut self) -> HResult<()> {
        self.meta_processed = false;
        self.meta = File::make_async_meta(&self.path,
                                          self.dirty_meta.clone(),
                                          None);
        self.meta.run().log();

        if self.dirsize.is_some() {
            self.dirsize
                = Some(File::make_async_dirsize(&self.path, self.dirty_meta.clone(), None));
            self.dirsize.as_mut()?.run().log();
        }
        Ok(())
    }

    fn get_color(&self, meta: &std::fs::Metadata) -> Option<lscolors::Color> {
        match COLORS.style_for_path_with_metadata(&self.path, Some(&meta)) {
            Some(style) => style.clone().foreground,
            None => None,
        }
    }

    pub fn calculate_size(&self) -> HResult<(u64, String)> {
        if let Some(ref dirsize) = self.dirsize {
            return Ok((dirsize.value.clone()? as u64, "".to_string()))
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

    // pub fn get_mime(&self) -> String {
    //     tree_magic::from_filepath(&self.path)
    // }

    pub fn is_text(&self) -> bool {
         tree_magic::match_filepath("text/plain", &self.path)
    }


    pub fn parent(&self) -> Option<PathBuf> {
        Some(self.path.parent()?.to_path_buf())
    }

    pub fn parent_as_file(&self) -> HResult<File> {
        let pathbuf = self.parent()?;
        File::new_from_path(&pathbuf, None)
    }

    pub fn grand_parent(&self) -> Option<PathBuf> {
        Some(self.path.parent()?.parent()?.to_path_buf())
    }

    pub fn grand_parent_as_file(&self) -> HResult<File> {
        let pathbuf = self.grand_parent()?;
        File::new_from_path(&pathbuf, None)
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
            true => TAGS.write()?.1.push(self.path.clone()),
            false => { TAGS.write()?.1.remove_item(&self.path); },
        }
        self.save_tags()?;
        Ok(())
    }

    pub fn save_tags(&self) -> HResult<()> {
        std::thread::spawn(|| -> HResult<()> {
            let tagfile_path = crate::paths::tagfile_path()?;
            let tags = TAGS.read()?.clone();
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
                crate::term::color_red()  };
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
                crate::term::color_red()  };
        Some(format!("{}{}", color, file_group.name().to_string_lossy()))
    }

    pub fn pretty_mtime(&self) -> Option<String> {
        if self.meta().is_err() { return None }
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
