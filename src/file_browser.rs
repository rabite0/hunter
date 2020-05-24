use async_value::Stale;
use osstrtools::OsStrTools;
use pathbuftools::PathBufTools;
use termion::event::Key;

use std::collections::HashSet;
use std::ffi::OsString;
use std::io::Write;
use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};

use crate::bookmarks::BMPopup;
use crate::coordinates::Coordinates;
use crate::dirty::Dirtyable;
use crate::fail::{ErrorLog, HError, HResult};
use crate::files::{File, Files};
use crate::foldview::LogView;
use crate::fscache::FsCache;
use crate::hbox::HBox;
use crate::listview::{FileSource, ListView};
use crate::preview::{AsyncWidget, Previewer};
use crate::proclist::ProcView;
use crate::stats::{FsExt, FsStat};
use crate::tabview::{TabView, Tabbable};
use crate::term;
use crate::term::ScreenExt;
use crate::textview::TextView;
use crate::widget::Widget;
use crate::widget::{Events, WidgetCore};

#[derive(PartialEq)]
pub enum FileBrowserWidgets {
    FileList(AsyncWidget<ListView<Files>>),
    Previewer(Previewer),
    Blank(AsyncWidget<TextView>),
}

impl Widget for FileBrowserWidgets {
    fn get_core(&self) -> HResult<&WidgetCore> {
        match self {
            FileBrowserWidgets::FileList(widget) => widget.get_core(),
            FileBrowserWidgets::Previewer(widget) => widget.get_core(),
            FileBrowserWidgets::Blank(widget) => widget.get_core(),
        }
    }
    fn get_core_mut(&mut self) -> HResult<&mut WidgetCore> {
        match self {
            FileBrowserWidgets::FileList(widget) => widget.get_core_mut(),
            FileBrowserWidgets::Previewer(widget) => widget.get_core_mut(),
            FileBrowserWidgets::Blank(widget) => widget.get_core_mut(),
        }
    }
    fn set_coordinates(&mut self, coordinates: &Coordinates) -> HResult<()> {
        match self {
            FileBrowserWidgets::FileList(widget) => widget.set_coordinates(coordinates),
            FileBrowserWidgets::Previewer(widget) => widget.set_coordinates(coordinates),
            FileBrowserWidgets::Blank(widget) => widget.set_coordinates(coordinates),
        }
    }
    fn refresh(&mut self) -> HResult<()> {
        match self {
            FileBrowserWidgets::FileList(widget) => widget.refresh(),
            FileBrowserWidgets::Previewer(widget) => widget.refresh(),
            FileBrowserWidgets::Blank(widget) => widget.refresh(),
        }
    }
    fn get_drawlist(&self) -> HResult<String> {
        match self {
            FileBrowserWidgets::FileList(widget) => widget.get_drawlist(),
            FileBrowserWidgets::Previewer(widget) => widget.get_drawlist(),
            FileBrowserWidgets::Blank(widget) => widget.get_drawlist(),
        }
    }

    fn on_key(&mut self, key: Key) -> HResult<()> {
        match self {
            FileBrowserWidgets::FileList(widget) => widget.on_key(key),
            FileBrowserWidgets::Previewer(widget) => widget.on_key(key),
            FileBrowserWidgets::Blank(widget) => widget.on_key(key),
        }
    }
}

pub struct FileBrowser {
    pub columns: HBox<FileBrowserWidgets>,
    pub cwd: File,
    pub prev_cwd: Option<File>,
    core: WidgetCore,
    proc_view: Arc<Mutex<ProcView>>,
    bookmarks: Arc<Mutex<BMPopup>>,
    log_view: Arc<Mutex<LogView>>,
    fs_cache: FsCache,
    fs_stat: Arc<RwLock<FsStat>>,
}

impl Tabbable for TabView<FileBrowser> {
    type Tab = FileBrowser;

    fn new_tab(&mut self) -> HResult<()> {
        self.active_tab_mut().save_tab_settings().log();

        let cur_tab = self.active_tab();
        let settings = cur_tab.fs_cache.tab_settings.read()?.clone();
        let cache = cur_tab.fs_cache.new_client(settings).ok();

        let mut tab = FileBrowser::new(&self.active_tab_().core, cache)?;

        let proc_view = cur_tab.proc_view.clone();
        let bookmarks = cur_tab.bookmarks.clone();
        let log_view = cur_tab.log_view.clone();
        tab.proc_view = proc_view;
        tab.bookmarks = bookmarks;
        tab.log_view = log_view;
        tab.fs_stat = cur_tab.fs_stat.clone();

        self.push_widget(tab)?;
        self.active = self.widgets.len() - 1;
        Ok(())
    }

    fn close_tab(&mut self) -> HResult<()> {
        self.close_tab_().log();
        Ok(())
    }

    fn next_tab(&mut self) -> HResult<()> {
        self.next_tab_();
        Ok(())
    }

    fn prev_tab(&mut self) -> HResult<()> {
        self.prev_tab_();
        Ok(())
    }

    fn goto_tab(&mut self, index: usize) -> HResult<()> {
        self.goto_tab_(index)
    }

    fn get_tab_names(&self) -> Vec<Option<String>> {
        self.widgets
            .iter()
            .map(|filebrowser| {
                let path = filebrowser.cwd.path();
                let last_dir = path.components().last().unwrap();
                let dir_name = last_dir.as_os_str().to_string_lossy().to_string();
                Some(dir_name)
            })
            .collect()
    }

    fn active_tab(&self) -> &Self::Tab {
        self.active_tab_()
    }

    fn active_tab_mut(&mut self) -> &mut Self::Tab {
        self.active_tab_mut_()
    }

    fn on_tab_switch(&mut self) -> HResult<()> {
        self.active_tab_mut().refresh()
    }

    fn on_key_sub(&mut self, key: Key) -> HResult<()> {
        match self.active_tab_mut().on_key(key) {
            // returned by specific tab when called with ExecCmd action
            Err(HError::FileBrowserNeedTabFiles) => {
                let tab_dirs = self
                    .widgets
                    .iter()
                    .map(|w| w.cwd.clone())
                    .collect::<Vec<_>>();
                let selected_files = self
                    .widgets
                    .iter()
                    .map(|w| w.selected_files().unwrap_or(vec![]))
                    .collect();

                self.widgets[self.active].exec_cmd(tab_dirs, selected_files)
            }
            result @ _ => result,
        }
    }

