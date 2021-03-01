use std::cmp::Ord;
use std::collections::{HashMap, HashSet};
use std::ops::Index;
use std::fs::Metadata;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::sync::mpsc::Sender;
use std::hash::{Hash, Hasher};
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::ffi::OsStr;

use failure;
use failure::Fail;
use lscolors::LsColors;
use tree_magic_fork;
use users::{get_current_username,
            get_current_groupname,
            get_user_by_uid,
            get_group_by_gid};
use chrono::TimeZone;
use failure::Error;
use rayon::{ThreadPool, ThreadPoolBuilder};
use natord::compare;
use mime_guess;
use rayon::prelude::*;
use nix::{dir::*,
          fcntl::OFlag,
          sys::stat::Mode};

use pathbuftools::PathBufTools;
use async_value::{Async, Stale, StopIter};

use crate::fail::{HResult, HError, ErrorLog};
use crate::dirty::{DirtyBit, Dirtyable};
use crate::widget::Events;
use crate::icon::Icons;
use crate::fscache::{FsCache, FsEvent};

lazy_static! {
    static ref COLORS: LsColors = LsColors::from_env().unwrap_or_default();
    static ref TAGS: RwLock<(bool, Vec<PathBuf>)> = RwLock::new((false, vec![]));
    static ref ICONS: Icons = Icons::new();
    static ref IOTICK_CLIENTS: AtomicUsize = AtomicUsize::default();
    static ref IOTICK: AtomicUsize = AtomicUsize::default();
}

pub fn tick_str() -> &'static str {
    // Using mod 5 for that nice nonlinear look
    match IOTICK.load(Ordering::Relaxed) % 5 {
        0 => "   ",
        1 => ".  ",
        2 => ".. ",
        _ => "..."
    }
}

pub fn start_ticking(sender: Sender<Events>) {
    use std::time::Duration;

    IOTICK_CLIENTS.fetch_add(1, Ordering::Relaxed);
    if IOTICK_CLIENTS.load(Ordering::Relaxed) == 1 {
        std::thread::spawn(move || {
            IOTICK.store(0, Ordering::Relaxed);

            // Gently slow down refreshes
            let backoff = Duration::from_millis(10);
            let mut cooldown = Duration::from_millis(10);

            loop {
                IOTICK.fetch_add(1, Ordering::Relaxed);

                // Send refresh event before sleeping
                sender.send(crate::widget::Events::WidgetReady)
                      .unwrap();

                // All jobs done?
                if IOTICK_CLIENTS.load(Ordering::Relaxed) == 0 {
                    IOTICK.store(0, Ordering::Relaxed);
                    return;
                }

                std::thread::sleep(cooldown);

                // Slow down up to 1 second
                if cooldown < Duration::from_millis(1000) {
                    cooldown += backoff;
                }
            }
        });
    }
}

pub fn stop_ticking() {
    IOTICK_CLIENTS.fetch_sub(1, Ordering::Relaxed);
}

