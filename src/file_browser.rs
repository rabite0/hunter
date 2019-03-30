use termion::event::Key;

use std::io::Write;
use std::sync::{Arc, Mutex};
use std::path::PathBuf;
use std::ffi::OsString;

use crate::files::{File, Files, PathBufExt};
use crate::fscache::FsCache;
use crate::listview::ListView;
use crate::hbox::HBox;
use crate::widget::Widget;
use crate::tabview::{TabView, Tabbable};
use crate::preview::{Previewer, AsyncWidget};
use crate::fail::{HResult, HError, ErrorLog};
use crate::widget::{Events, WidgetCore};
use crate::proclist::ProcView;
use crate::bookmarks::BMPopup;
use crate::term;
use crate::term::ScreenExt;
use crate::foldview::LogView;
use crate::coordinates::Coordinates;

#[derive(PartialEq)]
pub enum FileBrowserWidgets {
    FileList(AsyncWidget<ListView<Files>>),
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
    fn set_coordinates(&mut self, coordinates: &Coordinates) -> HResult<()> {
        match self {
            FileBrowserWidgets::FileList(widget) => widget.set_coordinates(coordinates),
            FileBrowserWidgets::Previewer(widget) => widget.set_coordinates(coordinates),
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
    pub columns: HBox<FileBrowserWidgets>,
    pub cwd: File,
    pub prev_cwd: Option<File>,
    core: WidgetCore,
    proc_view: Arc<Mutex<ProcView>>,
    bookmarks: Arc<Mutex<BMPopup>>,
    log_view: Arc<Mutex<LogView>>,
    fs_cache: FsCache,
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
                        w.selected_files()
                            .map_err(|_| Vec::<Files>::new())
                            .unwrap()
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
        Ok(())
    }
}







impl FileBrowser {
    pub fn new(core: &WidgetCore, cache: Option<FsCache>) -> HResult<FileBrowser> {
        let startup = cache.is_none();
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
            let main_dir = File::new(&main_path.file_name()?
                                     .to_string_lossy()
                                     .to_string(),
                                     main_path.clone(),
                                     None);
            let files = cache.get_files_sync(&main_dir)?;
            let selection = cache.get_selection(&main_dir).ok();
            let mut list = ListView::new(&core_m.clone(),
                                         files);
            if let Some(file) = selection {
                list.select_file(&file);
            }

            list.refresh().log();

            if startup {
                list.animate_slide_up().log();
            }

            list.content.meta_all();
            Ok(list)
        }));

        let cache = fs_cache.clone();
        if let Some(left_path) = left_path {
            let left_widget = AsyncWidget::new(&core, Box::new(move |_| {
                let left_dir = File::new(&left_path.file_name()?
                                         .to_string_lossy()
                                         .to_string(),
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

                if startup {
                    list.animate_slide_up().log();
                }

                Ok(list)
            }));
            let left_widget = FileBrowserWidgets::FileList(left_widget);
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



        Ok(FileBrowser { columns: columns,
                         cwd: cwd,
                         prev_cwd: None,
                         core: core.clone(),
                         proc_view: Arc::new(Mutex::new(proc_view)),
                         bookmarks: Arc::new(Mutex::new(bookmarks)),
                         log_view: Arc::new(Mutex::new(log_view)),
                         fs_cache: fs_cache,
        })
    }

    pub fn enter_dir(&mut self) -> HResult<()> {
        let file = self.selected_file()?;

        if file.is_dir() {
            match file.is_readable() {
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

            self.main_widget_goto(&file).log();
        } else {
            self.core.get_sender().send(Events::InputEnabled(false))?;

            let status = std::process::Command::new("rifle")
                .args(file.path.file_name())
                .status();

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
            self.main_widget_goto(&new_cwd).log();
        }

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

        loop {
            let bookmark =  self.bookmarks.lock()?.pick(cwd.to_string());

            if let Err(HError::TerminalResizedError) = bookmark {
                    self.core.screen.clear().log();
                    self.resize().log();
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
        if !self.left_async_widget_mut()?.ready() { return Ok(()) }
        if self.cwd.parent().is_none() { return Ok(()) }

        let selection = self.cwd()?.clone();

        self.left_widget_mut()?.select_file(&selection);

        Ok(())
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


        let left_selection = self.left_widget()?.clone_selected_file();
        let left_files = self.get_left_files()?;
        self.fs_cache.put_files(left_files, Some(left_selection)).log();
        self.left_widget_mut()?.content.meta_updated = false;

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

    pub fn preview_widget_mut(&mut self) -> HResult<&mut Previewer> {
        match self.columns.widgets.get_mut(2)? {
            FileBrowserWidgets::Previewer(previewer) => Ok(previewer),
            _ => { return HError::wrong_widget("filelist", "previewer"); }
        }
    }

    pub fn toggle_colums(&mut self) {
        self.columns.toggle_zoom().log();
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
        let dir = self.minibuffer("cd");

        match dir {
            Ok(dir) => {
                self.columns.widgets.clear();
                let cwd = File::new_from_path(&std::path::PathBuf::from(&dir), None)?;
                self.cwd = cwd;
                let dir = std::path::PathBuf::from(&dir);
                let left_dir = std::path::PathBuf::from(&dir);
                let mcore = self.main_widget()?.get_core()?.clone();
                let lcore = self.left_widget()?.get_core()?.clone();;

                let middle = AsyncWidget::new(&self.core, Box::new(move |_| {
                    let files = Files::new_from_path(&dir.clone())?;
                    let listview = ListView::new(&mcore, files);
                    Ok(listview)
                }));
                let middle = FileBrowserWidgets::FileList(middle);

                let left = AsyncWidget::new(&self.core, Box::new(move |_| {
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
                tab_files: Vec<Vec<File>>) -> HResult<()> {

        let cwd = self.cwd()?.clone();
        let selected_file = self.selected_file()?;
        let selected_files = self.selected_files()?;

        let cmd = self.minibuffer("exec")?.trim_start().to_string() + " ";

        let cwd_files = if selected_files.len() == 0 {
            vec![selected_file]
        } else { selected_files };

        let cmd = crate::proclist::Cmd {
            cmd: OsString::from(cmd),
            short_cmd: None,
            args: None,
            cwd: cwd,
            cwd_files: Some(cwd_files),
            tab_files: Some(tab_files),
            tab_paths: Some(tab_dirs)
        };

        self.proc_view.lock()?.run_proc_subshell(cmd)?;

        Ok(())
    }

    pub fn run_subshell(&mut self) -> HResult<()> {
        self.core.get_sender().send(Events::InputEnabled(false))?;

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

    pub fn get_footer(&self) -> HResult<String> {
        let xsize = self.get_coordinates()?.xsize();
        let ypos = self.get_coordinates()?.position().y();
        let pos = self.main_widget()?.get_selection();
        let file = self.main_widget()?.content.files.get(pos)?;

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

        let status = format!("{}{}{}{}",
                             status,
                             crate::term::header_color(),
                             crate::term::goto_xy(count_xpos, count_ypos),
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

        let color = if file.is_dir() || file.color.is_none() {
            crate::term::highlight_color() } else {
            crate::term::from_lscolor(file.color.as_ref().unwrap()) };

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
            Key::Char('/') => { self.turbo_cd()?; },
            Key::Char('Q') => { self.quit_with_dir()?; },
            Key::Right | Key::Char('f') => { self.enter_dir()?; },
            Key::Char('F') => { self.open_bg()?; },
            Key::Left | Key::Char('b') => { self.go_back()?; },
            Key::Char('-') => { self.goto_prev_cwd()?; },
            Key::Char('`') => { self.goto_bookmark()?; },
            Key::Char('m') => { self.add_bookmark()?; },
            Key::Char('w') => { self.proc_view.lock()?.popup()?; },
            Key::Char('l') => self.log_view.lock()?.popup()?,
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
