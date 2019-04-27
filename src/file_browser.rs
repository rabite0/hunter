use termion::event::Key;
use pathbuftools::PathBufTools;
use osstrtools::OsStrTools;

use std::io::Write;
use std::sync::{Arc, Mutex, RwLock};
use std::path::PathBuf;
use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::collections::HashSet;

use crate::files::{File, Files};
use crate::fscache::FsCache;
use crate::listview::ListView;
use crate::hbox::HBox;
use crate::widget::Widget;
use crate::tabview::{TabView, Tabbable};
use crate::preview::{Previewer, AsyncWidget};
use crate::textview::TextView;
use crate::fail::{HResult, HError, ErrorLog};
use crate::widget::{Events, WidgetCore};
use crate::proclist::ProcView;
use crate::bookmarks::BMPopup;
use crate::term;
use crate::term::ScreenExt;
use crate::foldview::LogView;
use crate::coordinates::Coordinates;
use crate::dirty::Dirtyable;
use crate::stats::{FsStat, FsExt};

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
    fs_stat: Arc<RwLock<FsStat>>
}

impl Tabbable for TabView<FileBrowser> {
    fn new_tab(&mut self) -> HResult<()> {
        let cur_tab = self.active_tab_();

        let settings = cur_tab.fs_cache.tab_settings.read()?.clone();
        let cache = cur_tab.fs_cache.new_client(settings).ok();

        let mut tab = FileBrowser::new(&self.active_tab_().core, cache)?;

        let proc_view = cur_tab.proc_view.clone();
        let bookmarks = cur_tab.bookmarks.clone();
        let log_view  = cur_tab.log_view.clone();
        tab.proc_view = proc_view;
        tab.bookmarks = bookmarks;
        tab.log_view  = log_view;
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

    fn goto_tab(&mut self, index: usize) -> HResult<()> {
        self.goto_tab_(index)
    }

    fn get_tab_names(&self) -> Vec<Option<String>> {
        self.widgets.iter().map(|filebrowser| {
            let path = filebrowser.cwd.path();
            let last_dir = path.components().last().unwrap();
            let dir_name = last_dir.as_os_str().to_string_lossy().to_string();
            Some(dir_name)
        }).collect()
    }

    fn active_tab(& self) -> & dyn Widget {
        self.active_tab_()
    }

    fn active_tab_mut(&mut self) -> &mut dyn Widget {
        self.active_tab_mut_()
    }

    fn on_tab_switch(&mut self) -> HResult<()> {
        self.active_tab_mut().refresh()
    }

    fn on_key_sub(&mut self, key: Key) -> HResult<()> {
        match key {
            Key::Char('!') => {
                let tab_dirs = self.widgets.iter().map(|w| w.cwd.clone())
                                                  .collect::<Vec<_>>();
                let selected_files = self
                    .widgets
                    .iter()
                    .map(|w| {
                        w.selected_files().unwrap_or(vec![])
                    }).collect();

                self.widgets[self.active].exec_cmd(tab_dirs, selected_files)
            }
            _ => { self.active_tab_mut().on_key(key) }
        }
    }

    fn on_refresh(&mut self) -> HResult<()> {
        let fs_changes = self.active_tab_()
            .fs_cache
            .fs_changes
            .write()?
            .drain(..)
            .collect::<Vec<_>>();

        for tab in &mut self.widgets {
            for (dir, old_file, new_file) in fs_changes.iter() {
                tab.replace_file(&dir,
                                 old_file.as_ref(),
                                 new_file.as_ref()).log()
            }
        }

        let open_dirs = self.widgets
            .iter()
            .fold(HashSet::new(), |mut dirs, tab| {
                tab.left_dir().map(|dir| dirs.insert(dir.clone())).ok();
                dirs.insert(tab.cwd.clone());
                tab.preview_widget()
                    .map(|preview| preview.get_file().map(|file| {
                        if file.is_dir() {
                            dirs.insert(file.clone());
                        }
                    })).ok();
                dirs
            });

        self.active_tab_mut_().fs_cache.watch_only(open_dirs).log();
        self.active_tab_mut_().fs_stat.write()?.refresh().log();
        Ok(())
    }

    fn on_config_loaded(&mut self) -> HResult<()> {
        // hack: wait a bit for widget readyness...
        let duration = std::time::Duration::from_millis(100);
        std::thread::sleep(duration);

        let show_hidden = self.config().show_hidden();
        for tab in self.widgets.iter_mut() {
            tab.left_widget_mut().map(|w| {
                w.content.show_hidden = show_hidden;
                w.content.dirty_meta.set_dirty();
                w.refresh().log();
            }).ok();

            tab.main_widget_mut().map(|w| {
                w.content.show_hidden = show_hidden;
                w.content.dirty_meta.set_dirty();
                w.content.sort();
                w.refresh().log();
            }).ok();

            tab.preview_widget_mut().map(|w| w.config_loaded()).ok();
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
        columns.set_ratios(vec![20,30,49]);
        let list_coords = columns.calculate_coordinates()?;

        core_l.coordinates = list_coords[0].clone();
        core_m.coordinates = list_coords[1].clone();
        core_p.coordinates = list_coords[2].clone();

        let main_path = cwd.ancestors()
                           .take(1)
                           .map(|path| {
                               std::path::PathBuf::from(path)
                           }).last()?;
        let left_path = main_path.parent().map(|p| p.to_path_buf());

        let cache = fs_cache.clone();
        let main_widget = AsyncWidget::new(&core, Box::new(move |_| {
            let name = if main_path.parent().is_none() {
                "root".to_string()
            } else {
                main_path.file_name()?
                    .to_string_lossy()
                    .to_string()
            };
            let main_dir = File::new(&name,
                                     main_path.clone(),
                                     None);
            let mut files = cache.get_files_sync(&main_dir)?;
            let selection = cache.get_selection(&main_dir).ok();

            files.meta_all();

            let mut list = ListView::new(&core_m.clone(),
                                         files);
            if let Some(file) = selection {
                list.select_file(&file);
            }

            list.refresh().log();

            Ok(list)
        }));

        let cache = fs_cache.clone();
        if let Some(left_path) = left_path {
            let left_widget = AsyncWidget::new(&core, Box::new(move |_| {
                let name = if left_path.parent().is_none() {
                    "root".to_string()
                } else {
                    left_path.file_name()?
                        .to_string_lossy()
                        .to_string()
                };
                let left_dir = File::new(&name,
                                         left_path.clone(),
                                         None);
                let files = cache.get_files_sync(&left_dir)?;
                let selection = cache.get_selection(&left_dir).ok();
                let mut list = ListView::new(&core_l,
                                             files);
                if let Some(file) = selection {
                    list.select_file(&file);
                }

                list.refresh().log();

                Ok(list)
            }));
            let left_widget = FileBrowserWidgets::FileList(left_widget);
            columns.push_widget(left_widget);
        } else {
            let left_widget = AsyncWidget::new(&core, Box::new(move |_| {
                let blank = TextView::new_blank(&core_l);
                Ok(blank)
            }));

            let left_widget = FileBrowserWidgets::Blank(left_widget);
            columns.push_widget(left_widget);
        }

        let previewer = Previewer::new(&core_p, fs_cache.clone());

        columns.push_widget(FileBrowserWidgets::FileList(main_widget));
        columns.push_widget(FileBrowserWidgets::Previewer(previewer));
        columns.set_active(1).log();
        columns.refresh().log();


        let cwd = File::new_from_path(&cwd, None).unwrap();

        let proc_view = ProcView::new(&core);
        let bookmarks = BMPopup::new(&core);
        let log_view = LogView::new(&core, vec![]);
        let fs_stat = FsStat::new().unwrap();



        Ok(FileBrowser { columns: columns,
                         cwd: cwd,
                         prev_cwd: None,
                         core: core.clone(),
                         proc_view: Arc::new(Mutex::new(proc_view)),
                         bookmarks: Arc::new(Mutex::new(bookmarks)),
                         log_view: Arc::new(Mutex::new(log_view)),
                         fs_cache: fs_cache,
                         fs_stat: Arc::new(RwLock::new(fs_stat))
        })
    }

    pub fn enter_dir(&mut self) -> HResult<()> {
        let file = self.selected_file()?;

        if file.is_dir() {
            let dir = file;
            match dir.is_readable() {
                Ok(true) => {},
                Ok(false) => {
                    let status =
                        format!("{}Stop right there, cowboy! Check your permisions!",
                                term::color_red());
                    self.show_status(&status).log();
                    return Ok(());
                }
                err @ Err(_) => err.log()
            }

            let previewer_files = self.preview_widget_mut()?.take_files().ok();

            self.columns.remove_widget(0);

            self.prev_cwd = Some(self.cwd.clone());
            self.cwd = dir.clone();

            let core = self.core.clone();
            let cache = self.fs_cache.clone();

            let main_widget = AsyncWidget::new(&core.clone(), Box::new(move |_| {
                let files = match previewer_files {
                    Some(files) => files,
                    None => cache.get_files_sync(&dir)?
                };

                let selection = cache.get_selection(&dir).ok();

                let mut list = ListView::new(&core, files);

                if let Some(file) = selection {
                    list.select_file(&file);
                }

                list.content.meta_all();

                Ok(list)
            }));

            let main_widget = FileBrowserWidgets::FileList(main_widget);
            self.columns.insert_widget(1, main_widget);

        } else {
            self.core.get_sender().send(Events::InputEnabled(false))?;
            self.core.screen.drop_screen();

            let status = std::process::Command::new("rifle")
                .args(file.path.file_name())
                .status();

            self.core.screen.reset_screen().log();
            self.clear().log();
            self.core.screen.cursor_hide().log();

            self.core.get_sender().send(Events::InputEnabled(true))?;

            match status {
                Ok(status) =>
                    self.show_status(&format!("\"{}\" exited with {}",
                                              "rifle", status)).log(),
                Err(err) =>
                    self.show_status(&format!("Can't run this \"{}\": {}",
                                              "rifle", err)).log()
            }
        }
        Ok(())
    }

    pub fn open_bg(&mut self) -> HResult<()> {
        let cwd = self.cwd()?;
        let file = self.selected_file()?;

        let cmd = crate::proclist::Cmd {
            cmd: OsString::from(file.strip_prefix(&cwd)),
            short_cmd: None,
            args: None,
            cwd: cwd.clone(),
            cwd_files: None,
            tab_files: None,
            tab_paths: None
        };

        self.proc_view.lock()?.run_proc_raw(cmd)?;

        Ok(())
    }

    pub fn main_widget_goto_wait(&mut self, dir :&File) -> HResult<()> {
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
        self.cache_files().log();

        let dir = dir.clone();
        let cache = self.fs_cache.clone();

        self.prev_cwd = Some(self.cwd.clone());
        self.cwd = dir.clone();

        let main_async_widget = self.main_async_widget_mut()?;
        main_async_widget.change_to(Box::new(move |stale, core| {
            let (selected_file, files) = cache.get_files(&dir, stale)?;
            let files = files.wait()?;

            let mut list = ListView::new(&core, files);

            list.content.meta_set_fresh().log();
            list.content.meta_all();

            if let Some(file) = selected_file {
                list.select_file(&file);
            }
            Ok(list)
        })).log();

        if let Ok(grand_parent) = self.cwd()?.parent_as_file() {
            self.left_widget_goto(&grand_parent).log();
        } else {
            self.left_async_widget_mut()?.change_to(Box::new(move |_,_| {
                HError::stale()?
            })).log();
        }

        Ok(())
    }

    pub fn left_widget_goto(&mut self, dir: &File) -> HResult<()> {
        let cache = self.fs_cache.clone();
        let dir = dir.clone();

        let left_async_widget = self.left_async_widget_mut()?;
        left_async_widget.change_to(Box::new(move |stale, core| {
            let cached_files = cache.get_files(&dir, stale)?;
            let (_, files) = cached_files;

            let files = files.wait()?;

            let list = ListView::new(&core, files);
            Ok(list)
        }))?;
        Ok(())
    }

    pub fn go_back(&mut self) -> HResult<()> {
        if let Ok(new_cwd) = self.cwd.parent_as_file() {
            let core = self.core.clone();
            let preview_files = self.take_main_files();
            let old_left = self.columns.remove_widget(0);
            self.prev_cwd = Some(self.cwd.clone());
            self.cwd = new_cwd.clone();

            if let Ok(left_dir) = new_cwd.parent_as_file() {
                let cache = self.fs_cache.clone();
                let left_widget = AsyncWidget::new(&core.clone(), Box::new(move |_| {
                    let files = cache.get_files_sync(&left_dir)?;
                    let list = ListView::new(&core, files);
                    Ok(list)
                }));

                let left_widget = FileBrowserWidgets::FileList(left_widget);
                self.columns.prepend_widget(left_widget);
            } else {
                let left_widget = AsyncWidget::new(&core.clone(), Box::new(move |_| {
                    let blank = TextView::new_blank(&core);
                    Ok(blank)
                }));

                let left_widget = FileBrowserWidgets::Blank(left_widget);
                self.columns.prepend_widget(left_widget);
            }
            self.columns.replace_widget(1, old_left);
            self.main_widget_mut()?.content.meta_all();

            if let Ok(preview_files) = preview_files {
                self.preview_widget_mut().map(|preview| {
                    preview.put_preview_files(preview_files)
                }).ok();
            }
        }

        self.columns.resize_children().log();
        self.refresh()
    }

    pub fn goto_prev_cwd(&mut self) -> HResult<()> {
        let prev_cwd = self.prev_cwd.take()?;
        self.main_widget_goto(&prev_cwd)?;
        Ok(())
    }

    fn get_boomark(&mut self) -> HResult<String> {
        let cwd = &match self.prev_cwd.as_ref() {
            Some(cwd) => cwd,
            None => &self.cwd
        }.path.to_string_lossy().to_string();

        self.bookmarks.lock()?.set_coordinates(&self.core.coordinates).log();

        loop {
            let bookmark =  self.bookmarks.lock()?.pick(cwd.to_string());

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
            return bookmark;
        }
    }

    pub fn goto_bookmark(&mut self) -> HResult<()> {
        let path = self.get_boomark()?;
        let path = File::new_from_path(&PathBuf::from(path), None)?;
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

        self.screen()?.set_title(&path)?;
        Ok(())
    }

    pub fn update_preview(&mut self) -> HResult<()> {
        if !self.main_async_widget_mut()?.ready() { return Ok(()) }
        if self.main_widget()?
            .content
            .len() == 0 {
                self.preview_widget_mut()?.set_stale().log();
                return Ok(());
            }
        let file = self.selected_file()?.clone();
        let preview = self.preview_widget_mut()?;
        preview.set_file(&file).log();
        Ok(())
    }

    pub fn set_left_selection(&mut self) -> HResult<()> {
        if self.cwd.parent().is_none() { return Ok(()) }
        if !self.left_async_widget_mut()?.ready() { return Ok(()) }

        let selection = self.cwd()?.clone();

        self.left_widget_mut()?.select_file(&selection);

        Ok(())
    }

    pub fn take_main_files(&mut self) -> HResult<Files> {
        let core = self.core.clone();
        let blank = AsyncWidget::new(&core.clone(), Box::new(move |_| {
            HError::no_files()
        }));
        let blank = FileBrowserWidgets::Blank(blank);

        let old_widget = self.columns.replace_widget(1, blank);

        if let FileBrowserWidgets::FileList(main_widget) = old_widget {
            let files = main_widget.take_widget()?.content;
            return Ok(files)
        }
        HError::no_files()
    }

    pub fn take_left_files(&mut self) -> HResult<Files> {
        let core = self.core.clone();
        let blank = AsyncWidget::new(&core.clone(), Box::new(move |_| {
            HError::no_files()
        }));
        let blank = FileBrowserWidgets::FileList(blank);

        let old_widget = self.columns.replace_widget(0, blank);

        if let FileBrowserWidgets::FileList(left_widget) = old_widget {
            let files = left_widget.take_widget()?.content;
            return Ok(files)
        }
        HError::no_files()
    }

    pub fn get_files(&self) -> HResult<&Files> {
        Ok(&self.main_widget()?.content)
    }

    pub fn get_left_files(&self) -> HResult<&Files> {
        Ok(&self.left_widget()?.content)
    }

    pub fn cache_files(&mut self) -> HResult<()> {
        let files = self.get_files()?;
        let selected_file = self.selected_file().ok();
        self.fs_cache.put_files(files, selected_file).log();
        self.main_widget_mut()?.content.meta_updated = false;


        // if self.cwd.parent().is_some() {
        //     let left_selection = self.left_widget()?.clone_selected_file();
        //     let left_files = self.get_left_files()?;
        //     self.fs_cache.put_files(left_files, Some(left_selection)).log();
        //     self.left_widget_mut()?.content.meta_updated = false;
        // }

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

    fn replace_file(&mut self,
                    dir: &File,
                    old: Option<&File>,
                    new: Option<&File>) -> HResult<()> {
        if &self.cwd == dir {
            self.main_widget_mut()?.content.replace_file(old, new.cloned()).log();
        }

        self.preview_widget_mut()?.replace_file(dir, old, new).ok();

        if &self.left_dir()? == &dir {
            self.left_widget_mut()?.content.replace_file(old, new.cloned()).log();
        }
        Ok(())
    }

    pub fn selected_file(&self) -> HResult<File> {
        let widget = self.main_widget()?;
        let file = widget.selected_file().clone();
        Ok(file)
    }

    pub fn selected_files(&self) -> HResult<Vec<File>> {
        let widget = self.main_widget()?;
        let files = widget.content.get_selected().into_iter().map(|f| {
            f.clone()
        }).collect();
        Ok(files)
    }

    pub fn main_async_widget_mut(&mut self) -> HResult<&mut AsyncWidget<ListView<Files>>> {
        let widget = self.columns.active_widget_mut()?;

        let widget = match widget {
            FileBrowserWidgets::FileList(filelist) => filelist,
            _ => { HError::wrong_widget("previewer", "filelist")? }
        };
        Ok(widget)
    }

    pub fn main_widget(&self) -> HResult<&ListView<Files>> {
        let widget = self.columns.active_widget()?;

        let widget = match widget {
            FileBrowserWidgets::FileList(filelist) => filelist.widget(),
            _ => { HError::wrong_widget("previewer", "filelist")? }
        };
        widget
    }

    pub fn main_widget_mut(&mut self) -> HResult<&mut ListView<Files>> {
        let widget = self.columns.active_widget_mut()?;

        let widget = match widget {
            FileBrowserWidgets::FileList(filelist) => filelist.widget_mut(),
            _ => { HError::wrong_widget("previewer", "filelist")? }
        };
        widget
    }

    pub fn left_async_widget_mut(&mut self) -> HResult<&mut AsyncWidget<ListView<Files>>> {
        let widget = match self.columns.widgets.get_mut(0)? {
            FileBrowserWidgets::FileList(filelist) => filelist,
            _ => { return HError::wrong_widget("previewer", "filelist"); }
        };
        Ok(widget)
    }

    pub fn left_widget(&self) -> HResult<&ListView<Files>> {
        let widget = match self.columns.widgets.get(0)? {
            FileBrowserWidgets::FileList(filelist) => filelist.widget(),
            _ => { return HError::wrong_widget("previewer", "filelist"); }
        };
        widget
    }

    pub fn left_widget_mut(&mut self) -> HResult<&mut ListView<Files>> {
        let widget = match self.columns.widgets.get_mut(0)? {
            FileBrowserWidgets::FileList(filelist) => filelist.widget_mut(),
            _ => { return HError::wrong_widget("previewer", "filelist"); }
        };
        widget
    }

    pub fn preview_widget(&self) -> HResult<&Previewer> {
        match self.columns.widgets.get(2)? {
            FileBrowserWidgets::Previewer(previewer) => Ok(previewer),
            _ => { return HError::wrong_widget("filelist", "previewer"); }
        }
    }

    pub fn preview_widget_mut(&mut self) -> HResult<&mut Previewer> {
        match self.columns.widgets.get_mut(2)? {
            FileBrowserWidgets::Previewer(previewer) => Ok(previewer),
            _ => { return HError::wrong_widget("filelist", "previewer"); }
        }
    }

    pub fn toggle_colums(&mut self) {
        self.preview_widget().map(|preview| preview.cancel_animation()).log();
        self.columns.toggle_zoom().log();
    }

    pub fn quit_with_dir(&self) -> HResult<()> {
        let cwd = self.cwd()?.clone().path;
        let selected_file = self.selected_file()?;
        let selected_file = selected_file.path.to_string_lossy();
        let selected_files = self.selected_files()?;

        let selected_files = selected_files.iter().map(|f| {
            format!("\"{}\" ", &f.path.to_string_lossy())
        }).collect::<String>();

        let mut filepath = dirs_2::home_dir()?;
        filepath.push(".hunter_cwd");

        let output = format!("HUNTER_CWD=\"{}\"\nF=\"{}\"\nMF=({})\n",
                             cwd.to_str()?,
                             selected_file,
                             selected_files);

        let mut file = std::fs::File::create(filepath)?;
        file.write(output.as_bytes())?;
        HError::quit()
    }

    pub fn turbo_cd(&mut self) -> HResult<()> {
        let dir = self.minibuffer("cd")?;

        let path = std::path::PathBuf::from(&dir);
        let dir = File::new_from_path(&path.canonicalize()?, None)?;
        self.main_widget_goto(&dir)?;

        Ok(())
    }

    fn external_select(&mut self) -> HResult<()> {
        let shell = std::env::var("SHELL").unwrap_or("bash".into());
        let cmd = self.core
            .config.read()?
            .get()?
            .select_cmd
            .clone();

        self.core.get_sender().send(Events::InputEnabled(false))?;
        self.core.screen.drop_screen();
        self.preview_widget().map(|preview| preview.cancel_animation()).log();

        let cmd_result = std::process::Command::new(shell)
            .arg("-c")
            .arg(&cmd)
            .stdin(std::process::Stdio::inherit())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .output();

        self.core.screen.reset_screen().log();
        self.clear().log();
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
                                let dir = File::new_from_path(&path, None)?;

                                self.main_widget_goto(&dir).log();
                            } else if path.is_file() {
                                let file = File::new_from_path(&path, None)?;
                                let dir = file.parent_as_file()?;

                                self.main_widget_goto_wait(&dir).log();

                                self.main_widget_mut()?.select_file(&file);
                            }
                        } else {
                            let msg = format!("Can't access path: {}!",
                                              path.to_string_lossy());
                            self.show_status(&msg).log();
                        }
                    } else {
                        let mut last_file = None;
                        for file_path in paths {
                            if !file_path.exists() {
                                let msg = format!("Can't find: {}",
                                                  file_path .to_string_lossy());
                                self.show_status(&msg).log();
                                continue;
                            }

                            let dir_path = file_path.parent()?;
                            if self.cwd.path != dir_path {
                                let file_dir = File::new_from_path(&dir_path, None);

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

                        self.main_widget_mut().map(|w| {
                            last_file.map(|f| w.select_file(&f));
                            w.content.set_dirty();
                        }).log();
                    }
                } else {
                    self.show_status("External program failed!").log();
                }
            }
            Err(_) => self.show_status("Can't run external program!").log()
        }

