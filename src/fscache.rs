use notify::{RecommendedWatcher, Watcher, DebouncedEvent, RecursiveMode};

use async_value::{Async, Stale};

use std::sync::{Arc, RwLock};
use std::sync::mpsc::{channel, Sender, Receiver};
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use std::path::PathBuf;

use crate::files::{Files, File, SortBy};
use crate::widget::Events;
use crate::fail::{HResult, HError, ErrorLog, Backtrace, ArcBacktrace};


#[derive(Debug, Clone)]
pub struct DirSettings {
    sort: SortBy,
    dirs_first: bool,
    reverse: bool,
    show_hidden: bool,
    filter: Option<String>,
    filter_selected: bool
}

impl DirSettings {
    fn new() -> DirSettings {
        DirSettings {
            sort: SortBy::Name,
            dirs_first: true,
            reverse: false,
            show_hidden: true,
            filter: None,
            filter_selected: false
        }
    }
}

#[derive(Debug, Clone)]
pub struct TabSettings {
    selection: Option<File>,
    multi_selections: Vec<File>,
    dir_settings: DirSettings,
}

impl TabSettings {
    fn new() -> TabSettings {
        TabSettings {
            selection: None,
            multi_selections: vec![],
            dir_settings: DirSettings::new()
        }
    }
}


impl std::fmt::Debug for FsCache {
    fn fmt(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(formatter,
               "{:?}\n{:?}\n{:?}",
               self.tab_settings,
               self.watched_dirs,
               self.files)
    }
}


#[derive(Clone)]
pub struct FsCache {
    files: Arc<RwLock<HashMap<File, Files>>>,
    pub tab_settings: Arc<RwLock<HashMap<File, TabSettings>>>,
    watched_dirs: Arc<RwLock<HashSet<File>>>,
    watcher: Arc<RwLock<RecommendedWatcher>>,
    pub fs_changes: Arc<RwLock<Vec<(File, Option<File>, Option<File>)>>>,
}

impl FsCache {
    pub fn new(sender: Sender<Events>) -> FsCache {
        let (tx_fs_event, rx_fs_event) = channel();
        let watcher = RecommendedWatcher::new(tx_fs_event,
                                          Duration::from_secs(2)).unwrap();


        let fs_cache = FsCache {
            files: Arc::new(RwLock::new(HashMap::new())),
            tab_settings: Arc::new(RwLock::new(HashMap::new())),
            watched_dirs: Arc::new(RwLock::new(HashSet::new())),
            watcher: Arc::new(RwLock::new(watcher)),
            fs_changes: Arc::new(RwLock::new(vec![])),
        };

        watch_fs(rx_fs_event,
                 fs_cache.files.clone(),
                 fs_cache.fs_changes.clone(),
                 sender);

        fs_cache
    }

    pub fn new_client(&self, settings: HashMap<File, TabSettings>) -> HResult<FsCache> {
        let mut cache = self.clone();
        cache.tab_settings = Arc::new(RwLock::new(settings));
        Ok(cache)
    }
}

pub type CachedFiles = (Option<File>, Async<Files>);

impl FsCache {
    pub fn get_files(&self, dir: &File, stale: Stale) -> HResult<CachedFiles> {
        if self.files.read()?.contains_key(dir) {
            self.get_cached_files(dir)
        } else {
            let dir = dir.clone();
            let selection = self.get_selection(&dir).ok();
            let cache = self.clone();
            let files = Async::new(move |_| {
                let mut files = Files::new_from_path_cancellable(&dir.path, stale)?;
                cache.add_watch(&dir).log();
                FsCache::apply_settingss(&cache, &mut files).ok();
                Ok(files)
            });
            Ok((selection, files))
        }
    }

    pub fn get_files_sync(&self, dir: &File) -> HResult<Files> {
        let files = self.get_files(&dir, Stale::new())?.1;
        let mut files = files.run_sync()?;
        FsCache::apply_settingss(&self, &mut files).ok();
        let files = FsCache::ensure_not_empty(files)?;
        self.add_watch(&dir).log();
        Ok(files)
    }