    fn on_refresh(&mut self) -> HResult<()> {
        let open_dirs = self.widgets.iter().fold(HashSet::new(), |mut dirs, tab| {
            tab.left_dir().map(|dir| dirs.insert(dir.clone())).ok();
            dirs.insert(tab.cwd.clone());
            tab.preview_widget()
                .map(|preview| {
                    preview.get_file().map(|file| {
                        if file.is_dir() {
                            dirs.insert(file.clone());
                        }
                    })
                })
                .ok();
            dirs
        });

        self.active_tab_mut_().fs_cache.watch_only(open_dirs).log();
        self.active_tab_mut_().fs_stat.write()?.refresh().log();
        Ok(())
    }

    fn on_config_loaded(&mut self) -> HResult<()> {
        let show_hidden = self.core.config().show_hidden();

        for tab in self.widgets.iter_mut() {
            tab.left_async_widget_mut()
                .map(|async_w| {
                    async_w
                        .widget
                        .on_ready(move |mut w, _| {
                            w.as_mut()
                                .map(|w| {
                                    if w.content.show_hidden != show_hidden {
                                        w.content.show_hidden = show_hidden;
                                        w.content.recalculate_len();
                                        w.refresh().log();
                                    }
                                })
                                .ok();
                            Ok(())
                        })
                        .log();
                })
                .log();

            tab.main_async_widget_mut()
                .map(|async_w| {
                    async_w
                        .widget
                        .on_ready(move |mut w, _| {
                            w.as_mut()
                                .map(|w| {
                                    if w.content.show_hidden != show_hidden {
                                        w.content.show_hidden = show_hidden;
                                        w.content.recalculate_len();
                                        w.refresh().log();
                                    }
                                })
                                .ok();
                            Ok(())
                        })
                        .log()
                })
                .log();

            tab.preview_widget_mut().map(|w| w.config_loaded()).ok();
            tab.columns.set_ratios(self.core.config().ratios);
        }
        Ok(())
    }
}

impl FileBrowser {
    pub fn new(core: &WidgetCore, cache: Option<FsCache>) -> HResult<FileBrowser> {
        let fs_cache = cache.unwrap_or_else(|| FsCache::new(core.get_sender()));

        let cwd = std::env::current_dir().unwrap();
        let mut core_m = core.clone();
        let mut core_l = core.clone();
        let mut core_p = core.clone();

        let mut columns = HBox::new(core);
        columns.set_ratios(core.config().ratios);
        let list_coords = columns.calculate_coordinates()?;

        core_l.coordinates = list_coords[0].clone();
        core_m.coordinates = list_coords[1].clone();
        core_p.coordinates = list_coords[2].clone();

        let main_path = cwd
            .ancestors()
            .take(1)
            .map(|path| std::path::PathBuf::from(path))
            .last()
            .ok_or_else(|| HError::NoneError)?;
        let left_path = main_path.parent().map(|p| p.to_path_buf());

        let cache = fs_cache.clone();
        let main_widget = AsyncWidget::new(&core, move |stale| {
            let dir = File::new_from_path(&main_path)?;
            let source = FileSource::Path(dir);
            ListView::builder(core_m, source)
                .with_cache(cache)
                .with_stale(stale.clone())
                .build()
        });

        let cache = fs_cache.clone();
        if let Some(left_path) = left_path {
            let left_widget = AsyncWidget::new(&core_l.clone(), move |stale| {
                let dir = File::new_from_path(&left_path)?;
                let source = FileSource::Path(dir);
                ListView::builder(core_l, source)
                    .with_cache(cache)
                    .with_stale(stale.clone())
                    .build()
            });
            let left_widget = FileBrowserWidgets::FileList(left_widget);
            columns.push_widget(left_widget);
        } else {
            let mut left_widget = AsyncWidget::new(&core_l.clone(), move |_| {
                let files = Files::default();
                let source = FileSource::Files(files);
                ListView::builder(core_l, source).build()
            });

            left_widget
                .widget
                .on_ready(move |_, stale| {
                    // To stop from drawing empty placeholder
                    stale.set_stale()?;
                    Ok(())
                })
                .log();

            let left_widget = FileBrowserWidgets::FileList(left_widget);
            columns.push_widget(left_widget);
        }

        let previewer = Previewer::new(&core_p, fs_cache.clone());

        columns.push_widget(FileBrowserWidgets::FileList(main_widget));
        columns.push_widget(FileBrowserWidgets::Previewer(previewer));
        columns.set_active(1).log();
        columns.refresh().log();

        let cwd = File::new_from_path(&cwd).unwrap();

        let proc_view = ProcView::new(&core);
        let bookmarks = BMPopup::new(&core);
        let log_view = LogView::new(&core, vec![]);
        let fs_stat = FsStat::new().unwrap();

        Ok(FileBrowser {
            columns: columns,
            cwd: cwd,
            prev_cwd: None,
            core: core.clone(),
            proc_view: Arc::new(Mutex::new(proc_view)),
            bookmarks: Arc::new(Mutex::new(bookmarks)),
            log_view: Arc::new(Mutex::new(log_view)),
            fs_cache: fs_cache,
            fs_stat: Arc::new(RwLock::new(fs_stat)),
        })
    }

