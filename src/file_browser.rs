use termion::event::Key;
use notify::{INotifyWatcher, Watcher, DebouncedEvent, RecursiveMode};

use std::io::Write;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Duration;
use std::path::PathBuf;
use std::collections::HashMap;

use crate::files::{File, Files};
use crate::listview::ListView;
use crate::miller_columns::MillerColumns;
use crate::widget::Widget;
use crate::tabview::{TabView, Tabbable};
use crate::preview::{Previewer, WillBeWidget};
use crate::fail::{HResult, HError, ErrorLog};
use crate::widget::{Events, WidgetCore};
use crate::proclist::ProcView;
use crate::bookmarks::BMPopup;

#[derive(PartialEq)]
pub enum FileBrowserWidgets {
    FileList(WillBeWidget<ListView<Files>>),
    Previewer(Previewer),
}

impl Widget for FileBrowserWidgets {
    fn get_core(&self) -> HResult<&WidgetCore> {
        match self {
            FileBrowserWidgets::FileList(widget) => widget.get_core(),
            FileBrowserWidgets::Previewer(widget) => widget.get_core()
        }
    }
    fn get_core_mut(&mut self) -> HResult<&mut WidgetCore> {
        match self {
            FileBrowserWidgets::FileList(widget) => widget.get_core_mut(),
            FileBrowserWidgets::Previewer(widget) => widget.get_core_mut()
        }
    }
    fn refresh(&mut self) -> HResult<()> {
        match self {
            FileBrowserWidgets::FileList(widget) => widget.refresh(),
            FileBrowserWidgets::Previewer(widget) => widget.refresh()
        }
    }
    fn get_drawlist(&self) -> HResult<String> {
        match self {
            FileBrowserWidgets::FileList(widget) => widget.get_drawlist(),
            FileBrowserWidgets::Previewer(widget) => widget.get_drawlist()
        }
    }
}

pub struct FileBrowser {
    pub columns: MillerColumns<FileBrowserWidgets>,
    pub cwd: File,
    pub prev_cwd: Option<File>,
    selections: HashMap<File, File>,
    cached_files: HashMap<File, Files>,
    core: WidgetCore,
    watcher: INotifyWatcher,
    watches: Vec<PathBuf>,
    dir_events: Arc<Mutex<Vec<DebouncedEvent>>>,
    proc_view: Arc<Mutex<ProcView>>,
    bookmarks: Arc<Mutex<BMPopup>>
}

impl Tabbable for TabView<FileBrowser> {
    fn new_tab(&mut self) -> HResult<()> {
        let mut tab = FileBrowser::new_cored(&self.active_tab_().core)?;

        let proc_view = self.active_tab_().proc_view.clone();
        let bookmarks = self.active_tab_().bookmarks.clone();
        tab.proc_view = proc_view;
        tab.bookmarks = bookmarks;

        self.push_widget(tab)?;
        self.active += 1;
        Ok(())
    }

    fn close_tab(&mut self) -> HResult<()> {
        self.close_tab_()
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
                let selected_files = self.widgets.iter().fold(HashMap::new(),
                                                              |mut f, w| {
                    let dir = w.cwd().unwrap().clone();
                    let selected_files = w.selected_files().unwrap();
                    f.insert(dir, selected_files);
                    f
                });
                self.widgets[self.active].exec_cmd(tab_dirs, selected_files)
            }
            _ => { self.active_tab_mut().on_key(key) }
        }
    }
}





fn watch_dir(rx: Receiver<DebouncedEvent>,
             dir_events: Arc<Mutex<Vec<DebouncedEvent>>>,
             sender: Sender<Events>) {
    std::thread::spawn(move || {
        for event in rx.iter() {
            dir_events.lock().unwrap().push(event);
            sender.send(Events::WidgetReady).unwrap();
        }
    });
}