#[derive(Fail, Debug, Clone)]
pub enum FileError {
    #[fail(display = "Metadata still pending!")]
    MetaPending,
    #[fail(display = "Couldn't open directory! Error: {}", _0)]
    OpenDir(#[cause] nix::Error),
    #[fail(display = "Couldn't read files! Error: {}", _0)]
    ReadFiles(#[cause] nix::Error),
    #[fail(display = "Had problems with getdents64 in directory: {}", _0)]
    GetDents(String),
}

pub fn get_pool() -> ThreadPool {
    // Optimal number of threads depends on many things. This is a reasonable default.
    const THREAD_NUM: usize = 8;

    ThreadPoolBuilder::new()
        .num_threads(THREAD_NUM)
        .thread_name(|i| format!("hunter_iothread_{}", i))
        .build()
        .unwrap()
}

pub fn load_tags() -> HResult<()> {
    std::thread::spawn(|| -> HResult<()> {
        let tag_path = crate::paths::tagfile_path()?;

        if !tag_path.exists() {
            import_tags().log();
        }

        let tags = std::fs::read_to_string(tag_path)?;
        let mut tags = tags.lines()
                           .map(PathBuf::from)
                           .collect::<Vec<PathBuf>>();
        tags.sort();
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
    let tagged = TAGS.read()?.1.binary_search(path)
                               .map_or_else(|_| false,
                                            |_| true);
    Ok(tagged)
}

pub fn tags_loaded() -> HResult<()> {
    let loaded = TAGS.read()?.0;
    if loaded { Ok(()) }
    else { HError::tags_not_loaded() }
}

#[derive(Derivative)]
#[derivative(PartialEq, Eq, Hash, Clone, Debug)]
pub struct RefreshPackage {
    pub new_files: Option<Vec<File>>,
    pub new_len: usize,
    #[derivative(Debug="ignore")]
    #[derivative(PartialEq="ignore")]
    #[derivative(Hash="ignore")]
    pub jobs: Vec<Job>
}




impl RefreshPackage {
    fn new(mut files: Files, events: Vec<FsEvent>) -> RefreshPackage {
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

        // Save new files to add them all at once later
        let mut new_files = Vec::with_capacity(event_count);

        // Files that need rerendering to make all changes visible (size, etc.)
        let mut changed_files = HashSet::with_capacity(event_count);

        // Save deletions to delete them efficiently later
        let mut deleted_files = HashSet::with_capacity(event_count);

        // Stores jobs to asynchronously fetch metadata
        let mut jobs = Vec::with_capacity(event_count);

        let cache = &files.cache.take().unwrap();

        // Drop would set this stale after the function returns
        let stale = files.stale.take().unwrap();


        for event in events.into_iter().stop_stale(stale.clone()) {
            match event {
                Create(mut file) => {
                    let job = file.prepare_meta_job(cache);
                    job.map(|j| jobs.push(j));
                    new_files.push(file);
                }
                Change(file) => {
                    if let Some(&fpos) = file_pos_map.get(&file) {
                        let job = files.files[fpos].refresh_meta_job();
                        jobs.push(job);
                        changed_files.insert(file);
                    }
                }
                Rename(old, new) => {
                    if let Some(&fpos) = file_pos_map.get(&old) {
                        files.files[fpos].rename(&new.path).log();
                        let job = files.files[fpos].refresh_meta_job();
                        jobs.push(job);
                            }
                }
                Remove(file) => {
                    if let Some(_) = file_pos_map.get(&file) {
                        deleted_files.insert(file);
                    }
                }
            }
        }

        // Bail out without further processing
        if stale.is_stale().unwrap_or(true) {
            return RefreshPackage {
                new_files: None,
                new_len: 0,
                jobs: jobs
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

        // Need to unpack this to prevent issue with recursive Files type
            // Also, if no files remain add placeholder and set len
        let (files, new_len) = if files.len() > 0 {
                (std::mem::take(&mut files.files), files.len)
        } else {
            let placeholder = File::new_placeholder(&files.directory.path).unwrap();
            files.files.push(placeholder);
            (std::mem::take(&mut files.files), 1)
        };

        RefreshPackage {
            new_files: Some(files),
            new_len: new_len,
            jobs: jobs
        }
    }
}

// Tuple that stores path and "slots" to store metaadata in
pub type Job = (PathBuf,
                Option<Arc<RwLock<Option<Metadata>>>>,
                Option<Arc<(AtomicBool, AtomicUsize)>>);

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
    #[derivative(Debug="ignore")]
    #[derivative(PartialEq="ignore")]
    #[derivative(Hash="ignore")]
    pub jobs: Vec<Job>,
    #[derivative(Debug="ignore")]
    #[derivative(PartialEq="ignore")]
    #[derivative(Hash="ignore")]
    pub cache: Option<FsCache>,
    #[derivative(Debug="ignore")]
    #[derivative(PartialEq="ignore")]
    #[derivative(Hash="ignore")]
    pub stale: Option<Stale>
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
            show_hidden: false,
            filter: None,
            filter_selected: false,
            dirty: DirtyBit::new(),
            jobs: vec![],
            cache: None,
            stale: None
        }
    }
}

// Stop processing stuff when Files is dropped
impl Drop for Files {
    fn drop(&mut self) {
        self.stale
            .as_ref()
            .map(|s| s.set_stale());
    }
}


#[cfg(target_os = "linux")]
#[repr(C)]
#[derive(Clone, Debug)]
pub struct linux_dirent {
    pub d_ino: u64,
    pub d_off: u64,
    pub d_reclen: u16,
    pub d_type: u8,
    pub d_name: [u8; 0],
}


// This arcane spell hastens the target by around 30%.

// It uses quite a bit of usafe code, mostly to call into libc and
// dereference raw pointers inherent to the getdents API, but also to
// avoid some of the overhead built into Rust's default conversion
// methods. How the getdents64 syscall is intended to be used was
// mostly looked up in man 2 getdents64, the nc crate, and the
// upcoming version of the walkdir crate, plus random examples here
// and there..

// This should probably be replaced with walkdir when it gets a proper
// release with the new additions. nc itself is already too high level
// to meet the performance target, unfortunately.

// TODO: Better handling of file systems/kernels that don't support
// report the kind of file in d_type. Currently that means calling
// stat on ALL files and ithrowing away the result. This is wasteful.
#[cfg(target_os = "linux")]
pub fn from_getdents(fd: i32, path: &Path, nothidden: &AtomicUsize)  -> Result<Vec<File>, FileError>
{
    use libc::SYS_getdents64;

    // Buffer size was chosen after measuring different sizes and 4k seemed best
    const BUFFER_SIZE: usize = 1024 * 1024 * 4;

    // Abuse Vec<u8> as byte buffer
    let mut buf: Vec<u8> = vec![0; BUFFER_SIZE];
    let bufptr = buf.as_mut_ptr();

    // Store all the converted (to File) entries in here
    let files = std::sync::Mutex::new(Vec::<File>::new());
    let files = &files;

    // State of the getdents loop
    enum DentStatus {
        More(Vec<File>),
        Skip,
        Done,
        Err(FileError)
    }


    let result = crossbeam::scope(|s| {
        loop {
            // Returns number of bytes written to buffer
            let nread = unsafe { libc::syscall(SYS_getdents64, fd, bufptr, BUFFER_SIZE) };

            // 0 means done, -1 means an error happened
            if nread == 0 {
                break;
            } else if nread < 0 {
                let pathstr = path.to_string_lossy().to_string();
                HError::log::<()>(&format!("Couldn't read dents from: {}",
                                           &pathstr)).ok();
                break;
            }

            // Clone buffer for parallel processing in another thread
            let mut buf: Vec<u8> = buf.clone();

            s.spawn(move |_| {
                // Rough approximation of the number of entries. Actual
                // size changes from entry to entry due to variable string
                // size.
                let cap = nread as usize / std::mem::size_of::<linux_dirent>();
                // Use a local Vec to avoid contention on Mutex
                let mut localfiles = Vec::with_capacity(cap);
                let bufptr = buf.as_mut_ptr() as usize;
                let mut bpos: usize = 0;

                while bpos < nread as usize {
                    // The buffer contains a string of linux_dirents with
                    // varying sizes. To read them correctly one after the
                    // other the variable size of the current entry has to
                    // be addet to the offset of the current buffer. As
                    // long as the kernel doesn't provide wrong values and
                    // the calculations are corrent this is safe to do.
                    // It's bascally (buffer[n] -> buffer[n + len(buffer[n])
                    let d: &linux_dirent = unsafe {
                        std::mem::transmute::<usize, &linux_dirent>(bufptr  + bpos )
                    };

                    // Name lenegth is overallocated, true length can be found by checking with strlen
                    let name_len = d.d_reclen as usize -
                        std::mem::size_of::<u64>() -
                        std::mem::size_of::<u64>() -
                        std::mem::size_of::<u16>() -
                        std::mem::size_of::<u8>();

                    // OOB!!!
                    if bpos + name_len > BUFFER_SIZE {
                        HError::log::<()>(&format!("WARNING: Name for file was out of bounds in: {}",
                                                   path.to_string_lossy())).ok();
                        return DentStatus::Err(FileError::GetDents(path.to_string_lossy().to_string()));
                    }

                    // Add length of current dirent to the current offset
                    // tbuffer[n] -> buffer[n + len(buffer[n])
                    bpos = bpos + d.d_reclen as usize;

                    let name: &OsStr = {
                        // Safe as long as d_name is NULL terminated
                        let true_len = unsafe { libc::strlen(d.d_name.as_ptr() as *const i8) };
                        // Safe if strlen returned without SEGFAULT on OOB (if d_name weren't NULL terminated)
                        let bytes: &[u8] = unsafe { std::slice::from_raw_parts(d.d_name.as_ptr() as *const u8,
                                                                               true_len) };

                        // Don't want this
                        if bytes.len() == 0  || bytes == b"." || bytes == b".." {
                            continue;
                        }

                        // A bit sketchy maybe, but if all checks passed, should be OK.
                        unsafe { std::mem::transmute::<&[u8], &OsStr>(bytes) }
                    };

                    // Avoid reallocation on push
                    let mut pathstr = std::ffi::OsString::with_capacity(path.as_os_str().len() +
                                                                        name.len() +
                                                                        2);
                    pathstr.push(path.as_os_str());
                    pathstr.push("/");
                    pathstr.push(name);

                    let path = PathBuf::from(pathstr);

                    // See dirent.h
                    // Some file systems and Linux < 2.6.4 don't support d_type
                    let (kind, target) = match d.d_type {
                        4 => (Kind::Directory, None),
                        0 => {
                            use nix::sys::stat::*;

                            // This is a bit unfortunate, since the
                            // Metadata can't be seaved, but at lest
                            // stat is faster with an open fd
                            let flags = nix::fcntl::AtFlags::AT_SYMLINK_NOFOLLOW;
                            let stat =
                                match fstatat(fd, &path, flags) {
                                    Ok(stat) => stat,
                                    Err(_) => return DentStatus::Err(FileError::GetDents(path.to_string_lossy()
                                                                                         .to_string()))
                                };

                            let mode = SFlag::from_bits_truncate(stat.st_mode);

                            match mode & SFlag::S_IFMT {
                                SFlag::S_IFDIR => (Kind::Directory, None),
                                _ => (Kind::File, None)
                            }
                        }
                        10 => {
                            // This is a link
                            let target = nix::fcntl::readlinkat(fd, &path)
                                .map(PathBuf::from).ok();
                            let target_kind =
                                match path.is_dir() {
                                    true => Kind::Directory,
                                    false => Kind::File
                                };
                            (target_kind, target)
                        }
                        _ => (Kind::File, None)
                    };

                    let name = name.to_str()
                                   .map(|n| String::from(n))
                                   .unwrap_or_else(|| name.to_string_lossy().to_string());

                    let hidden = name.as_bytes()[0] == b'.';

                    if !hidden {
                        nothidden.fetch_add(1, Ordering::Relaxed);
                    }

                    // Finally the File is created
                    let file = File {
                        name: name,
                        hidden: hidden,
                        kind: kind,
                        path: path,
                        dirsize: None,
                        target: target,
                        meta: None,
                        selected: false,
                        tag: None,
                    };

                    // Push into local Vec
                    localfiles.push(file);
                }

                // Successfully looped over all dirents. Now append everything at once
                files.lock().unwrap().append(&mut localfiles);
                DentStatus::Done
            });
        }
    });

    match result {
        Ok(()) => Ok(std::mem::take(&mut *files.lock().unwrap())),
        Err(_) => Err(FileError::GetDents(path.to_string_lossy().to_string()))
    }
}



impl Files {
    // Use getdents64 on Linux
    #[cfg(target_os = "linux")]
    pub fn new_from_path_cancellable(path: &Path, stale: Stale) -> HResult<Files> {
        use std::os::unix::io::AsRawFd;

        let nonhidden = AtomicUsize::default();

        let dir  = Dir::open(path.clone(),
                             OFlag::O_DIRECTORY,
                             Mode::empty())
            .map_err(|e| FileError::OpenDir(e))?;

        let direntries = from_getdents(dir.as_raw_fd(), path, &nonhidden)?;

        if stale.is_stale()? {
            HError::stale()?;
        }

        let mut files = Files::default();
        files.directory = File::new_from_path(&path)?;


        files.files = direntries;
        files.len = nonhidden.load(Ordering::Relaxed);
        files.stale = Some(stale);

        Ok(files)
    }


    #[cfg(not(target_os = "linux"))]
    pub fn new_from_path_cancellable(path: &Path, stale: Stale) -> HResult<Files> {
        use std::os::unix::io::AsRawFd;

        let nonhidden = AtomicUsize::default();

        let mut dir = Dir::open(path.clone(),
                                OFlag::O_DIRECTORY,
                                Mode::empty())
            .map_err(|e| FileError::OpenDir(e))?;

        let dirfd = dir.as_raw_fd();

        let direntries  = dir
            .iter()
            .stop_stale(stale.clone())
            .map(|f| {
                let f = File::new_from_nixentry(f?, path, dirfd);
                // Fast check to avoid iterating twice
                if f.name.as_bytes()[0] != b'.' {
                    nonhidden.fetch_add(1, Ordering::Relaxed);
                }
                Ok(f)
            })
            .collect::<Result<_,_>>()
            .map_err(|e| FileError::ReadFiles(e))?;

        if stale.is_stale()? {
            HError::stale()?;
        }

        let mut files = Files::default();
        files.directory = File::new_from_path(&path)?;
        files.files = direntries;
        files.len = nonhidden.load(Ordering::Relaxed);
        files.stale = Some(stale);

        Ok(files)
    }

    pub fn enqueue_jobs(&mut self, n: usize) {
        let from = self.meta_upto.unwrap_or(0);
        self.meta_upto = Some(from + n);

        let cache = match self.cache.clone() {
            Some(cache) => cache,
            None => return
        };

        let mut jobs = self.iter_files_mut()
                           .collect::<Vec<&mut File>>()
                           .into_par_iter()
                           .skip(from)
                           .take(n)
                           .filter_map(|f| f.prepare_meta_job(&cache))
                           .collect::<Vec<_>>();

        self.jobs.append(&mut jobs);
    }

    pub fn run_jobs(&mut self, sender: Sender<Events>) {
        let jobs = std::mem::take(&mut self.jobs);
        let stale = self.stale
                        .clone()
                        .unwrap_or_else(Stale::new);

        if jobs.len() == 0 { return; }

        std::thread::spawn(move || {
            let pool = get_pool();
            let stale = &stale;

            start_ticking(sender);

            pool.scope_fifo(move |s| {
                for (path, mslot, dirsize) in jobs.into_iter()
                                                  .stop_stale(stale.clone())
                {
                    s.spawn_fifo(move |_| {
                        if let Some(mslot) = mslot {
                            if let Ok(meta) = std::fs::symlink_metadata(&path) {
                                *mslot.write().unwrap() = Some(meta);
                            }
                        }

                        if let Some(dirsize) = dirsize {
                            let size = Dir::open(&path,
                                                 OFlag::O_DIRECTORY,
                                                 Mode::empty())
                                .map(|mut d| d.iter().count())
                                .map_err(|e| FileError::OpenDir(e))
                                .log_and()
                                .unwrap_or(0);

                            dirsize.0.store(true, Ordering::Relaxed);
                            dirsize.1.store(size, Ordering::Relaxed);
                        };
                    });
                }
            });

            stop_ticking();
        });
    }

    pub fn recalculate_len(&mut self) {
        self.len = self.par_iter_files().count();
    }

    pub fn get_file_mut(&mut self, index: usize) -> Option<&mut File> {
        // Need actual length of self.files for this
        let hidden_in_between = self.files_in_between(index, self.files.len());

        self.files.get_mut(index + hidden_in_between)
    }

    pub fn par_iter_files(&self) -> impl ParallelIterator<Item=&File> {
        let filter_fn = self.filter_fn();

        self.files
            .par_iter()
            .filter(move |f| filter_fn(f))
    }

    pub fn iter_files(&self) -> impl Iterator<Item=&File> {
        let filter_fn = self.filter_fn();

        self.files
            .iter()
            .filter(move |&f| filter_fn(f))
    }

    pub fn files_in_between(&self, pos: usize, n_before: usize) -> usize {
        let filter_fn = self.filter_fn();

        self.files[..pos].iter()
                          .rev()
                          .enumerate()
                          .filter(|(_, f)| filter_fn(f))
                          .take(n_before)
                          .map(|(i, _)| i + 1)
                          .last()
                          .unwrap_or(0)
    }

    pub fn iter_files_from(&self, from: &File, n_before: usize) -> impl Iterator<Item=&File> {
        let fpos = self.find_file(from).unwrap_or(0);

        let files_in_between = self.files_in_between(fpos, n_before);

        let filter_fn = self.filter_fn();

        self.files[fpos.saturating_sub(files_in_between)..]
            .iter()
            .filter(move |f| filter_fn(f))
    }

    pub fn iter_files_mut_from(&mut self, from: &File, n_before: usize) -> impl Iterator<Item=&mut File> {
        let fpos = self.find_file(from).unwrap_or(0);
        let files_in_between = self.files_in_between(fpos, n_before);

        let filter_fn = self.filter_fn();

        self.files[fpos.saturating_sub(files_in_between)..]
            .iter_mut()
            .filter(move |f| filter_fn(f))
    }

    pub fn iter_files_mut(&mut self) -> impl Iterator<Item=&mut File> {
        let filter_fn = self.filter_fn();

        self.files
            .iter_mut()
            .filter(move |f| filter_fn(f))
    }

    #[allow(trivial_bounds)]
    pub fn filter_fn(&self) -> impl Fn(&File) -> bool + 'static {
        let filter = self.filter.clone();
        let filter_selected = self.filter_selected;
        let show_hidden = self.show_hidden;

        move |f| {
            f.kind == Kind::Placeholder ||
                !(filter.is_some() &&
                  !f.name.contains(filter.as_ref().unwrap())) &&
                (!filter_selected || f.selected) &&
                !(!show_hidden && f.name.starts_with("."))
        }
    }

    #[allow(trivial_bounds)]
    pub fn sorter(&self) -> impl Fn(&File, &File) -> std::cmp::Ordering {
        use std::cmp::Ordering::*;

        let dirs_first = self.dirs_first.clone();
        let sort = self.sort.clone();

        let dircmp = move |a: &File, b: &File| {
            match (a.is_dir(),  b.is_dir()) {
                (true, false) if dirs_first => Less,
                (false, true) if dirs_first => Greater,
                _ => Equal
            }
        };


        let reverse = self.reverse;
        let namecmp = move |a: &File, b: &File| {
            let (a, b) = match reverse {
                true => (b, a),
                false => (a, b),
            };

            compare(&a.name, &b.name)
        };

        let reverse = self.reverse;
        let sizecmp = move |a: &File, b: &File| {
            let (a, b) = match reverse {
                true => (b, a),
                false => (a, b),
            };

            match (a.meta(), b.meta()) {
                (Some(a_meta), Some(b_meta)) => {
                    let a_meta = a_meta.as_ref().unwrap();
                    let b_meta = b_meta.as_ref().unwrap();
                    match a_meta.size() == b_meta.size() {
                        true => compare(&b.name, &a.name),
                        false => b_meta.size().cmp(&a_meta.size())
                    }
                }
                _ => Equal
            }
        };

        let reverse = self.reverse;
        let timecmp = move |a: &File, b: &File| {
            let (a, b) = match reverse {
                true => (b, a),
                false => (a, b),
            };

            match (a.meta(), b.meta()) {
                (Some(a_meta), Some(b_meta)) => {
                    let a_meta = a_meta.as_ref().unwrap();
                    let b_meta = b_meta.as_ref().unwrap();
                    match a_meta.mtime() == b_meta.mtime() {
                        true => compare(&b.name, &a.name),
                        false => b_meta.mtime().cmp(&a_meta.mtime())
                    }
                }
                _ => Equal
            }
        };


        move |a, b| match sort {
            SortBy::Name => {
                match dircmp(a, b) {
                    Equal => namecmp(a, b),
                    ord @ _ => ord
                }
            },
            SortBy::Size => {
                match dircmp(a, b) {
                    Equal => sizecmp(a, b),
                    ord @ _ => ord
                }
            }
            SortBy::MTime => {
                match dircmp(a, b) {
                    Equal => timecmp(a, b),
                    ord @ _ => ord
                }
            }
        }
    }

    pub fn sort(&mut self) {
        let sort = self.sorter();

        self.files
            .par_sort_unstable_by(sort);
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

            // Need to recheck hidden files
            self.meta_upto = None;
        }

        self.recalculate_len();
    }

    fn remove_placeholder(&mut self) {
        let dirpath = self.directory.path.clone();
        self.find_file_with_path(&dirpath).cloned()
            .map(|placeholder| {
                if let Some(pos) = self.files.iter().position(|x| *x == placeholder) {
                    self.files.remove(pos);
                }
                if self.len > 0 {
                    self.len -= 1;
                }
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
                self.stale.as_ref().map(|s| s.set_fresh());
                refresh.pull_async()?;
                let mut refresh = refresh.value?;
                self.files = refresh.new_files.take()?;
                self.jobs.append(&mut refresh.jobs);
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

    pub fn process_fs_events(&mut self, sender: Sender<Events>) -> HResult<()> {
        let pending = self.pending_events.read()?.len();

        if pending > 0 {
            let events = std::mem::take(&mut *self.pending_events.write()?);

            let files = self.clone();

            let mut refresh = Async::new(move |_| {
                let refresh = RefreshPackage::new(files, events);
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

    pub fn find_file(&self, file: &File) -> Option<usize> {
        let comp = self.sorter();
        let pos = self.files
            .binary_search_by(|probe| comp(probe, file))
            .ok()?;

        debug_assert_eq!(file.path, self.files[pos].path);

        Some(pos)
    }

    pub fn find_file_with_name(&self, name: &str) -> Option<&File> {
        self.iter_files()
            .find(|f| f.name.to_lowercase().contains(name))
    }

    pub fn find_file_with_path(&mut self, path: &Path) -> Option<&mut File> {
        self.iter_files_mut().find(|file| file.path == path)
    }

    pub fn set_filter(&mut self, filter: Option<String>) {
        self.filter = filter;

        // Do this first, so we know len() == 0 needs a placeholder
        self.remove_placeholder();

        self.recalculate_len();

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
    pub dirsize: Option<Arc<(AtomicBool, AtomicUsize)>>,
    pub target: Option<PathBuf>,
    pub meta: Option<Arc<RwLock<Option<Metadata>>>>,
    pub selected: bool,
    pub tag: Option<bool>,
}

impl File {
    pub fn new(
        name: &str,
        path: PathBuf) -> File {
        let hidden = name.starts_with(".");

        File {
            name: name.to_string(),
            hidden: hidden,
            kind: if path.is_dir() { Kind::Directory } else { Kind::File },
            path: path,
            dirsize: None,
            target: None,
            meta: None,
            selected: false,
            tag: None,
        }
    }

    pub fn new_from_nixentry(direntry: Entry,
                             path: &Path,
                             dirfd: i32) -> File {
        // Scary stuff to avoid some of the overhead in Rusts conversions
        // Speedup is a solid ~10%
        let name: &OsStr = unsafe {
            use std::ffi::CStr;
            // &CStr -> &[u8]
            let s = direntry.file_name() as *const CStr;
            let s: &[u8] = s.cast::<&[u8]>().as_ref().unwrap();
            // &Cstr -> &OsStr, minus the NULL byte
            let len = s.len();
            let s = &s[..len-1] as *const [u8];
            s.cast::<&OsStr>().as_ref().unwrap()
        };

        // Avoid reallocation on push
        let mut pathstr = std::ffi::OsString::with_capacity(path.as_os_str().len() +
                                                            name.len() +
                                                            2);
        pathstr.push(path.as_os_str());
        pathstr.push("/");
        pathstr.push(name);

        let path = PathBuf::from(pathstr);

        let name = name.to_str()
                       .map(|n| String::from(n))
                       .unwrap_or_else(|| name.to_string_lossy().to_string());

        let hidden = name.as_bytes()[0] == b'.';

        let (kind, target) = match direntry.file_type() {
            Some(ftype) => match ftype {
                Type::Directory => (Kind::Directory, None),
                Type::Symlink => {
                    // Read link target
                    let target = nix::fcntl::readlinkat(dirfd, &path)
                        .map(PathBuf::from).ok();
                    let target_kind =
                        match path.is_dir() {
                                    true => Kind::Directory,
                                    false => Kind::File
                                };
                    (target_kind, target)
                }
                _ => (Kind::File, None)
            }
            _ => (Kind::Placeholder, None)
        };

        File {
            name: name,
            hidden: hidden,
            kind: kind,
            path: path,
            dirsize: None,
            target: target,
            meta: None,
            selected: false,
            tag: None,
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

    pub fn rename(&mut self, new_path: &Path) -> HResult<()> {
        self.name = new_path.file_name()?.to_string_lossy().to_string();
        self.path = new_path.into();
        Ok(())
    }

    pub fn set_dirsize(&mut self, dirsize: Arc<(AtomicBool, AtomicUsize)>) {
        self.dirsize = Some(dirsize);
    }

    pub fn refresh_meta_job(&mut self) -> Job {
        let meta = self.meta
            .as_ref()
            .map_or_else(|| Arc::default(),
                         |m| {
                             *m.write().unwrap() = None;
                             m.clone()
                         });


        (self.path.clone(), Some(meta), None)
    }

    pub fn prepare_meta_job(&mut self, cache: &FsCache) -> Option<Job> {
        let mslot = match self.meta {
            Some(_) => None,
            None => {
                let meta: Arc<RwLock<Option<Metadata>>> = Arc::default();
                self.meta = Some(meta.clone());
                Some(meta)
            }
        };

        let dslot = match self.dirsize {
            None if self.is_dir() => {
                let dslot = match cache.get_dirsize(self) {
                    Some(dslot) => dslot,
                    None => cache.make_dirsize(self)
                };
                self.set_dirsize(dslot.clone());
                Some(dslot)
            }
            _ => None
        };

        if mslot.is_some() || dslot.is_some() {
            let path = self.path.clone();
            Some((path, mslot, dslot))
        } else {
            None
        }
    }

    pub fn meta(&self) -> Option<std::sync::RwLockReadGuard<'_, Option<Metadata>>> {
        let meta = self.meta
            .as_ref()?
            .read()
            .ok();

        match meta {
            Some(meta) =>
                if meta.is_some() {
                    Some(meta)
                } else {
                    None
                },
            None => None
        }
    }

    pub fn get_color(&self) -> Option<String> {
        let meta = self.meta()?;
        let meta = meta.as_ref()?;
        match COLORS.style_for_path_with_metadata(&self.path, Some(&meta)) {
            // TODO: Also handle bg color, bold, etc.?
            Some(style) => style.foreground
                                .as_ref()
                                .map(|c| crate::term::from_lscolor(&c)),
            None => None,
        }
    }

    pub fn calculate_size(&self) -> HResult<(usize, &str)> {
        if self.is_dir() {
            let size = match self.dirsize {
                Some(ref dirsize) => {
                    let (ref ready, ref size) = **dirsize;
                    if ready.load(Ordering::Relaxed) == true {
                        (size.load(Ordering::Relaxed), "")
                    } else {
                        return Err(FileError::MetaPending)?;
                    }
                },
                None => (0, ""),
            };

            return Ok(size);
        }


        let mut unit = 0;
        let mut size = match self.meta() {
            Some(meta) => meta.as_ref().unwrap().size(),
            None => return Err(FileError::MetaPending)?
        };
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

        Ok((size as usize, unit))
    }

    // Sadly tree_magic tends to panic (in unwraps a None) when called
    // with things like pipes, non-existing files. and other stuff. To
    // prevent it from crashing hunter it's necessary to catch the
    // panic with a custom panic hook and handle it gracefully by just
    // doing nothing
    pub fn get_mime(&self) -> HResult<mime_guess::Mime> {
        use std::panic;
        use crate::fail::MimeError;

        if let Some(ext) = self.path.extension() {
            let mime = mime_guess::from_ext(&ext.to_string_lossy()).first();
            if mime.is_some() {
                return Ok(mime.unwrap());
            }
        }

        // Take and replace panic handler which does nothing
        let panic_hook = panic::take_hook();
        panic::set_hook(Box::new(|_| {} ));

        // Catch possible panic caused by tree_magic
        let mime = panic::catch_unwind(|| {
            let mime = tree_magic_fork::from_filepath(&self.path);
            mime.and_then(|m| mime::Mime::from_str(&m).ok())
        });

        // Restore previous panic handler
        panic::set_hook(panic_hook);

        mime.unwrap_or(None)
            .ok_or_else(|| {
                let file = self.name.clone();
                HError::Mime(MimeError::Panic(file))
            })
    }


    pub fn is_text(&self) -> bool {
        tree_magic_fork::match_filepath("text/plain", &self.path)
    }

    pub fn is_filtered(&self, filter: &str, filter_selected: bool) -> bool {
        self.kind == Kind::Placeholder ||
            !(// filter.is_some() &&
              !self.name.contains(filter// .as_ref().unwrap()
              )) &&
            (!filter_selected || self.selected)
    }

    pub fn is_hidden(&self) -> bool {
        self.hidden
    }


    pub fn parent(&self) -> Option<&Path> {
        self.path.parent()
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

    pub fn read_dir(&self) -> HResult<Files> {
        Files::new_from_path_cancellable(&self.path, Stale::new())
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

        self.save_tags()?;
        Ok(())
    }

    pub fn save_tags(&self) -> HResult<()> {
        if self.tag.is_none() { return Ok(()); }

        let path = self.path.clone();
        let state = self.tag.unwrap();

        std::thread::spawn(move || -> HResult<()> {
            use std::os::unix::ffi::OsStrExt;

            let tagfile_path = crate::paths::tagfile_path()?;
            let mut tags = TAGS.write()?;

            match state {
                true => {
                    match tags.1.binary_search(&path) {
                        Ok(_) => {},
                        Err(inspos) => tags.1.insert(inspos, path)
                    };
                },
                false => {
                    match tags.1.binary_search(&path) {
                        Ok(delpos) => { tags.1.remove(delpos); },
                        Err(_) => {}
                    };
                }
            }

            let tagstr = tags.1.iter()
                               .fold(std::ffi::OsString::new(), |mut s, f| {
                                   s.push(f);
                                   s.push("\n");
                                   s
                               });

            std::fs::write(tagfile_path, tagstr.as_bytes())?;
            Ok(())
        });
        Ok(())
    }

    pub fn is_readable(&self) -> HResult<bool> {
        let meta = self.meta()?;
        let meta = meta.as_ref()?;
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
        let meta = self.meta()?;
        let meta = meta.as_ref()?;

        let perms: usize = format!("{:o}", meta.mode()).parse().unwrap();
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
        let meta = self.meta()?;
        let meta = meta.as_ref()?;
        let uid = meta.uid();
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
        let meta = self.meta()?;
        let meta = meta.as_ref()?;
        let gid = meta.gid();
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
        let meta = self.meta()?;
        let meta = meta.as_ref()?;

        let time: chrono::DateTime<chrono::Local>
            = chrono::Local.timestamp(meta.mtime(), 0);
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



// Small wrapper that simplifies stopping with more complex control flow
pub struct Ticker {
    invalidated: bool
}

impl Ticker {
    pub fn start_ticking(sender: Sender<Events>) -> Self {
        start_ticking(sender);
        Ticker {
            invalidated: false
        }
    }

    pub fn stop_ticking(&mut self) {
        stop_ticking();
        self.invalidated = true;
    }
}

impl Drop for Ticker {
    fn drop(&mut self) {
        if !self.invalidated {
            self.stop_ticking();
        }
    }
}