    pub fn get_selection(&self, dir: &File) -> HResult<File> {
        Ok(self.tab_settings.read()?.get(&dir).as_ref()?.selection.as_ref()?.clone())
    }

    pub fn save_settings(&self, files: &Files, selection: Option<File>) -> HResult<()> {
        let dir = files.directory.clone();
        let tab_settings = FsCache::extract_tab_settings(&files, selection);
        self.tab_settings.write()?.insert(dir, tab_settings);
        Ok(())
    }

    pub fn put_files(&self, files: &Files, selection: Option<File>) -> HResult<()> {
        let dir = files.directory.clone();

        let tab_settings = FsCache::extract_tab_settings(&files, selection);

        self.tab_settings.write()?.insert(dir.clone(), tab_settings);

        // let mut file_cache = self.files.write()?;

        // if file_cache.contains_key(&files.directory) {
        //     if files.meta_updated {
        //         let mut files = files.clone();
        //         files.meta_updated = false;
        //         file_cache.insert(dir, files);
        //     }
        // } else {
        //     file_cache.insert(dir, files.clone());
        // }

        Ok(())
    }

    pub fn is_cached(&self, dir: &File) -> HResult<bool> {
        Ok(self.files.read()?.contains_key(dir))
    }

    pub fn watch_only(&self, open_dirs: HashSet<File>) -> HResult<()> {
        let removable = self.watched_dirs
            .read()?
            .difference(&open_dirs)
            .map(|dir| dir.clone())
            .collect::<Vec<File>>();

        for watch in removable {
            self.remove_watch(&watch).log();
        }

        Ok(())
    }

    fn add_watch(&self, dir: &File) -> HResult<()> {
        if !self.watched_dirs.read()?.contains(&dir) {
            self.watcher.write()?.watch(&dir.path, RecursiveMode::NonRecursive)?;
            self.watched_dirs.write()?.insert(dir.clone());
        }
        Ok(())
    }

    fn remove_watch(&self, dir: &File) -> HResult<()> {
        if self.watched_dirs.read()?.contains(&dir) {
            self.watched_dirs.write()?.remove(dir);
            self.watcher.write()?.unwatch(&dir.path)?
        }
        Ok(())
    }

    fn get_cached_files(&self, dir: &File) -> HResult<CachedFiles> {
        let tab_settings = match self.tab_settings.read()?.get(&dir) {
                Some(tab_settings) => tab_settings.clone(),
                None => TabSettings::new()
        };
        let selection = tab_settings.selection.clone();
        let file_cache = self.files.clone();
        let dir = dir.clone();

        let files = Async::new(move |_| {
            let mut files = file_cache.read()
                .map_err(|e| HError::from(e))?
                .get(&dir)
                .ok_or(HError::NoneError(Backtrace::new_arced()))?
                .clone();
            let tab_settings = &tab_settings;

            files.sort = tab_settings.dir_settings.sort;
            files.dirs_first = tab_settings.dir_settings.dirs_first;
            files.reverse = tab_settings.dir_settings.reverse;
            files.show_hidden = tab_settings.dir_settings.show_hidden;
            files.filter = tab_settings.dir_settings.filter.clone();

            if tab_settings.multi_selections.len() > 0 {
                for file in &mut files.files {
                    for selected_files in &tab_settings.multi_selections {
                        if file.path == selected_files.path {
                            file.selected = true;
                        }
                    }
                }
            }

            files.sort();
            let files = FsCache::ensure_not_empty(files)?;
            Ok(files)
        });

        Ok((selection, files))
    }


    pub fn apply_settingss(cache: &FsCache,
                       files: &mut Files)
                       -> HResult<()> {
        let dir = &files.directory;
        let tab_settings = cache.tab_settings.read()?.get(&dir).cloned();
        if tab_settings.is_none() { return Ok(()) }
        let tab_settings = tab_settings?;

        files.sort = tab_settings.dir_settings.sort;
        files.dirs_first = tab_settings.dir_settings.dirs_first;
        files.reverse = tab_settings.dir_settings.reverse;
        files.show_hidden = tab_settings.dir_settings.show_hidden;
        files.filter = tab_settings.dir_settings.filter.clone();
        files.filter_selected = tab_settings.dir_settings.filter_selected;

        if tab_settings.multi_selections.len() > 0 {
            for file in &mut files.files {
                for selected_files in &tab_settings.multi_selections {
                    if file.path == selected_files.path {
                        file.selected = true;
                    }
                }
            }
        }

        files.sort();
        Ok(())
    }