impl FileBrowser {
    pub fn new_cored(core: &WidgetCore) -> HResult<FileBrowser> {
        let cwd = std::env::current_dir().unwrap();
        let mut core_m = core.clone();
        let mut core_l = core.clone();
        let mut core_p = core.clone();

        let mut miller = MillerColumns::new(core);
        miller.set_ratios(vec![20,30,49]);
        let list_coords = miller.calculate_coordinates()?;

        core_l.coordinates = list_coords[0].clone();
        core_m.coordinates = list_coords[1].clone();
        core_p.coordinates = list_coords[2].clone();

        let main_path = cwd.ancestors()
                           .take(1)
                           .map(|path| {
                               std::path::PathBuf::from(path)
                           }).last()?;
        let left_path = main_path.parent().map(|p| p.to_path_buf());

        let main_widget = WillBeWidget::new(&core, Box::new(move |_| {
            let mut list = ListView::new(&core_m,
                                         Files::new_from_path(&main_path)?);
            list.animate_slide_up().log();
            Ok(list)
        }));

        if let Some(left_path) = left_path {
            let left_widget = WillBeWidget::new(&core, Box::new(move |_| {
                let mut list = ListView::new(&core_l,
                                             Files::new_from_path(&left_path)?);
                list.animate_slide_up().log();
                Ok(list)
            }));
            let left_widget = FileBrowserWidgets::FileList(left_widget);
            miller.push_widget(left_widget);
        }

        let previewer = Previewer::new(&core_p);

        miller.push_widget(FileBrowserWidgets::FileList(main_widget));
        miller.push_widget(FileBrowserWidgets::Previewer(previewer));
        miller.refresh().log();


        let cwd = File::new_from_path(&cwd).unwrap();
        let dir_events = Arc::new(Mutex::new(vec![]));

        let (tx_watch, rx_watch) = channel();
        let watcher = INotifyWatcher::new(tx_watch, Duration::from_secs(2)).unwrap();
        watch_dir(rx_watch, dir_events.clone(), core.get_sender());

        let proc_view = ProcView::new(&core);
        let bookmarks = BMPopup::new(&core);



        Ok(FileBrowser { columns: miller,
                         cwd: cwd,
                         prev_cwd: None,
                         selections: HashMap::new(),
                         cached_files: HashMap::new(),
                         core: core.clone(),
                         watcher: watcher,
                         watches: vec![],
                         dir_events: dir_events,
                         proc_view: Arc::new(Mutex::new(proc_view)),
                         bookmarks: Arc::new(Mutex::new(bookmarks)) })
    }