    pub fn enter_dir(&mut self) -> HResult<()> {
        let file = self.selected_file()?;

        if file.is_dir() {
            let dir = file;
            match dir.is_readable() {
                Ok(true) => {}
                Ok(false) => {
                    let status = format!(
                        "{}Stop right there, cowboy! Check your permisions!",
                        term::color_red()
                    );
                    self.core.show_status(&status).log();
                    return Ok(());
                }
                err @ Err(_) => err.log(),
            }
            self.preview_widget_mut()?.set_stale().log();
            self.preview_widget_mut()?.cancel_animation().log();
            let previewer_files = self.preview_widget_mut()?.take_files().ok();
            let main_files = self.take_main_files().ok();

            self.prev_cwd = Some(self.cwd.clone());
            self.cwd = dir.clone();

            let cache = self.fs_cache.clone();
            self.main_async_widget_mut()?
                .change_to(move |stale, core| {
                    let source = match previewer_files {
                        Some(files) => FileSource::Files(files),
                        None => FileSource::Path(dir),
                    };

                    ListView::builder(core, source)
                        .with_cache(cache)
                        .with_stale(stale.clone())
                        .build()
                })
                .log();

            let cache = self.fs_cache.clone();
            let left_dir = self.cwd.parent_as_file()?;
            self.left_async_widget_mut()?
                .change_to(move |stale, core| {
                    let source = match main_files {
                        Some(files) => FileSource::Files(files),
                        None => FileSource::Path(left_dir),
                    };

                    ListView::builder(core, source)
                        .with_cache(cache)
                        .with_stale(stale.clone())
                        .build()
                })
                .log();
        } else {
            self.preview_widget_mut()
                .map(|preview| {
                    preview.cancel_animation().log();
                })
                .log();
            self.core.get_sender().send(Events::InputEnabled(false))?;
            self.core.screen.suspend().log();

            let status = std::process::Command::new("xdg-open")
                .args(file.path.file_name())
                .status();

            self.core.screen.activate().log();
            self.core.clear().log();

            self.core.get_sender().send(Events::InputEnabled(true))?;

            match status {
                Ok(status) => self
                    .core
                    .show_status(&format!("\"{}\" exited with {}", "xdg-open", status))
                    .log(),
                Err(err) => self
                    .core
                    .show_status(&format!("Can't run this \"{}\": {}", "xdg-open", err))
                    .log(),
            }
        }

        Ok(())
    }

    pub fn move_down_left_widget(&mut self) -> HResult<()> {
        let left_files_pos = self.left_widget()?.get_selection();

        let next_dir = self
            .get_left_files()?
            .iter_files()
            .skip(left_files_pos + 1)
            .find(|&file| file.is_dir())
            .cloned();

        self.main_widget_goto(&next_dir.ok_or_else(|| HError::NoneError)?)
            .log();

        Ok(())
    }

    pub fn move_up_left_widget(&mut self) -> HResult<()> {
        let left_files_pos = self.left_widget()?.get_selection();

        let next_dir = self
            .get_left_files()?
            .iter_files()
            .take(left_files_pos)
            .collect::<Vec<&File>>()
            .into_iter()
            .rev()
            .find(|&file| file.is_dir())
            .cloned();

        self.main_widget_goto(&next_dir.ok_or_else(|| HError::NoneError)?)
            .log();

        Ok(())
    }

    pub fn open_bg(&mut self) -> HResult<()> {
        let cwd = self.cwd()?;
        let file = self.selected_file()?;

        let cmd = crate::proclist::Cmd {
            cmd: OsString::from(file.strip_prefix(&cwd)),
            short_cmd: None,
            args: None,
            vars: None,
            cwd: cwd.clone(),
            cwd_files: None,
            tab_files: None,
            tab_paths: None,
        };

        self.proc_view.lock()?.run_proc_raw(cmd)?;

        Ok(())
    }

    pub fn main_widget_goto_wait(&mut self, dir: &File) -> HResult<()> {
        self.main_widget_goto(&dir)?;

        // replace this with on_ready_mut() later
        let pause = std::time::Duration::from_millis(10);
        while self.main_widget().is_err() {
            self.main_async_widget_mut()?.refresh().log();
            std::thread::sleep(pause);
        }

        Ok(())
    }

    pub fn main_widget_goto(&mut self, dir: &File) -> HResult<()> {
        self.preview_widget_mut().map(|p| p.set_stale()).ok();

        let dir = dir.clone();
        let cache = self.fs_cache.clone();

        self.prev_cwd = Some(self.cwd.clone());
        self.cwd = dir.clone();
        let file_source = FileSource::Path(self.cwd.clone());

        let main_async_widget = self.main_async_widget_mut()?;
        main_async_widget
            .change_to(move |stale: &Stale, core| {
                let view = ListView::builder(core, file_source)
                    .with_cache(cache)
                    .with_stale(stale.clone())
                    .build()?;

                Ok(view)
            })
            .log();

        if let Ok(grand_parent) = self.cwd()?.parent_as_file() {
            self.left_widget_goto(&grand_parent).log();
        } else {
            self.left_async_widget_mut()?
                .change_to(move |_, _| HError::stale()?)
                .log();
        }

        Ok(())
    }

    pub fn left_widget_goto(&mut self, dir: &File) -> HResult<()> {
        // Check if we're in the correct directory already and return
        // if we are
        let left_dir = &self.left_widget()?.content.directory;
        if self.left_widget().is_ok() && left_dir == dir {
            return Ok(());
        }

        let cache = self.fs_cache.clone();
        let file_source = FileSource::Path(dir.clone());
        let left_async_widget = self.left_async_widget_mut()?;
        left_async_widget
            .change_to(move |stale, core| {
                let view = ListView::builder(core, file_source)
                    .with_cache(cache)
                    .with_stale(stale.clone())
                    .build()?;

                Ok(view)
            })
            .log();

        Ok(())
    }

    pub fn go_back(&mut self) -> HResult<()> {
        if let Ok(new_cwd) = self.cwd.parent_as_file() {
            let previewer_selection = self.selected_file().ok();
            let main_selection = self.cwd.clone();
            let preview_files = self.take_main_files();

            self.prev_cwd = Some(self.cwd.clone());
            self.cwd = new_cwd.clone();

            let cache = self.fs_cache.clone();

            let files = self.take_left_files();
            let file_source = match files {
                Ok(files) => FileSource::Files(files),
                Err(_) => FileSource::Path(new_cwd.clone()),
            };

            self.main_async_widget_mut()?
                .change_to(move |stale, core| {
                    ListView::builder(core, file_source)
                        .select(main_selection)
                        .with_cache(cache)
                        .with_stale(stale.clone())
                        .build()
                })
                .log();

            if let Ok(left_dir) = new_cwd.parent_as_file() {
                let file_source = FileSource::Path(left_dir);
                let cache = self.fs_cache.clone();
                self.left_async_widget_mut()?
                    .change_to(move |stale, core| {
                        ListView::builder(core, file_source)
                            .with_cache(cache)
                            .with_stale(stale.clone())
                            .build()
                    })
                    .log();
            } else {
                // Just place a dummy in the left column
                self.left_async_widget_mut()?
                    .change_to(move |_, core| {
                        let files = Files::default();
                        let source = FileSource::Files(files);
                        ListView::builder(core, source).build()
                    })
                    .log();

                self.left_async_widget_mut()?
                    .widget
                    .on_ready(move |_, stale| {
                        // To stop from drawing empty placeholder
                        stale.set_stale()?;
                        Ok(())
                    })
                    .log()
            }

            if let Ok(preview_files) = preview_files {
                self.preview_widget_mut()
                    .map(|preview| preview.put_preview_files(preview_files, previewer_selection))
                    .ok();
            }
        }

        self.columns.resize_children().log();
        self.refresh()
    }

