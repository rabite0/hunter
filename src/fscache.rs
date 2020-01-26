use notify::{RecommendedWatcher, Watcher, DebouncedEvent, RecursiveMode};

use async_value::{Async, Stale};

use std::sync::{Arc, RwLock, Weak};
use std::sync::mpsc::{channel, Sender, Receiver};
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use std::path::PathBuf;

use crate::files::{Files, File, SortBy};
use crate::widget::Events;
use crate::fail::{HResult, HError, ErrorLog};

pub type CachedFiles = (Option<File>, Async<Files>);


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
struct FsEventDispatcher {
    targets: Arc<RwLock<HashMap<File, Vec<Weak<RwLock<Vec<FsEvent>>>>>>>
}

impl FsEventDispatcher {
    fn new() -> Self {
        FsEventDispatcher {
            targets: Arc::new(RwLock::new(HashMap::new()))
        }
    }

    fn add_target(&self,
                  dir: &File,
                  target: &Arc<RwLock<Vec<FsEvent>>>) -> HResult<()> {
        let target = Arc::downgrade(target);

        self.targets
            .write()
            .map(|mut targets| {
                match targets.get_mut(dir) {
                    Some(targets) => targets.push(target),
                    None => { targets.insert(dir.clone(), vec![target]); }
                }
            })?;
        Ok(())
    }

    fn remove_target(&self, dir: &File) -> HResult<()> {
        self.targets
            .write()?
            .get_mut(dir)
            .map(|targets| {
                targets.retain(|t| t.upgrade().is_some());
            });
        Ok(())
    }

    fn dispatch(&self, events: HashMap<File, Vec<FsEvent>>) -> HResult<()> {
        for (dir, events) in events {
            for target_dirs in self.targets
                .read()?
                .get(&dir) {
                    for target in target_dirs {
                        if let Some(target) = target.upgrade() {
                            let events = events.clone();

                            target.write()?.extend(events)
                        }
                    }
                }
        }
        Ok(())
    }

    // fn remove_unnecessary
}