    pub fn ensure_not_empty(mut files: Files) -> HResult<Files> {
        if files.len() == 0 {
            let path = &files.directory.path;
            let placeholder = File::new_placeholder(&path)?;
            files.files.push(placeholder);
        }
        Ok(files)
    }


    fn extract_tab_settings(files: &Files, selection: Option<File>) -> TabSettings {
        TabSettings {
            selection: selection,
            multi_selections: files.get_selected().into_iter().cloned().collect(),
            dir_settings: DirSettings {
                sort: files.sort,
                dirs_first: files.dirs_first,
                reverse: files.reverse,
                show_hidden: files.show_hidden,
                filter: files.filter.clone(),
                filter_selected: files.filter_selected
            }
        }
    }
}


fn watch_fs(rx_fs_events: Receiver<DebouncedEvent>,
            fs_cache: Arc<RwLock<HashMap<File, Files>>>,
            fs_changes: Arc<RwLock<Vec<(File, Option<File>, Option<File>)>>>,
            sender: Sender<Events>) {
    std::thread::spawn(move || -> HResult<()> {
        for event in rx_fs_events.iter() {
            apply_event(&fs_cache, &fs_changes, event).log();

            sender.send(Events::WidgetReady).ok();
        }
        Ok(())
    });
}

fn apply_event(_fs_cache: &Arc<RwLock<HashMap<File, Files>>>,
               fs_changes: &Arc<RwLock<Vec<(File, Option<File>, Option<File>)>>>,
               event: DebouncedEvent)
               -> HResult<()> {
    let path = &event.get_source_path()?;

    let dirpath = path.parent()
        .map(|path| path.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("/"));
    let dir = File::new_from_path(&dirpath, None)?;

    let old_file = File::new_from_path(&path, None)?;
    let mut new_file = match event {
        DebouncedEvent::Remove(_) => None,
        _ => Some(File::new_from_path(&path, None)?)
    };

    new_file.as_mut().map(|file| file.meta_sync());

    fs_changes.write()?.push((dir,
                              Some(old_file),
                              new_file));

    // for dir in fs_cache.write()?.values_mut() {
    //     if dir.path_in_here(&path).unwrap_or(false) {
    //         let old_file = dir.find_file_with_path(&path).cloned();
    //         let dirty_meta = old_file
    //             .as_ref()
    //             .map(|f| f.dirty_meta.clone())
    //             .unwrap_or(None);
    //         let mut new_file = match event {
    //             DebouncedEvent::Remove(_) => None,
    //             _ => Some(File::new_from_path(&path, dirty_meta)?)
    //         };

    //         new_file.as_mut().map(|file| file.meta_sync());
    //         dir.replace_file(old_file.as_ref(), new_file.clone()).log();

    //         fs_changes.write()?.push((dir.directory.clone(),
    //                                   old_file,
    //                                   new_file));
    //     }
    // }
    Ok(())
}

trait PathFromEvent {
    fn get_source_path(&self) -> HResult<&PathBuf>;
}

impl PathFromEvent for DebouncedEvent {
    fn get_source_path(&self) -> HResult<&PathBuf> {
        match self {
            DebouncedEvent::Create(path)      |
            DebouncedEvent::Write(path)       |
            DebouncedEvent::Chmod(path)       |
            DebouncedEvent::Remove(path)      |
            DebouncedEvent::NoticeWrite(path) |
            DebouncedEvent::NoticeRemove(path)  => Ok(path),
            DebouncedEvent::Rename(old_path, _) => Ok(old_path),
            DebouncedEvent::Error(err, path)
                => Err(HError::INotifyError(format!("{}, {:?}", err, path),
                                            Backtrace::new_arced())),
            DebouncedEvent::Rescan
                => Err(HError::INotifyError("Need to rescan".to_string(),
                                            Backtrace::new_arced()))

        }
    }
}
