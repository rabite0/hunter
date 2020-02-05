use std::cmp::Ord;
use std::collections::{HashMap, HashSet};
use std::ops::Index;
use std::fs::Metadata;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::sync::mpsc::Sender;
use std::hash::{Hash, Hasher};
use std::str::FromStr;

use lscolors::LsColors;
use tree_magic;
use users::{get_current_username,
            get_current_groupname,
            get_user_by_uid,
            get_group_by_gid};
use chrono::TimeZone;
use failure::Error;
use rayon::{ThreadPool, ThreadPoolBuilder};
use alphanumeric_sort::compare_str;
use mime_guess;
use rayon::prelude::*;

use pathbuftools::PathBufTools;
use async_value::{Async, Stale, StopIter};

use crate::fail::{HResult, HError, ErrorLog};
use crate::dirty::{AsyncDirtyBit, DirtyBit, Dirtyable};
use crate::widget::Events;
use crate::icon::Icons;
use crate::fscache::FsEvent;


lazy_static! {
    static ref COLORS: LsColors = LsColors::from_env().unwrap_or_default();
    static ref TAGS: RwLock<(bool, Vec<PathBuf>)> = RwLock::new((false, vec![]));
    static ref ICONS: Icons = Icons::new();
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
pub struct RefreshPackage {
    pub new_files: Option<Vec<File>>,
    pub new_buffer: Option<Vec<String>>,
    pub new_len: usize,
}




impl RefreshPackage {
    fn new(mut files: Files,
           old_buffer: Vec<String>,
           events: Vec<FsEvent>,
           render_fn: impl Fn(&File) -> String) -> RefreshPackage {
        use FsEvent::*;

        // If there is only a placeholder at this point, remove it now
        if files.len() == 1 {
            files.remove_placeholder();
        }

        //To preallocate collections
        let event_count = events.len();

        // Need at least one copy for the hashmaps
        let static_files = files.clone();

        // Optimization to speed up access to array
        let file_pos_map: HashMap<&File, usize> = static_files
            .files
            .iter()
            .enumerate()
            .map(|(i, file)| (file, i))
            .collect();


        // Need to know which line of the ListView buffer belongs to which file
        let list_pos_map: HashMap<&File, usize> = static_files
            .iter_files()
            .enumerate()
            .take_while(|&(i, _)| i < old_buffer.len())
            .map(|(i, file)| (file, i))
            .collect();

        // Save new files to add them all at once later
        let mut new_files = Vec::with_capacity(event_count);

        // Files that need rerendering to make all changes visible (size, etc.)
        let mut changed_files = HashSet::with_capacity(event_count);

        // Save deletions to delete them efficiently later
        let mut deleted_files = HashSet::with_capacity(event_count);

        for event in events.into_iter() {
            match event {
                Create(mut file) => {
                    let dirty_meta = files.dirty_meta.clone();
                    file.dirty_meta = Some(dirty_meta);
                    file.meta_sync().log();
                    new_files.push(file);
                }
                Change(file) => {
                    if let Some(&fpos) = file_pos_map.get(&file) {
                        files.files[fpos].meta_sync().log();
                        changed_files.insert(file);
                    }
                }
                Rename(old, new) => {
                    if let Some(&fpos) = file_pos_map.get(&old) {
                        files.files[fpos].rename(&new.path).log();
                        files.files[fpos].meta_sync().log();
                    }
                }
                Remove(file) => {
                    if let Some(_) = file_pos_map.get(&file) {
                        deleted_files.insert(file);
                    }
                }
            }
        }

        if deleted_files.len() > 0 {
            files.files.retain(|file| !deleted_files.contains(file));
        }

        // Finally add all new files
        files.files.extend(new_files);

        // Files added, removed, renamed to hidden, etc...
        files.recalculate_len();
        files.sort();

        // Prerender new buffer in current thread
        let mut old_buffer = old_buffer;

        let new_buffer = files.iter_files()
            .map(|file| {
                match list_pos_map.get(&file) {
                    Some(&old_pos) =>
                        match changed_files.contains(&file) {
                            true => render_fn(&file),
                            false => std::mem::take(&mut old_buffer[old_pos])
                        }
                    None => render_fn(&file)
                }
            }).collect();

        // Need to unpack this to prevent issue with recursive Files type
        // Also, if no files remain add placeholder and set len
        let (files, new_len, new_buffer) = if files.len() > 0 {
            (files.files, files.len, new_buffer)
        } else {
            let placeholder = File::new_placeholder(&files.directory.path).unwrap();
            let buffer = vec![render_fn(&placeholder)];
            files.files.push(placeholder);
            (files.files, 1, buffer)
        };


        RefreshPackage {
            new_files: Some(files),
            new_buffer: Some(new_buffer),
            new_len: new_len
        }
    }
}


#[derive(Derivative)]
#[derivative(PartialEq, Eq, Hash, Clone, Debug)]
pub struct Files {
    pub directory: File,
    pub files: Vec<File>,
    pub len: usize,
    #[derivative(Debug="ignore")]
    #[derivative(PartialEq="ignore")]
    #[derivative(Hash="ignore")]
    pub pending_events: Arc<RwLock<Vec<FsEvent>>>,
    #[derivative(Debug="ignore")]
    #[derivative(PartialEq="ignore")]
    #[derivative(Hash="ignore")]
    pub refresh: Option<Async<RefreshPackage>>,
    pub meta_upto: Option<usize>,
    pub sort: SortBy,
    pub dirs_first: bool,
    pub reverse: bool,
    pub show_hidden: bool,
    pub filter: Option<String>,
    pub filter_selected: bool,
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

use std::default::Default;

impl Default for Files {
    fn default() -> Files {
        Files {
            directory: File::new_placeholder(Path::new("")).unwrap(),
            files: vec![],
            len: 0,
            pending_events: Arc::new(RwLock::new(vec![])),
            refresh: None,
            meta_upto: None,
            sort: SortBy::Name,
            dirs_first: true,
            reverse: false,
            show_hidden: true,
            filter: None,
            filter_selected: false,
            dirty: DirtyBit::new(),
            dirty_meta: AsyncDirtyBit::new(),
        }
    }
}


impl Files {
    pub fn new_from_path(path: &Path) -> HResult<Files> {
        let direntries: Result<Vec<_>, _> = std::fs::read_dir(&path)?.collect();
        let dirty_meta = AsyncDirtyBit::new();
        let tags = &TAGS.read().ok()?.1;

        let files: Vec<_> = direntries?
            .iter()
            .map(|file| {
                let name = file.file_name();
                let name = name.to_string_lossy();
                let path = file.path();
                let mut file = File::new(&name,
                                         path,
                                         Some(dirty_meta.clone()));
                file.set_tag_status(&tags);
                Some(file)
            })
            .collect();

        let len = files.len();

        let mut files = Files::default();
        files.directory = File::new_from_path(&path, None)?;
        files.len = len;
        files.dirty_meta = dirty_meta;


        Ok(files)
    }

    pub fn new_from_path_cancellable(path: &Path,
                                     stale: Stale)
                                     -> HResult<Files> {
        let direntries: Result<Vec<_>, _> = std::fs::read_dir(&path)?.collect();
        let dirty = DirtyBit::new();
        let dirty_meta = AsyncDirtyBit::new();

        let files: Vec<_> = direntries?
            .into_iter()
            .stop_stale(stale.clone())
            .par_bridge()
            .map(|file| {
                let file = File::new_from_direntry(file,
                                                   Some(dirty_meta.clone()));
                file
            })
            .collect();

        if stale.is_stale()? {
            return Err(crate::fail::HError::StalePreviewError {
                file: path.to_string_lossy().to_string()
            })?;
        }

        let len = files.len();

        let files = Files {
            directory: File::new_from_path(&path, None)?,
            files: files,
            len: len,
            pending_events: Arc::new(RwLock::new(vec![])),
            refresh: None,
            meta_upto: None,
            sort: SortBy::Name,
            dirs_first: true,
            reverse: false,
            show_hidden: true,
            filter: None,
            filter_selected: false,
            dirty: dirty,
            dirty_meta: dirty_meta,
        };

        Ok(files)
    }

    pub fn recalculate_len(&mut self) {
        self.len = self.par_iter_files().count();
    }

    pub fn get_file_mut(&mut self, index: usize) -> Option<&mut File> {
        self.par_iter_files_mut()
            .find_first(|(i, _)| *i == index)
            .map(|(_, f)| f)
    }

    pub fn par_iter_files(&self) -> impl ParallelIterator<Item=&File> {
        let filter = self.filter.clone();
        let filter_selected = self.filter_selected;
        let show_hidden = self.show_hidden;

        self.files
            .par_iter()
            .filter(move |f|
                    f.kind == Kind::Placeholder ||
                    !(filter.is_some() &&
                      !f.name.contains(filter.as_ref().unwrap())) &&
                    (!filter_selected || f.selected))
            .filter(move |f| !(!show_hidden && f.hidden))
    }

    pub fn par_iter_files_mut(&mut self) -> impl ParallelIterator<Item=(usize,
                                                                        &mut File)> {
        let filter = self.filter.clone();
        let filter_selected = self.filter_selected;
        let show_hidden = self.show_hidden;

        self.files
            .par_iter_mut()
            .enumerate()
            .filter(move |(_,f)|
                    f.kind == Kind::Placeholder ||
                    !(filter.is_some() &&
                      !f.name.contains(filter.as_ref().unwrap())) &&
                    (!filter_selected || f.selected))
            .filter(move |(_,f)| !(!show_hidden && f.hidden))
    }
    pub fn iter_files(&self) -> impl Iterator<Item=&File> {
        let filter = self.filter.clone();
        let filter_selected = self.filter_selected;
        let show_hidden = self.show_hidden;

        self.files
            .iter()
            .filter(move |f|
                    f.kind == Kind::Placeholder ||
                    !(filter.is_some() &&
                      !f.name.contains(filter.as_ref().unwrap())) &&
                    (!filter_selected || f.selected))
            .filter(move |f| !(!show_hidden && f.hidden))
    }

    pub fn iter_files_mut(&mut self) -> impl Iterator<Item=&mut File> {
        let filter = self.filter.clone();
        let filter_selected = self.filter_selected;
        let show_hidden = self.show_hidden;

        self.files
            .iter_mut()
            .filter(move |f|
                    f.kind == Kind::Placeholder ||
                    !(filter.is_some() &&
                      !f.name.contains(filter.as_ref().unwrap())) &&
                    (!filter_selected || f.selected))
            .filter(move |f| !(!show_hidden && f.hidden))
    }

    #[allow(trivial_bounds)]
    pub fn into_iter_files(self) -> impl Iterator<Item=File> {
        let filter = self.filter;
        let filter_selected = self.filter_selected;
        let show_hidden = self.show_hidden;

        self.files
            .into_iter()
            .filter(move |f|
                    f.kind == Kind::Placeholder ||
                    !(filter.is_some() &&
                      !f.name.contains(filter.as_ref().unwrap())) &&
                    (!filter_selected || f.selected))
            .filter(move |f| !(!show_hidden && f.name.starts_with(".")))
    }

    pub fn sort(&mut self) {
        use std::cmp::Ordering::*;

        let dirs_first = self.dirs_first;

        match self.sort {
            SortBy::Name => self
                .files
                .par_sort_unstable_by(|a, b| {
                    if dirs_first {
                        match (a.is_dir(),  b.is_dir()) {
                            (true, false) => Less,
                            (false, true) => Greater,
                            _ => compare_str(&a.name, &b.name),
                        }
                    } else {
                        compare_str(&a.name, &b.name)
                    }
                }),
            SortBy::Size => {
                if self.meta_upto < Some(self.len()) {
                    self.meta_all_sync().log();
                }

                self.files.par_sort_unstable_by(|a, b| {
                    if dirs_first {
                        match (a.is_dir(),  b.is_dir()) {
                            (true, false) => return Less,
                            (false, true) => return Greater,
                            _ => {}
                        }
                    }

                    match (a.meta(), b.meta()) {
                        (Some(a_meta), Some(b_meta)) => {
                            match a_meta.size() == b_meta.size() {
                                true => compare_str(&b.name, &a.name),
                                false => b_meta.size()
                                               .cmp(&a_meta.size())
                            }
                        }
                        _ => Equal
                    }
                })
            }
            SortBy::MTime => {
                if self.meta_upto < Some(self.len()) {
                    self.meta_all_sync().log();
                }

                self.files.par_sort_unstable_by(|a, b| {
                    if dirs_first {
                        match (a.is_dir(),  b.is_dir()) {
                            (true, false) => return Less,
                            (false, true) => return Greater,
                            _ => {}
                        }
                    }

                    match (a.meta(), b.meta()) {
                        (Some(a_meta), Some(b_meta)) => {
                            match a_meta.mtime() == b_meta.mtime() {
                                true => compare_str(&b.name, &a.name),
                                false => b_meta.mtime()
                                               .cmp(&a_meta.mtime())
                            }
                        }
                        _ => Equal
                    }
                })
            }
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
        self.show_hidden = !self.show_hidden;
        self.set_dirty();

        if self.show_hidden == true && self.len() > 1 {
            self.remove_placeholder();
        } else {
            // avoid doing this twice, since remove_placeholder() does it too
            self.recalculate_len();
        }
    }

    fn remove_placeholder(&mut self) {
        let dirpath = self.directory.path.clone();
        self.find_file_with_path(&dirpath).cloned()
            .map(|placeholder| {
                self.files.remove_item(&placeholder);
                self.recalculate_len();
            });
    }

    pub fn ready_to_refresh(&self) -> HResult<bool> {
        let pending = self.pending_events.read()?.len();
        let running = self.refresh.is_some();
        Ok(pending > 0 && !running)
    }

    pub fn get_refresh(&mut self) -> HResult<Option<RefreshPackage>> {
        if let Some(mut refresh) = self.refresh.take() {
            if refresh.is_ready() {
                refresh.pull_async()?;
                let mut refresh = refresh.value?;
                self.files = refresh.new_files.take()?;
                if refresh.new_len != self.len() {
                    self.len = refresh.new_len;
                }
                return Ok(Some(refresh));
            } else {
                self.refresh.replace(refresh);
            }
        }

        return Ok(None)
    }

    pub fn process_fs_events(&mut self,
                             buffer: Vec<String>,
                             sender: Sender<Events>,
                             render_fn: impl Fn(&File) -> String + Send + 'static)
                             -> HResult<()> {
        let pending = self.pending_events.read()?.len();

        if pending > 0 {
            let events = self.pending_events
                .write()?
                .drain(0..pending)
                .collect::<Vec<_>>();
            let files = self.clone();

            let mut refresh = Async::new(move |_| {
                let refresh = RefreshPackage::new(files,
                                                  buffer,
                                                  events,
                                                  render_fn);
                Ok(refresh)
            });

            refresh.on_ready(move |_,_| {
                Ok(sender.send(Events::WidgetReady)?)
            })?;

            refresh.run()?;

            self.refresh = Some(refresh);
        }

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

    pub fn find_file_with_name(&self, name: &str) -> Option<&File> {
        self.iter_files()
            .find(|f| f.name.to_lowercase().contains(name))
    }

    pub fn find_file_with_path(&mut self, path: &Path) -> Option<&mut File> {
        self.iter_files_mut().find(|file| file.path == path)
    }

    pub fn meta_all_sync(&mut self) -> HResult<()> {
        let same = Mutex::new(true);

        self.iter_files_mut()
            .par_bridge()
            .for_each(|f| {
                if !f.meta_processed {
                    f.meta_sync().log();
                    same.lock()
                        .map(|mut t| *t = false)
                        .map_err(HError::from)
                        .log();
                }
            });

        if !*same.lock()? {
            self.set_dirty();
        }

        Ok(())
    }

    pub fn set_filter(&mut self, filter: Option<String>) {
        self.filter = filter;

        // Do this first, so we know len() == 0 needs a placeholder
        self.remove_placeholder();

        if self.len() == 0 {
            let placeholder = File::new_placeholder(&self.directory.path).unwrap();
            self.files.push(placeholder);
            self.len = 1;
        }

        self.set_dirty();
    }

    pub fn get_filter(&self) -> Option<String> {
        self.filter.clone()
    }

    pub fn toggle_filter_selected(&mut self) {
        self.filter_selected = !self.filter_selected;
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn get_selected(&self) -> impl Iterator<Item=&File> {
        self.iter_files()
            .filter(|f| f.is_selected())
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

impl std::fmt::Debug for File {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(formatter, "{:?}", self.path)
    }
}

impl std::default::Default for File {
    fn default() -> File {
        File::new_placeholder(Path::new("")).unwrap()
    }
}


#[derive(Clone)]
pub struct File {
    pub name: String,
    pub path: PathBuf,
    pub hidden: bool,
    pub kind: Kind,
    pub dirsize: Option<usize>,
    pub target: Option<PathBuf>,
    pub color: Option<lscolors::Color>,
    pub meta: Option<Metadata>,
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
        let hidden = name.starts_with(".");

        File {
            name: name.to_string(),
            hidden: hidden,
            kind: if path.is_dir() { Kind::Directory } else { Kind::File },
            path: path,
            dirsize: None,
            target: None,
            meta: None,
            meta_processed: false,
            dirty_meta: dirty_meta,
            color: None,
            selected: false,
            tag: None,
        }
    }

    pub fn new_with_stale(name: &str,
                          path: PathBuf,
                          dirty_meta: Option<AsyncDirtyBit>) -> File {
        let hidden = name.starts_with(".");

        File {
            name: name.to_string(),
            hidden: hidden,
            kind: if path.is_dir() { Kind::Directory } else { Kind::File },
            path: path,
            dirsize: None,
            target: None,
            meta: None,
            meta_processed: false,
            dirty_meta: dirty_meta,
            color: None,
            selected: false,
            tag: None,
        }
    }

    pub fn new_from_direntry(direntry: std::fs::DirEntry,
                             dirty_meta: Option<AsyncDirtyBit>) -> File {
        let path = direntry.path();
        let name = direntry.file_name()
                           .to_string_lossy()
                           .to_string();
        let hidden = name.chars().nth(0) == Some('.');

        let kind = match direntry.file_type() {
            Ok(ftype) => match ftype.is_file() {
                true => Kind::File,
                false => Kind::Directory
            }
            _ => Kind::Placeholder
        };

        File {
            name: name,
            hidden: hidden,
            kind: kind,
            path: path,
            dirsize: None,
            target: None,
            meta: None,
            meta_processed: false,
            dirty_meta: dirty_meta,
            color: None,
            selected: false,
            tag: None,
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

    pub fn rename(&mut self, new_path: &Path) -> HResult<()> {
        self.name = new_path.file_name()?.to_string_lossy().to_string();
        self.path = new_path.into();
        Ok(())
    }

    pub fn meta_sync(&mut self) -> HResult<()> {
        let meta = std::fs::symlink_metadata(&self.path)?;
        self.meta = Some(meta);
        self.process_meta().log();

        if self.is_dir() {
            let dirsize = std::fs::read_dir(&self.path)?.count();
            self.dirsize = Some(dirsize);
        }

        Ok(())
    }

    pub fn meta(&self) -> Option<&Metadata> {
        self.meta.as_ref()
    }

    pub fn process_meta(&mut self) -> HResult<()> {
        if let Some(ref meta) = self.meta {
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
        self.meta_sync()
    }

    fn get_color(&self, meta: &std::fs::Metadata) -> Option<lscolors::Color> {
        match COLORS.style_for_path_with_metadata(&self.path, Some(&meta)) {
            Some(style) => style.clone().foreground,
            None => None,
        }
    }

    pub fn calculate_size(&self) -> HResult<(u32, &str)> {
        if let Some(ref dirsize) = self.dirsize {
            return Ok((*dirsize as u32, ""))
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
        };

        Ok((size as u32, unit))
    }

    pub fn get_mime(&self) -> Option<mime_guess::Mime> {
        if let Some(ext) = self.path.extension() {
            let mime = mime_guess::from_ext(&ext.to_string_lossy()).first();
            mime
        } else {
            // Fix crash in tree_magic when called on non-regular file
            // Also fix crash when a file doesn't exist any more
            self.meta()
                .and_then(|meta| {
                    if meta.is_file() && self.path.exists() {
                        let mime = tree_magic::from_filepath(&self.path);
                        mime::Mime::from_str(&mime).ok()
                    } else { None }
                })
        }
    }

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

    pub fn read_dir(&self) -> HResult<Files> {
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

    pub fn set_tag_status(&mut self, tags: &[PathBuf]) {
        match tags.contains(&self.path) {
            true => self.tag = Some(true),
            false => self.tag = Some(false)
        }
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
        if self.meta().is_none() { return None }
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
        if self.meta().is_none() { return None }
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
        if self.meta().is_none() { return None }
        let time: chrono::DateTime<chrono::Local>
            = chrono::Local.timestamp(self.meta().unwrap().mtime(), 0);
        Some(time.format("%F %R").to_string())
    }

    pub fn icon(&self) -> &'static str {
        ICONS.get(&self.path)
    }

    pub fn short_path(&self) -> PathBuf {
        self.path.short_path()
    }

    pub fn short_string(&self) -> String {
        self.path.short_string()
    }
}