#[derive(Clone)]
pub struct FsCache {
    files: Arc<RwLock<HashMap<File, Files>>>,
    pub tab_settings: Arc<RwLock<HashMap<File, TabSettings>>>,
    watched_dirs: Arc<RwLock<HashSet<File>>>,
    watcher: Arc<RwLock<RecommendedWatcher>>,
    fs_event_dispatcher: FsEventDispatcher
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
            fs_event_dispatcher: FsEventDispatcher::new()
        };

        watch_fs(rx_fs_event,
                 fs_cache.fs_event_dispatcher.clone(),
                 sender);

        fs_cache
    }

    pub fn new_client(&self, settings: HashMap<File, TabSettings>) -> HResult<FsCache> {
        let mut cache = self.clone();
        cache.tab_settings = Arc::new(RwLock::new(settings));
        Ok(cache)
    }
}

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
                cache.fs_event_dispatcher.add_target(&dir,
                                                     &files.pending_events).log();
                FsCache::apply_settingss(&cache, &mut files).ok();
                files.sort();
                Ok(files)
            });
            Ok((selection, files))
        }
    }

    pub fn get_files_sync_stale(&self, dir: &File, stale: Stale) -> HResult<Files> {
        let files = self.get_files(&dir, stale)?.1;
        let files = files.run_sync()?;
        let files = FsCache::ensure_not_empty(files)?;
        Ok(files)
    }

    pub fn get_files_sync(&self, dir: &File) -> HResult<Files> {
        let files = self.get_files(&dir, Stale::new())?.1;
        let files = files.run_sync()?;
        let files = FsCache::ensure_not_empty(files)?;
        Ok(files)
    }

    pub fn get_selection(&self, dir: &File) -> HResult<File> {
        Ok(self.tab_settings
           .read()?
           .get(&dir)
           .as_ref()?
           .selection
           .as_ref()?
           .clone())
    }

    pub fn set_selection(&self, dir: File, selection: File) -> HResult<()> {
        self.tab_settings.write()
            .map(|mut settings| {
                let setting = settings.entry(dir).or_insert(TabSettings::new());
                setting.selection = Some(selection);
            })?;
        Ok(())
    }

    pub fn save_settings(&self, files: &Files, selection: Option<File>) -> HResult<()> {
        let dir = files.directory.clone();
        let tab_settings = FsCache::extract_tab_settings(&files, selection);
        self.tab_settings.write()?.insert(dir, tab_settings);
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
                .ok_or(HError::NoneError)?
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

        if files.show_hidden != tab_settings.dir_settings.show_hidden ||
            files.filter != tab_settings.dir_settings.filter ||
            files.filter_selected != tab_settings.dir_settings.filter_selected {
                files.recalculate_len();
            }

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

        Ok(())
    }

    pub fn ensure_not_empty(mut files: Files) -> HResult<Files> {
        if files.len() == 0 {
            let path = &files.directory.path;
            let placeholder = File::new_placeholder(&path)?;
            files.files.push(placeholder);
            files.len = 1;
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


#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub enum FsEvent {
    Create(File),
    Change(File),
    Rename(File, File),
    Remove(File)
}

impl FsEvent {
    pub fn file(&self) -> &File {
        use FsEvent::*;
        match self {
            Create(event_file) |
            Change(event_file) |
            Remove(event_file) |
            Rename(_, event_file) => &event_file
        }
    }

    pub fn for_file(&self, file: &File) -> bool {
        use FsEvent::*;
        match self {
            Create(event_file) |
            Change(event_file) |
            Remove(event_file) |
            Rename(_, event_file) => event_file.path == file.path
        }
    }
}

use std::convert::TryFrom;
impl TryFrom<DebouncedEvent> for FsEvent {
    type Error = HError;

    fn try_from(event: DebouncedEvent) -> HResult<Self> {
        let event = match event {
            DebouncedEvent::Create(path)
                => FsEvent::Create(File::new_from_path(&path, None)?),

            DebouncedEvent::Remove(path)
                => FsEvent::Remove(File::new_from_path(&path, None)?),

            DebouncedEvent::Write(path)       |
            DebouncedEvent::Chmod(path)
                =>  FsEvent::Change(File::new_from_path(&path, None)?),

            DebouncedEvent::Rename(old_path, new_path)
                => FsEvent::Rename(File::new_from_path(&old_path, None)?,
                                   File::new_from_path(&new_path, None)?),

            DebouncedEvent::Error(err, path)
                => Err(HError::INotifyError(format!("{}, {:?}", err, path)))?,
            DebouncedEvent::Rescan
                => Err(HError::INotifyError("Need to rescan".to_string()))?,
            // Ignore NoticeRemove/NoticeWrite
            _ => None?,
        };

        Ok(event)
    }
}


fn watch_fs(rx_fs_events: Receiver<DebouncedEvent>,
            fs_event_dispatcher: FsEventDispatcher,
            sender: Sender<Events>) {
    std::thread::spawn(move || -> HResult<()> {
        let transform_event =
            move |event: DebouncedEvent| -> HResult<(File, FsEvent)> {
                let path = event.get_source_path()?;
                let dirpath = path.parent()
                    .map(|path| path)
                    .unwrap_or(std::path::Path::new("/"));
                let dir = File::new_from_path(&dirpath, None)?;
                let event = FsEvent::try_from(event)?;
                Ok((dir, event))
            };

        let collect_events =
            move || -> HResult<HashMap<File, Vec<FsEvent>>> {
                let event = loop {
                    use DebouncedEvent::*;

                    let event = rx_fs_events.recv()?;
                    match event {
                        NoticeWrite(_) => continue,
                        NoticeRemove(_) => continue,
                        _ => break std::iter::once(event)
                    }
                };

                // Wait a bit to batch up more events
                std::thread::sleep(std::time::Duration::from_millis(100));

                // Batch up all other remaining events received so far
                let events = event.chain(rx_fs_events.try_iter())
                    .map(transform_event)
                    .flatten()
                    .fold(HashMap::with_capacity(1000), |mut events, (dir, event)| {
                        events.entry(dir)
                            .or_insert(vec![])
                            .push(event);

                        events
                    });

                Ok(events)
            };


        let dispatch_events =
            move |events| -> HResult<()> {
                fs_event_dispatcher.dispatch(events)?;
                sender.send(Events::WidgetReady)?;
                Ok(())
            };

        loop {
            if let Ok(events) = collect_events().log_and() {
                dispatch_events(events).log();
            }
        }
    });
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
                => Err(HError::INotifyError(format!("{}, {:?}", err, path))),
            DebouncedEvent::Rescan
                => Err(HError::INotifyError("Need to rescan".to_string()))

        }
    }
}