    pub fn goto_prev_cwd(&mut self) -> HResult<()> {
        let prev_cwd = self.prev_cwd.take().ok_or_else(|| HError::NoneError)?;
        self.main_widget_goto(&prev_cwd)?;
        Ok(())
    }

    pub fn go_home(&mut self) -> HResult<()> {
        let home = crate::paths::home_path().unwrap_or(PathBuf::from("~/"));
        let home = File::new_from_path(&home)?;
        self.main_widget_goto(&home)
    }

    fn get_boomark(&mut self) -> HResult<String> {
        let cwd = &match self.prev_cwd.as_ref() {
            Some(cwd) => cwd,
            None => &self.cwd,
        }
        .path
        .to_string_lossy()
        .to_string();

        self.bookmarks
            .lock()?
            .set_coordinates(&self.core.coordinates)
            .log();

        loop {
            let bookmark = self.bookmarks.lock()?.pick(cwd.to_string());

            if let Err(HError::TerminalResizedError) = bookmark {
                self.core.screen.clear().log();
                self.resize().log();
                self.refresh().log();
                self.draw().log();
                continue;
            }

            if let Err(HError::WidgetResizedError) = bookmark {
                let coords = &self.core.coordinates;
                self.bookmarks.lock()?.set_coordinates(&coords).log();
                self.core.screen.clear().log();
                self.refresh().log();
                self.draw().log();
                continue;
            }

            if let Err(HError::RefreshParent) = bookmark {
                self.refresh().log();
                self.draw().log();
                continue;
            }

            return bookmark;
        }
    }

    pub fn goto_bookmark(&mut self) -> HResult<()> {
        let path = self.get_boomark()?;
        let path = File::new_from_path(&PathBuf::from(path))?;
        self.main_widget_goto(&path)?;
        Ok(())
    }

    pub fn add_bookmark(&mut self) -> HResult<()> {
        let cwd = self.cwd.path.to_string_lossy().to_string();
        let coords = &self.core.coordinates;
        self.bookmarks.lock()?.set_coordinates(&coords).log();
        self.bookmarks.lock()?.add(&cwd)?;
        Ok(())
    }

    pub fn set_title(&self) -> HResult<()> {
        let path = self.cwd.short_string();

        self.core.screen()?.set_title(&path)?;
        Ok(())
    }

    pub fn update_preview(&mut self) -> HResult<()> {
        if !self.main_async_widget_mut()?.ready() {
            return Ok(());
        }
        if self.main_widget()?.content.len() == 0 {
            self.preview_widget_mut()?.set_stale().log();
            return Ok(());
        }

        let file = self.selected_file()?;

        // Don't even call previewer on empty files to save CPU cycles
        match (file.is_dir(), file.calculate_size()) {
            (false, Ok((size, unit))) => {
                if size == 0 && unit == "" {
                    self.preview_widget_mut()?.set_stale().log();
                    return Ok(());
                }
            }
            _ => {}
        }

        let preview = self.preview_widget_mut()?;
        preview.set_file(&file).log();
        Ok(())
    }

    pub fn set_left_selection(&mut self) -> HResult<()> {
        if self.cwd.parent().is_none() {
            return Ok(());
        }
        if !self.left_async_widget_mut()?.ready() {
            return Ok(());
        }

        let selection = self.cwd()?.clone();

        // Saves doing iteration to find file's position
        if let Some(ref current_selection) = self.left_widget()?.current_item {
            if current_selection.name == selection.name {
                return Ok(());
            }
        }

        self.left_widget_mut()?.select_file(&selection);

        let selected_file = self.left_widget()?.selected_file();
        self.cwd
            .parent_as_file()
            .map(|dir| {
                self.fs_cache
                    .set_selection(dir.clone(), selected_file.clone())
            })
            .log();

        Ok(())
    }

    pub fn take_main_files(&mut self) -> HResult<Files> {
        let w = self.main_widget_mut()?;
        let files = std::mem::take(&mut w.content);
        w.content.len = 0;
        Ok(files)
    }

    pub fn take_left_files(&mut self) -> HResult<Files> {
        let w = self.left_widget_mut()?;
        let files = std::mem::take(&mut w.content);
        w.content.len = 0;
        Ok(files)
    }

    pub fn get_files(&self) -> HResult<&Files> {
        Ok(&self.main_widget()?.content)
    }

    pub fn get_left_files(&self) -> HResult<&Files> {
        Ok(&self.left_widget()?.content)
    }

    pub fn save_selected_file(&self) -> HResult<()> {
        self.selected_file()
            .map(|f| self.fs_cache.set_selection(self.cwd.clone(), f))?
    }

    pub fn save_tab_settings(&mut self) -> HResult<()> {
        if !self.main_async_widget_mut()?.ready() {
            return Ok(());
        }

        if self.main_widget()?.content.len() > 0 {
            let files = self.get_files()?;
            let selected_file = self.selected_file().ok();
            self.fs_cache.save_settings(files, selected_file).log();
        }

        Ok(())
    }

    pub fn cwd(&self) -> HResult<&File> {
        Ok(&self.cwd)
    }

    pub fn set_cwd(&mut self) -> HResult<()> {
        let cwd = self.cwd()?;
        std::env::set_current_dir(&cwd.path)?;
        Ok(())
    }

    pub fn left_dir(&self) -> HResult<&File> {
        let widget = self.left_widget()?;
        let dir = &widget.content.directory;
        Ok(dir)
    }