        Ok(())
    }

    fn external_cd(&mut self) -> HResult<()> {
        let shell = std::env::var("SHELL").unwrap_or("bash".into());
        let cmd = self.core
            .config.read()?
            .get()?
            .cd_cmd
            .clone();

        self.core.get_sender().send(Events::InputEnabled(false))?;
        self.core.screen.drop_screen();
        self.preview_widget().map(|preview| preview.cancel_animation()).log();

        let cmd_result = std::process::Command::new(shell)
            .arg("-c")
            .arg(cmd)
            .stdin(std::process::Stdio::inherit())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .output();

        self.core.screen.reset_screen().log();
        self.clear().log();
        self.core.get_sender().send(Events::InputEnabled(true))?;

        match cmd_result {
            Ok(cmd_result) => {
                if cmd_result.status.success() {
                    let cwd = &self.cwd.path;

                    let path_string = OsString::from_vec(cmd_result.stdout);
                    let path_string = path_string.trim_end_newlines();
                    let path = PathBuf::from(path_string);
                    let path = if path.is_absolute() {
                        path
                    } else {
                        cwd.join(path)
                    };

                    if path.exists() {
                        if path.is_dir() {
                            let dir = File::new_from_path(&path, None)?;
                            self.main_widget_goto(&dir).log();
                        }
                        else {
                            let msg = format!("Can't access path: {}!",
                                              path.to_string_lossy());
                            self.show_status(&msg).log();
                        }

                    } else {
                        self.show_status("External program failed!").log();
                    }
                }
            }
            Err(_) => self.show_status("Can't run external program!").log()
        }

        Ok(())
    }


    fn exec_cmd(&mut self,
                tab_dirs: Vec<File>,
                tab_files: Vec<Vec<File>>) -> HResult<()> {

        let cwd = self.cwd()?.clone();
        let selected_file = self.selected_file().ok();
        let selected_files = self.selected_files().ok();

        let cmd = self.minibuffer("exec")?.trim_start().to_string() + " ";

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
            cwd: cwd,
            cwd_files: cwd_files,
            tab_files: Some(tab_files),
            tab_paths: Some(tab_dirs)
        };

        self.proc_view.lock()?.run_proc_subshell(cmd)?;

        Ok(())
    }

    pub fn run_subshell(&mut self) -> HResult<()> {
        self.core.get_sender().send(Events::InputEnabled(false))?;

        self.preview_widget().map(|preview| preview.cancel_animation()).log();
        self.core.screen.cursor_show().log();
        self.core.screen.drop_screen();

        let shell = std::env::var("SHELL").unwrap_or("bash".into());
        let status = std::process::Command::new(&shell).status();

        self.core.screen.reset_screen().log();


        self.core.get_sender().send(Events::InputEnabled(true))?;

        match status {
            Ok(status) =>
                self.show_status(&format!("\"{}\" exited with {}",
                                          shell, status)).log(),
            Err(err) =>
                self.show_status(&format!("Can't run this \"{}\": {}",
                                          shell, err)).log()
        }



        Ok(())
    }

    pub fn show_procview(&mut self) -> HResult<()> {
        self.preview_widget().map(|preview| preview.cancel_animation()).log();
        self.proc_view.lock()?.popup()?;
        Ok(())
    }

    pub fn show_log(&mut self) -> HResult<()> {
        self.preview_widget().map(|preview| preview.cancel_animation()).log();
        self.log_view.lock()?.popup()?;
        Ok(())
    }

    pub fn get_footer(&self) -> HResult<String> {
        let xsize = self.get_coordinates()?.xsize();
        let ypos = self.get_coordinates()?.position().y();
        let pos = self.main_widget()?.get_selection();
        let file = self.main_widget()?.content.get_files().get(pos).cloned()?;

        let permissions = file.pretty_print_permissions().unwrap_or("NOPERMS".into());
        let user = file.pretty_user().unwrap_or("NOUSER".into());
        let group = file.pretty_group().unwrap_or("NOGROUP".into());
        let mtime = file.pretty_mtime().unwrap_or("NOMTIME".into());
        let target = if let Some(target) = &file.target {
            "--> ".to_string() + &target.short_string()
        } else { "".to_string() };

        let main_widget = self.main_widget()?;
        let selection = main_widget.get_selection();
        let file_count = main_widget.content.len();
        let file_count = format!("{}", file_count);
        let digits = file_count.len();
        let file_count = format!("{:digits$}/{:digits$}",
                                 selection,
                                 file_count,
                                 digits = digits);
        let count_xpos = xsize - file_count.len() as u16;
        let count_ypos = ypos + self.get_coordinates()?.ysize();

        let fs = self.fs_stat.read()?.find_fs(&file.path)?.clone();

        let dev = fs.get_dev();
        let free_space = fs.get_free();
        let total_space = fs.get_total();
        let space = format!("{}: {} / {}",
                            dev,
                            free_space,
                            total_space);

        let space_xpos = count_xpos - space.len() as u16 - 5; // - 3;

        let status = format!("{} {}:{} {}{} {}{}",
                             permissions,
                             user,
                             group,
                             crate::term::header_color(),
                             mtime,
                             crate::term::color_yellow(),
                             target
        );
        let status = crate::term::sized_string_u(&status, (xsize-1) as usize);

        let status = format!("{}{}{}{}{}{} | {}",
                             status,
                             crate::term::header_color(),
                             crate::term::goto_xy(space_xpos, count_ypos),
                             crate::term::color_orange(),
                             space,
                             crate::term::header_color(),
                             file_count);

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

        let color = if file.is_dir() {
            crate::term::highlight_color() }
        else if file.color.is_none() {
            crate::term::normal_color()
        } else {
            crate::term::from_lscolor(file.color.as_ref().unwrap())
        };

        let path = self.cwd.short_string();

        let mut path = path;
        if &path == "" { path.clear(); }
        if &path == "~/" { path.pop(); }
        if &path == "/" { path.pop(); }


        let pretty_path = format!("{}/{}{}", path, &color, name );
        let sized_path = crate::term::sized_string(&pretty_path, xsize);
        Ok(sized_path)
    }
    fn render_footer(&self) -> HResult<String> {
        let xsize = term::xsize_u();
        match self.get_core()?.status_bar_content.lock()?.as_mut().take() {
            Some(status) => Ok(term::sized_string_u(&status, xsize)),
            _ => { self.get_footer() },
        }
    }
    fn refresh(&mut self) -> HResult<()> {
        self.set_title().log();
        self.columns.refresh().log();
        self.set_left_selection().log();
        self.set_cwd().log();
        if !self.columns.zoom_active { self.update_preview().log(); }
        self.columns.refresh().log();
        self.cache_files().log();
        Ok(())
    }

    fn get_drawlist(&self) -> HResult<String> {
        self.columns.get_drawlist()
    }

    fn on_key(&mut self, key: Key) -> HResult<()> {
        match key {
            Key::Alt(' ') => self.external_select()?,
            Key::Alt('/') => self.external_cd()?,
            Key::Char('/') => { self.turbo_cd()?; },
            Key::Char('q') => HError::quit()?,
            Key::Char('Q') => { self.quit_with_dir()?; },
            Key::Right | Key::Char('l') => { self.enter_dir()?; },
            Key::Char('L') => { self.open_bg()?; },
            Key::Left | Key::Char('h') => { self.go_back()?; },
            Key::Char('-') => { self.goto_prev_cwd()?; },
            Key::Char('`') => { self.goto_bookmark()?; },
            Key::Char('m') => { self.add_bookmark()?; },
            Key::Char('w') => { self.show_procview()?; },
            Key::Char('g') => self.show_log()?,
            Key::Char('z') => self.run_subshell()?,
            Key::Char('c') => self.toggle_colums(),
            _ => { self.main_widget_mut()?.on_key(key)?; },
        }
        if !self.columns.zoom_active { self.update_preview().log(); }
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