    pub fn enter_dir(&mut self) -> HResult<()> {
        let file = self.selected_file()?;

        if file.is_dir() {
            self.main_widget_goto(&file).log();
        } else {
            let status = std::process::Command::new("rifle")
                .args(file.path.file_name())
                .status();

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

    pub fn main_widget_goto(&mut self, dir: &File) -> HResult<()> {
        if dir.read_dir().is_err() {
            self.show_status("Can't enter! Permission denied!").log();
            return Ok(());
        }

        let dir = dir.clone();
        let selected_file = self.get_selection(&dir).ok().cloned();

        self.get_files().and_then(|files| self.cache_files(files)).log();
        let cached_files = self.get_cached_files(&dir).ok();

        self.prev_cwd = Some(self.cwd.clone());
        self.cwd = dir.clone();

        let main_widget = self.main_widget_mut()?;
        main_widget.change_to(Box::new(move |stale, core| {
            let path = dir.path();
            let cached_files = cached_files.clone();

            let files = cached_files.or_else(|| {
                Files::new_from_path_cancellable(&path, stale).ok()
            })?;

            let mut list = ListView::new(&core, files);

            if let Some(file) = &selected_file {
                list.select_file(file);
            }
            Ok(list)
        })).log();

        if let Ok(grand_parent) = self.cwd()?.parent_as_file() {
            self.left_widget_goto(&grand_parent).log();
        } else {
            self.left_widget_mut()?.set_stale().log();
        }

        Ok(())
    }

    pub fn left_widget_goto(&mut self, dir: &File) -> HResult<()> {
        self.get_left_files().and_then(|files| self.cache_files(files)).log();
        let cached_files = self.get_cached_files(&dir).ok();
        let dir = dir.clone();

        let left_widget = self.left_widget_mut()?;
        left_widget.change_to(Box::new(move |stale, core| {
            let path = dir.path();
            let cached_files = cached_files.clone();

            let files = cached_files.or_else(|| {
                Files::new_from_path_cancellable(&path, stale).ok()
            })?;

            let list = ListView::new(&core, files);
            Ok(list)
        }))?;
        Ok(())
    }

    pub fn go_back(&mut self) -> HResult<()> {
        if let Ok(new_cwd) = self.cwd.parent_as_file() {
            self.main_widget_goto(&new_cwd).log();
        }

        self.refresh()
    }

    pub fn goto_prev_cwd(&mut self) -> HResult<()> {
        let prev_cwd = self.prev_cwd.take()?;
        self.main_widget_goto(&prev_cwd)?;
        Ok(())
    }

    pub fn goto_bookmark(&mut self) -> HResult<()> {
        let cwd = match self.prev_cwd.as_ref() {
            Some(cwd) => cwd,
            None => &self.cwd
        }.path.to_string_lossy().to_string();

        let path = self.bookmarks.lock()?.pick(cwd)?;
        let path = File::new_from_path(&PathBuf::from(path))?;
        self.main_widget_goto(&path)?;
        Ok(())
    }

    pub fn add_bookmark(&mut self) -> HResult<()> {
        let cwd = self.cwd.path.to_string_lossy().to_string();
        self.bookmarks.lock()?.add(&cwd)?;
        Ok(())
    }

    pub fn update_preview(&mut self) -> HResult<()> {
        if !self.main_widget()?.ready() { return Ok(()) }
        let file = self.selected_file()?.clone();
        let selection = self.get_selection(&file).ok().cloned();
        let cached_files = self.get_cached_files(&file).ok();
        let preview = self.preview_widget_mut()?;
        preview.set_file(&file, selection, cached_files);
        Ok(())
    }

    pub fn set_left_selection(&mut self) -> HResult<()> {
        if !self.left_widget()?.ready() { return Ok(()) }

        let parent = self.cwd()?.parent_as_file();

        let left_selection = self.get_selection(&parent?)?;
        self.left_widget()?.widget()?.lock()?.as_mut()?.select_file(&left_selection);

        Ok(())
    }

    pub fn get_selection(&self, dir: &File) -> HResult<&File> {
        Ok(self.selections.get(dir)?)
    }

    pub fn get_files(&mut self) -> HResult<Files> {
        Ok(self.main_widget()?.widget()?.lock()?.as_ref()?.content.clone())
    }

    pub fn get_left_files(&mut self) -> HResult<Files> {
        Ok(self.left_widget()?.widget()?.lock()?.as_ref()?.content.clone())
    }

    pub fn cache_files(&mut self, files: Files) -> HResult<()> {
        let dir = files.directory.clone();
        self.cached_files.insert(dir, files);
        Ok(())
    }

    pub fn get_cached_files(&mut self, dir: &File) -> HResult<Files> {
        Ok(self.cached_files.get(dir)?.clone())
    }

    pub fn save_selection(&mut self) -> HResult<()> {
        let cwd = self.cwd()?.clone();
        if let Ok(main_selection) = self.selected_file() {
            self.selections.insert(cwd.clone(), main_selection);
        }
        if let Ok(left_dir) = self.cwd()?.parent_as_file() {
            self.selections.insert(left_dir, cwd);
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

    pub fn left_dir(&self) -> HResult<File> {
        let widget = self.left_widget()?.widget()?;
        let dir = widget.lock()?.as_ref()?.content.directory.clone();
        Ok(dir)
    }

    fn update_watches(&mut self) -> HResult<()> {
        if !self.left_widget()?.ready() || !self.main_widget()?.ready() {
            return Ok(())
        }
        let watched_dirs = self.watches.clone();
        let cwd = self.cwd()?.clone();
        let left_dir = self.left_dir()?;
        let preview_dir = self.selected_file().ok().map(|f| f.path);

        for watched_dir in watched_dirs.iter() {
            if watched_dir != &cwd.path && watched_dir != &left_dir.path &&
                Some(watched_dir.clone()) != preview_dir {
                self.watcher.unwatch(&watched_dir).ok();
                self.watches.remove_item(&watched_dir);
            }
        }
        if !watched_dirs.contains(&cwd.path) {
            self.watcher.watch(&cwd.path, RecursiveMode::NonRecursive)?;
            self.watches.push(cwd.path);
        }
        if !watched_dirs.contains(&left_dir.path) {
            self.watcher.watch(&left_dir.path, RecursiveMode::NonRecursive)?;
            self.watches.push(left_dir.path);
        }
        if let Some(preview_dir) = preview_dir {
            if !watched_dirs.contains(&preview_dir) && preview_dir.is_dir() {
                self.watcher.watch(&preview_dir, RecursiveMode::NonRecursive)?;
                self.watches.push(preview_dir);
            }
        }
        Ok(())
    }

    fn handle_dir_events(&mut self) -> HResult<()> {
        let dir_events =  self.dir_events.clone();
        for event in dir_events.lock()?.iter() {
            let main_widget = self.main_widget()?.widget()?;
            let mut main_widget = main_widget.lock()?;
            let main_result = main_widget.as_mut()?.content.handle_event(event);

            let left_widget = self.left_widget()?.widget()?;
            let mut left_files = left_widget.lock()?;
            let left_result = left_files.as_mut()?.content.handle_event(event);

            match main_result {
                Err(HError::WrongDirectoryError { .. }) => {
                    match left_result {
                        Err(HError::WrongDirectoryError { .. }) => {
                            let preview = self.preview_widget_mut()?;
                            preview.reload();
                        }, _ => {}
                    }
                }, _ => {}
            }
        }
        dir_events.lock()?.clear();
        Ok(())
    }

    pub fn selected_file(&self) -> HResult<File> {
        let widget = self.main_widget()?.widget()?;
        let file = widget.lock()?.as_ref()?.selected_file().clone();
        Ok(file)
    }

    pub fn selected_files(&self) -> HResult<Vec<File>> {
        let widget = self.main_widget()?.widget()?;
        let files = widget.lock()?.as_ref()?.content.get_selected().into_iter().map(|f| {
            f.clone()
        }).collect();
        Ok(files)
    }

    pub fn main_widget(&self) -> HResult<&WillBeWidget<ListView<Files>>> {
        let widget = match self.columns.get_main_widget()? {
            FileBrowserWidgets::FileList(filelist) => Ok(filelist),
            _ => { return HError::wrong_widget("previewer", "filelist"); }
        };
        widget
    }

    pub fn main_widget_mut(&mut self) -> HResult<&mut WillBeWidget<ListView<Files>>> {
        let widget = match self.columns.get_main_widget_mut()? {
            FileBrowserWidgets::FileList(filelist) => Ok(filelist),
            _ => { return HError::wrong_widget("previewer", "filelist"); }
        };
        widget
    }

    pub fn left_widget(&self) -> HResult<&WillBeWidget<ListView<Files>>> {
        let widget = match self.columns.get_left_widget()? {
            FileBrowserWidgets::FileList(filelist) => Ok(filelist),
            _ => { return HError::wrong_widget("previewer", "filelist"); }
        };
        widget
    }

    pub fn left_widget_mut(&mut self) -> HResult<&mut WillBeWidget<ListView<Files>>> {
        let widget = match self.columns.get_left_widget_mut()? {
            FileBrowserWidgets::FileList(filelist) => Ok(filelist),
            _ => { return HError::wrong_widget("previewer", "filelist"); }
        };
        widget
    }

    pub fn preview_widget(&self) -> HResult<&Previewer> {
        match self.columns.get_right_widget()? {
            FileBrowserWidgets::Previewer(previewer) => Ok(previewer),
            _ => { return HError::wrong_widget("filelist", "previewer"); }
        }
    }

    pub fn preview_widget_mut(&mut self) -> HResult<&mut Previewer> {
        match self.columns.get_right_widget_mut()? {
            FileBrowserWidgets::Previewer(previewer) => Ok(previewer),
            _ => { return HError::wrong_widget("filelist", "previewer"); }
        }
    }

    pub fn quit_with_dir(&self) -> HResult<()> {
        let cwd = self.cwd()?.clone().path;
        let selected_file = self.selected_file()?;
        let selected_file = selected_file.path.to_string_lossy();

        let mut filepath = dirs_2::home_dir()?;
        filepath.push(".hunter_cwd");

        let output = format!("HUNTER_CWD=\"{}\"\nF=\"{}\"",
                             cwd.to_str()?,
                             selected_file);

        let mut file = std::fs::File::create(filepath)?;
        file.write(output.as_bytes())?;
        HError::quit()
    }

    pub fn turbo_cd(&mut self) -> HResult<()> {
        let dir = self.minibuffer("cd: ");

        match dir {
            Ok(dir) => {
                self.columns.widgets.widgets.clear();
                let cwd = File::new_from_path(&std::path::PathBuf::from(&dir))?;
                self.cwd = cwd;
                let dir = std::path::PathBuf::from(&dir);
                let left_dir = std::path::PathBuf::from(&dir);
                let mcore = self.main_widget()?.get_core()?.clone();
                let lcore = self.left_widget()?.get_core()?.clone();;

                let middle = WillBeWidget::new(&self.core, Box::new(move |_| {
                    let files = Files::new_from_path(&dir.clone())?;
                    let listview = ListView::new(&mcore, files);
                    Ok(listview)
                }));
                let middle = FileBrowserWidgets::FileList(middle);

                let left = WillBeWidget::new(&self.core, Box::new(move |_| {
                    let files = Files::new_from_path(&left_dir.parent()?)?;
                    let listview = ListView::new(&lcore, files);
                    Ok(listview)
                }));
                let left = FileBrowserWidgets::FileList(left);
                self.columns.push_widget(left);
                self.columns.push_widget(middle);
            },
            Err(_) => {}
        }
        Ok(())
    }

    fn exec_cmd(&mut self,
                tab_dirs: Vec<File>,
                tab_files: HashMap<File, Vec<File>>) -> HResult<()> {
        let cwd = self.cwd()?;
        let filename = self.selected_file()?.name.clone();
        let selected_files = self.selected_files()?;

        let file_names
            = selected_files.iter().map(|f| f.name.clone()).collect::<Vec<String>>();

        let cmd = self.minibuffer("exec:")?;

        self.show_status(&format!("Running: \"{}\"", &cmd)).log();

        let mut cmd = if file_names.len() == 0 {
            cmd.replace("$s", &format!("{}", &filename))
        } else {
            let args = file_names.iter().map(|f| {
                format!(" \"{}\" ", f)
            }).collect::<String>();
            cmd.replace("$s", &args)
        };

        for (i, tab_dir) in tab_dirs.iter().enumerate() {
            if let Some(tab_files) = tab_files.get(tab_dir) {
                let tab_file_identifier = format!("${}s", i);
                let args = tab_files.iter().map(|f| {
                    let file_path = f.strip_prefix(&cwd);
                    format!(" \"{}\" ", file_path.to_string_lossy())
                }).collect::<String>();
                cmd = cmd.replace(&tab_file_identifier, &args);
            }

            let tab_identifier = format!("${}", i);
            let tab_path = tab_dir.path.to_string_lossy();
            cmd = cmd.replace(&tab_identifier, &tab_path);
        }

        self.proc_view.lock()?.run_proc(&cmd)?;

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

        let main_widget = self.main_widget()?.widget()?;
        let selection = main_widget.lock()?.as_ref().unwrap().get_selection();
        let file_count = main_widget.lock()?.as_ref().unwrap().content.len();
        let file_count = format!("{}", file_count);
        let digits = file_count.len();
        let file_count = format!("{:digits$}/{:digits$}",
                                 selection,
                                 file_count,
                                 digits = digits);
        let count_xpos = xsize - file_count.len() as u16;
        let count_ypos = ypos + self.get_coordinates()?.ysize();

        let status = format!("{} {}:{} {} {} {}", permissions, user, group, mtime,
                             crate::term::goto_xy(count_xpos, count_ypos), file_count);
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
    fn render_header(&self) -> HResult<String> {
        let xsize = self.get_coordinates()?.xsize();
        let file = self.selected_file()?;
        let name = &file.name;

        let color = if file.is_dir() || file.color.is_none() {
            crate::term::highlight_color() } else {
            crate::term::from_lscolor(file.color.as_ref().unwrap()) };

        let path = file.path.parent()?.to_string_lossy().to_string();

        let pretty_path = format!("{}/{}{}", path, &color, name );
        let sized_path = crate::term::sized_string(&pretty_path, xsize);
        Ok(sized_path)
    }
    fn render_footer(&self) -> HResult<String> {
        match self.get_core()?.status_bar_content.lock()?.as_mut().take() {
            Some(status) => Ok(status.clone()),
            _ => { self.get_footer() },
        }
    }
    fn refresh(&mut self) -> HResult<()> {
        //self.proc_view.lock()?.set_coordinates(self.get_coordinates()?);
        self.handle_dir_events().ok();
        self.columns.refresh().ok();
        self.set_left_selection().log();
        self.save_selection().log();
        self.set_cwd().ok();
        self.update_watches().ok();
        self.update_preview().ok();
        self.columns.refresh().ok();
        Ok(())
    }

    fn get_drawlist(&self) -> HResult<String> {
        let left = self.left_widget()?.get_drawlist()?;
        let main = self.main_widget()?.get_drawlist()?;
        let prev = self.preview_widget()?.get_drawlist()?;

        Ok(left + &main + &prev)
    }

    fn on_key(&mut self, key: Key) -> HResult<()> {
        match key {
            Key::Char('/') => { self.turbo_cd()?; },
            Key::Char('Q') => { self.quit_with_dir()?; },
            Key::Right | Key::Char('f') => { self.enter_dir()?; },
            Key::Left | Key::Char('b') => { self.go_back()?; },
            Key::Char('-') => { self.goto_prev_cwd()?; },
            Key::Char('`') => { self.goto_bookmark()?; },
            Key::Char('m') => { self.add_bookmark()?; },
            Key::Char('w') => {
                self.proc_view.lock()?.popup()?;
            }
                                ,
            _ => { self.main_widget_mut()?.on_key(key)?; },
        }
        self.update_preview()?;
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