    pub fn selected_file(&self) -> HResult<File> {
        let widget = self.main_widget()?;
        let file = widget.selected_file().clone();
        Ok(file)
    }

    pub fn selected_files(&self) -> HResult<Vec<File>> {
        let widget = self.main_widget()?;
        let files = widget
            .content
            .get_selected()
            .into_iter()
            .map(|f| f.clone())
            .collect();
        Ok(files)
    }

    pub fn main_async_widget_mut(&mut self) -> HResult<&mut AsyncWidget<ListView<Files>>> {
        let widget = self
            .columns
            .active_widget_mut()
            .ok_or_else(|| HError::NoneError)?;

        let widget = match widget {
            FileBrowserWidgets::FileList(filelist) => filelist,
            _ => HError::wrong_widget("previewer", "filelist")?,
        };
        Ok(widget)
    }

    pub fn main_widget(&self) -> HResult<&ListView<Files>> {
        let widget = self
            .columns
            .active_widget()
            .ok_or_else(|| HError::NoneError)?;

        let widget = match widget {
            FileBrowserWidgets::FileList(filelist) => filelist.widget(),
            _ => HError::wrong_widget("previewer", "filelist")?,
        };
        widget
    }

    pub fn main_widget_mut(&mut self) -> HResult<&mut ListView<Files>> {
        let widget = self
            .columns
            .active_widget_mut()
            .ok_or_else(|| HError::NoneError)?;

        let widget = match widget {
            FileBrowserWidgets::FileList(filelist) => filelist.widget_mut(),
            _ => HError::wrong_widget("previewer", "filelist")?,
        };
        widget
    }

    pub fn left_async_widget_mut(&mut self) -> HResult<&mut AsyncWidget<ListView<Files>>> {
        let widget = match self
            .columns
            .widgets
            .get_mut(0)
            .ok_or_else(|| HError::NoneError)?
        {
            FileBrowserWidgets::FileList(filelist) => filelist,
            _ => {
                return HError::wrong_widget("previewer", "filelist");
            }
        };
        Ok(widget)
    }

    pub fn left_widget(&self) -> HResult<&ListView<Files>> {
        let widget = match self
            .columns
            .widgets
            .get(0)
            .ok_or_else(|| HError::NoneError)?
        {
            FileBrowserWidgets::FileList(filelist) => filelist.widget(),
            _ => {
                return HError::wrong_widget("previewer", "filelist");
            }
        };
        widget
    }

    pub fn left_widget_mut(&mut self) -> HResult<&mut ListView<Files>> {
        let widget = match self
            .columns
            .widgets
            .get_mut(0)
            .ok_or_else(|| HError::NoneError)?
        {
            FileBrowserWidgets::FileList(filelist) => filelist.widget_mut(),
            _ => {
                return HError::wrong_widget("previewer", "filelist");
            }
        };
        widget
    }

    pub fn preview_widget(&self) -> HResult<&Previewer> {
        match self
            .columns
            .widgets
            .get(2)
            .ok_or_else(|| HError::NoneError)?
        {
            FileBrowserWidgets::Previewer(previewer) => Ok(previewer),
            _ => {
                return HError::wrong_widget("filelist", "previewer");
            }
        }
    }

    pub fn preview_widget_mut(&mut self) -> HResult<&mut Previewer> {
        match self
            .columns
            .widgets
            .get_mut(2)
            .ok_or_else(|| HError::NoneError)?
        {
            FileBrowserWidgets::Previewer(previewer) => Ok(previewer),
            _ => {
                return HError::wrong_widget("filelist", "previewer");
            }
        }
    }

    fn cancel_preview_animation(&mut self) {
        self.preview_widget_mut()
            .map(|preview| preview.cancel_animation())
            .log();
    }

    fn activate_main_widget(&mut self) {
        const MAIN_INDEX: usize = 1;
        self.columns.set_active(MAIN_INDEX).log();
    }

    fn activate_preview_widget(&mut self) {
        const PREVIEW_INDEX: usize = 2;
        self.columns.set_active(PREVIEW_INDEX).log();
    }

    pub fn toggle_colums(&mut self) {
        self.cancel_preview_animation();
        self.activate_main_widget();
        self.columns.toggle_zoom().log();
    }

    pub fn zoom_preview(&mut self) {
        self.cancel_preview_animation();
        self.activate_preview_widget();
        self.preview_widget_mut()
            .map(|preview| {
                preview.reload_text();
            })
            .log();

        self.columns.toggle_zoom().log();
    }

    pub fn quit_with_dir(&self) -> HResult<()> {
        let cwd = self.cwd()?.clone().path;
        let selected_file = self.selected_file()?;
        let selected_file = selected_file.path.to_string_lossy();
        let selected_files = self.selected_files()?;

        let selected_files = selected_files
            .iter()
            .map(|f| format!("\"{}\" ", &f.path.to_string_lossy()))
            .collect::<String>();

        let mut filepath = dirs_2::home_dir().ok_or_else(|| HError::NoneError)?;
        filepath.push(".hunter_cwd");

        let output = format!(
            "HUNTER_CWD=\"{}\"\nF=\"{}\"\nMF=({})\n",
            cwd.to_str().ok_or_else(|| HError::NoneError)?,
            selected_file,
            selected_files
        );

        let mut file = std::fs::File::create(filepath)?;
        file.write(output.as_bytes())?;
        HError::quit()
    }

    pub fn turbo_cd(&mut self) -> HResult<()> {
        use crate::minibuffer::MiniBufferEvent::*;

        // Return and reset on cancel
        let orig_dir = self.cwd()?.clone();
        let orig_dir_selected_file = self.selected_file()?;
        let mut orig_dir_filter = self.main_widget()?.content.get_filter();

        // For current dir
        let mut selected_file = Some(orig_dir_selected_file.clone());
        let mut filter = Some(orig_dir_filter.clone());

        // Helper function to restore any previous filter/selection
        let dir_restore =
            |s: &mut FileBrowser, filter: Option<Option<String>>, file: Option<File>| {
                s.main_widget_mut()
                    .map(|mw| {
                        filter.map(|f| mw.set_filter(f));
                        file.map(|f| mw.select_file(&f));
                    })
                    .log();
            };

        loop {
            let input = self.core.minibuffer_continuous("nav");
            // dbg!(&input);
            // self.refresh().log();
            // self.draw().log();

            match input {
                // While minibuffer runs it steals all events, thus explicit refresh/redraw
                Err(HError::RefreshParent) => {
                    self.refresh().log();
                    self.draw().log();
                    continue;
                }
                Err(HError::MiniBufferEvent(event)) => {
                    match event {
                        // Done here, restore filter, but leave selection as is
                        Done(_) | Empty => {
                            dir_restore(self, filter.take(), None);
                            self.core.minibuffer_clear().log();
                            break;
                        }
                        NewInput(input) => {
                            // Don't filter anything until a letter appears
                            if input.as_str() == "." || input.as_str() == ".." {
                                continue;
                            }

                            if input.ends_with('/') {
                                match input.as_str() {
                                    "../" => {
                                        dir_restore(self, filter.take(), selected_file.take());
                                        self.go_back().log();
                                        self.core.minibuffer_clear().log();
                                    }
                                    _ => {
                                        let sel = self.selected_file()?;

                                        if sel.is_dir() {
                                            dir_restore(self, filter.take(), selected_file.take());
                                            self.main_widget_goto(&sel)?;
                                            self.core.minibuffer_clear().log();
                                        }
                                    }
                                }
                                continue;
                            }

                            // Save current filter, if existing, before overwriting it
                            // Type is Option<Option<_>>, because filter itself is Option<_>
                            if filter.is_none() {
                                let dir_filter = self.main_widget()?.content.get_filter();
                                filter = Some(dir_filter);
                            }

                            // To restore on leave/cancel
                            if selected_file.is_none() {
                                selected_file = Some(self.selected_file()?);
                            }

                            self.main_widget_mut()?.set_filter(Some(input));
                        }
                        // Restore original directory and filter/selection
                        Cancelled => {
                            self.main_widget_goto(&orig_dir)?;
                            // Special case, because all others fail if directory isn't ready anyway
                            self.main_async_widget_mut()?
                                .widget
                                .on_ready(move |mw, _| {
                                    let mw = mw?;
                                    mw.set_filter(orig_dir_filter.take());
                                    mw.select_file(&orig_dir_selected_file);
                                    Ok(())
                                })?;
                            break;
                        }
                        CycleNext => {
                            // Because of filtering the selected file isn't just at n+1
                            let oldpos = self.main_widget()?.get_selection();

                            let mw = self.main_widget_mut()?;
                            mw.move_down();
                            mw.update_selected_file(oldpos);

                            // Refresh preview and draw header, too
                            self.refresh().log();
                            self.draw().log();

                            // Explicitly selected
                            selected_file = Some(self.selected_file()?);
                        }
                        CyclePrev => {
                            // Because of filtering the selected file isn't just at n-1
                            let oldpos = self.main_widget()?.get_selection();

                            let mw = self.main_widget_mut()?;
                            mw.move_up();
                            mw.update_selected_file(oldpos);

                            // Refresh preview and draw header, too
                            self.refresh().log();
                            self.draw().log();

                            // Explicitly selected
                            selected_file = Some(self.selected_file()?);
                        }
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }

    fn external_select(&mut self) -> HResult<()> {
        let shell = std::env::var("SHELL").unwrap_or("bash".into());
        let cmd = self.core.config.read()?.get()?.select_cmd.clone();

        self.core.get_sender().send(Events::InputEnabled(false))?;
        self.core.screen.suspend().log();
        self.preview_widget()
            .map(|preview| preview.cancel_animation())
            .log();

        let cmd_result = std::process::Command::new(shell)
            .arg("-c")
            .arg(&cmd)
            .stdin(std::process::Stdio::inherit())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .output();

        self.core.screen.activate().log();
        self.core.clear().log();
        self.core.get_sender().send(Events::InputEnabled(true))?;

        match cmd_result {
            Ok(cmd_result) => {
                if cmd_result.status.success() {
                    let cwd = &self.cwd.path;

                    let paths = OsString::from_vec(cmd_result.stdout)
                        .split_lines()
                        .iter()
                        .map(|output| {
                            let path = PathBuf::from(output);
                            if path.is_absolute() {
                                path
                            } else {
                                cwd.join(path)
                            }
                        })
                        .collect::<Vec<PathBuf>>();

                    if paths.len() == 1 {
                        let path = &paths[0];
                        if path.exists() {
                            if path.is_dir() {
                                let dir = File::new_from_path(&path)?;

                                self.main_widget_goto(&dir).log();
                            } else if path.is_file() {
                                let file = File::new_from_path(&path)?;
                                let dir = file.parent_as_file()?;

                                self.main_widget_goto(&dir).log();

                                self.main_async_widget_mut()?.widget.on_ready(move |w, _| {
                                    w?.select_file(&file);
                                    Ok(())
                                })?;
                            }
                        } else {
                            let msg = format!("Can't access path: {}!", path.to_string_lossy());
                            self.core.show_status(&msg).log();
                        }
                    } else {
                        let mut last_file = None;
                        for file_path in paths {
                            if !file_path.exists() {
                                let msg = format!("Can't find: {}", file_path.to_string_lossy());
                                self.core.show_status(&msg).log();
                                continue;
                            }

                            let dir_path = file_path.parent().ok_or_else(|| HError::NoneError)?;
                            if self.cwd.path != dir_path {
                                let file_dir = File::new_from_path(&dir_path);

                                self.main_widget_goto_wait(&file_dir?).log();
                            }

                            self.main_widget_mut()?
                                .content
                                .find_file_with_path(&file_path)
                                .map(|file| {
                                    file.toggle_selection();
                                    last_file = Some(file.clone());
                                });
                        }

                        self.main_widget_mut()
                            .map(|w| {
                                last_file.map(|f| w.select_file(&f));
                                w.content.set_dirty();
                            })
                            .log();
                    }
                } else {
                    self.core.show_status("External program failed!").log();
                }
            }
            Err(_) => self.core.show_status("Can't run external program!").log(),
        }

        Ok(())
    }

    fn external_cd(&mut self) -> HResult<()> {
        let shell = std::env::var("SHELL").unwrap_or("bash".into());
        let cmd = self.core.config.read()?.get()?.cd_cmd.clone();

        self.core.get_sender().send(Events::InputEnabled(false))?;
        self.core.screen.suspend().log();
        self.preview_widget()
            .map(|preview| preview.cancel_animation())
            .log();

        let cmd_result = std::process::Command::new(shell)
            .arg("-c")
            .arg(cmd)
            .stdin(std::process::Stdio::inherit())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .output();

        self.core.screen.activate().log();
        self.core.clear().log();
        self.core.get_sender().send(Events::InputEnabled(true))?;

        match cmd_result {
            Ok(cmd_result) => {
                if cmd_result.status.success() {
                    let cwd = &self.cwd.path;

                    let path_string = OsString::from_vec(cmd_result.stdout);
                    let path_string = path_string.trim_end("\n");
                    let path = PathBuf::from(path_string);
                    let path = if path.is_absolute() {
                        path
                    } else {
                        cwd.join(path)
                    };

                    if path.exists() {
                        if path.is_dir() {
                            let dir = File::new_from_path(&path)?;
                            self.main_widget_goto(&dir).log();
                        } else {
                            let msg = format!("Can't access path: {}!", path.to_string_lossy());
                            self.core.show_status(&msg).log();
                        }
                    } else {
                        self.core.show_status("External program failed!").log();
                    }
                }
            }
            Err(_) => self.core.show_status("Can't run external program!").log(),
        }

        Ok(())
    }

    fn exec_cmd(&mut self, tab_dirs: Vec<File>, tab_files: Vec<Vec<File>>) -> HResult<()> {
        let cwd = self.cwd()?.clone();
        let selected_file = self.selected_file().ok();
        let selected_files = self.selected_files().ok();

        let cmd = self.core.minibuffer("exec")?.to_string();

        let cwd_files = selected_files.map(|selected_files| {
            if selected_files.len() == 0 {
                if selected_file.is_some() {
                    vec![selected_file.unwrap()]
                } else {
                    selected_files
                }
            } else {
                selected_files
            }
        });

        let cmd = crate::proclist::Cmd {
            cmd: OsString::from(cmd),
            short_cmd: None,
            args: None,
            vars: None,
            cwd: cwd,
            cwd_files: cwd_files,
            tab_files: Some(tab_files),
            tab_paths: Some(tab_dirs),
        };

        self.proc_view.lock()?.run_proc_subshell(cmd)?;

        Ok(())
    }

    pub fn run_subshell(&mut self) -> HResult<()> {
        self.core.get_sender().send(Events::InputEnabled(false))?;

        self.preview_widget()
            .map(|preview| preview.cancel_animation())
            .log();
        self.core.screen.suspend().log();

        let shell = std::env::var("SHELL").unwrap_or("bash".into());
        let status = std::process::Command::new(&shell).status();

        self.core.screen.activate().log();

        self.core.get_sender().send(Events::InputEnabled(true))?;

        match status {
            Ok(status) => self
                .core
                .show_status(&format!("\"{}\" exited with {}", shell, status))
                .log(),
            Err(err) => self
                .core
                .show_status(&format!("Can't run this \"{}\": {}", shell, err))
                .log(),
        }

        Ok(())
    }

    pub fn show_procview(&mut self) -> HResult<()> {
        self.preview_widget()
            .map(|preview| preview.cancel_animation())
            .log();
        let procview = self.proc_view.clone();
        loop {
            match procview.lock()?.popup() {
                // Ignore refresh
                Err(HError::RefreshParent) => continue,
                Err(HError::TerminalResizedError) | Err(HError::WidgetResizedError) => {
                    self.resize().log()
                }
                _ => break,
            }
        }
        Ok(())
    }

    pub fn show_log(&mut self) -> HResult<()> {
        self.preview_widget()
            .map(|preview| preview.cancel_animation())
            .log();
        loop {
            let res = self.log_view.lock()?.popup();

            if let Err(HError::RefreshParent) = res {
                continue;
            }

            if let Err(HError::TerminalResizedError) = res {
                self.resize().log();
                continue;
            }

            break;
        }

        Ok(())
    }

    pub fn quick_action(&self) -> HResult<()> {
        let files = self.selected_files()?;
        let files = if files.len() > 0 {
            files
        } else {
            vec![self.selected_file()?.clone()]
        };

        let sender = self.core.get_sender();
        let core = self.preview_widget()?.get_core()?.clone();
        let proc_view = self.proc_view.clone();

        crate::quick_actions::open(files, sender, core, proc_view)?;
        Ok(())
    }

    pub fn get_footer(&self) -> HResult<String> {
        let xsize = self.get_coordinates()?.xsize();
        let ypos = self.get_coordinates()?.position().y();
        let file = self.selected_file()?;

        let permissions = file.pretty_print_permissions().unwrap_or("NOPERMS".into());
        let user = file.pretty_user().unwrap_or("NOUSER".into());
        let group = file.pretty_group().unwrap_or("NOGROUP".into());
        let mtime = file.pretty_mtime().unwrap_or("NOMTIME".into());
        let target = if let Some(target) = &file.target {
            "--> ".to_string() + &target.short_string()
        } else {
            "".to_string()
        };

        let main_widget = self.main_widget()?;
        let selection = main_widget.get_selection() + 1;
        let file_count = main_widget.content.len();
        let file_count = format!("{}", file_count);
        let digits = file_count.len();
        let file_count = format!(
            "{:digits$}/{:digits$}",
            selection,
            file_count,
            digits = digits
        );
        let count_xpos = xsize - file_count.len() as u16;
        let count_ypos = ypos + self.get_coordinates()?.ysize();

        let fs = self.fs_stat.read()?.find_fs(&file.path)?.clone();

        let dev = fs.get_dev().unwrap_or(String::from(""));
        let free_space = fs.get_free();
        let total_space = fs.get_total();
        let space = format!("{}{} / {}", dev, free_space, total_space);

        let space_xpos = count_xpos - space.len() as u16 - 5; // - 3;

        let status = format!(
            "{} {}:{} {}{} {}{}",
            permissions,
            user,
            group,
            crate::term::header_color(),
            mtime,
            crate::term::color_yellow(),
            target
        );
        let status = crate::term::sized_string_u(&status, (xsize - 1) as usize);

        let status = format!(
            "{}{}{}{}{}{} | {}",
            status,
            crate::term::header_color(),
            crate::term::goto_xy(space_xpos, count_ypos),
            crate::term::color_orange(),
            space,
            crate::term::header_color(),
            file_count
        );

        Ok(status)
    }
}

impl Widget for FileBrowser {
    fn get_core(&self) -> HResult<&WidgetCore> {
        Ok(&self.core)
    }
    fn get_core_mut(&mut self) -> HResult<&mut WidgetCore> {
        Ok(&mut self.core)
    }

    fn set_coordinates(&mut self, coordinates: &Coordinates) -> HResult<()> {
        self.core.coordinates = coordinates.clone();
        self.columns.set_coordinates(&coordinates).log();
        self.proc_view.lock()?.set_coordinates(&coordinates).log();
        self.log_view.lock()?.set_coordinates(&coordinates).log();
        self.bookmarks.lock()?.set_coordinates(&coordinates).log();
        Ok(())
    }

    fn render_header(&self) -> HResult<String> {
        let xsize = self.get_coordinates()?.xsize();
        let file = self.selected_file()?;
        let name = &file.name;

        let fcolor = file.get_color();

        let color = if file.is_dir() {
            crate::term::highlight_color()
        } else {
            match fcolor {
                Some(color) => color,
                None => crate::term::normal_color(),
            }
        };

        let path = self.cwd.short_string();

        let mut path = path;
        if &path == "" {
            path.clear();
        }
        if &path == "~/" {
            path.pop();
        }
        if &path == "/" {
            path.pop();
        }

        let pretty_path = format!("{}/{}{}", path, &color, name);
        let sized_path = crate::term::sized_string(&pretty_path, xsize);
        Ok(sized_path.to_string())
    }

    fn render_footer(&self) -> HResult<String> {
        let xsize = term::xsize_u();
        let mut status = self.get_core()?.status_bar_content.lock()?;
        let status = status.as_mut().take();
        let active = self.columns.active.unwrap_or(1);

        match (status, active) {
            (Some(status), _) => Ok(term::sized_string_u(&status, xsize)),
            (_, 2) => self.preview_widget()?.render_footer(),
            _ => self.get_footer(),
        }
    }

    fn refresh(&mut self) -> HResult<()> {
        self.set_title().log();
        self.columns.refresh().log();
        self.set_left_selection().log();
        self.set_cwd().log();
        if !self.columns.zoom_active {
            self.update_preview().log();
        }
        self.columns.refresh().log();
        Ok(())
    }

    fn get_drawlist(&self) -> HResult<String> {
        self.columns.get_drawlist()
    }

    fn on_key(&mut self, key: Key) -> HResult<()> {
        // Special handling for preview zoom
        let binds = self.search_in();
        let action = binds.get(key);

        match (action, self.columns.active) {
            (Some(FileBrowserAction::ZoomPreview), Some(2)) => {
                self.toggle_colums();
                return Ok(());
            }
            (Some(FileBrowserAction::ZoomPreview), Some(1)) => {
                self.zoom_preview();
                return Ok(());
            }
            (_, Some(2)) => {
                self.columns
                    .active_widget_mut()
                    .ok_or_else(|| HError::NoneError)?
                    .on_key(key)?;
                return Ok(());
            }
            _ => {}
        }

        match self.do_key(key) {
            Err(HError::WidgetUndefinedKeyError { .. }) => {
                match self.main_widget_mut()?.on_key(key) {
                    Ok(_) => {
                        self.save_tab_settings()?;
                    }
                    Err(HError::WidgetUndefinedKeyError { .. }) => {
                        self.preview_widget_mut()?.on_key(key)?
                    }
                    e @ _ => e?,
                }
            }
            e @ _ => e?,
        };

        if !self.columns.zoom_active {
            self.update_preview().log();
        }
        Ok(())
    }
}

use crate::keybind::{Acting, Bindings, FileBrowserAction, Movement};

impl Acting for FileBrowser {
    type Action = FileBrowserAction;

    fn search_in(&self) -> Bindings<Self::Action> {
        self.core.config().keybinds.filebrowser
    }

    fn movement(&mut self, movement: &Movement) -> HResult<()> {
        use Movement::*;

        match movement {
            Left => self.go_back(),
            Right => self.enter_dir(),
            _ => {
                let pos = self.main_widget()?.get_selection();
                self.main_widget_mut()?.movement(movement)?;
                if self.main_widget()?.get_selection() != pos {
                    self.preview_widget_mut()?.set_stale().log();
                    self.preview_widget_mut()?.cancel_animation().log();
                }
                self.save_selected_file()?;
                Ok(())
            }
        }
    }

    fn do_action(&mut self, action: &Self::Action) -> HResult<()> {
        use FileBrowserAction::*;
        match action {
            Quit => HError::quit()?,
            QuitWithDir => self.quit_with_dir()?,
            LeftColumnDown => self.move_down_left_widget()?,
            LeftColumnUp => self.move_up_left_widget()?,
            GotoHome => self.go_home()?,
            TurboCd => self.turbo_cd()?,
            SelectExternal => self.external_select()?,
            EnterDirExternal => self.external_cd()?,
            RunInBackground => self.open_bg()?,
            GotoPrevCwd => self.goto_prev_cwd()?,
            ShowBookmarks => self.goto_bookmark()?,
            AddBookmark => self.add_bookmark()?,
            ShowProcesses => self.show_procview()?,
            ShowLog => self.show_log()?,
            ShowQuickActions => self.quick_action()?,
            RunSubshell => self.run_subshell()?,
            ToggleColumns => self.toggle_colums(),
            ZoomPreview => self.zoom_preview(),
            // Tab implementation needs to call exec_cmd because ALL files are needed
            ExecCmd => Err(HError::FileBrowserNeedTabFiles)?,
        }
        Ok(())
    }
}

impl PartialEq for FileBrowser {
    fn eq(&self, other: &FileBrowser) -> bool {
        if self.columns == other.columns && self.cwd == other.cwd {
            true
        } else {
            false
        }
    }
}
