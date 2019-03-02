use termion::event::Key;
use notify::{INotifyWatcher, Watcher, DebouncedEvent, RecursiveMode};

use std::io::Write;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Duration;
use std::path::PathBuf;

use crate::files::{File, Files};
use crate::listview::ListView;
use crate::miller_columns::MillerColumns;
use crate::widget::Widget;
use crate::tabview::{TabView, Tabbable};
use crate::preview::WillBeWidget;
use crate::fail::{HResult, HError, ErrorLog};
use crate::widget::{Events, WidgetCore};
use crate::proclist::ProcView;



pub struct FileBrowser {
    pub columns: MillerColumns<WillBeWidget<ListView<Files>>>,
    pub cwd: File,
    core: WidgetCore,
    watcher: INotifyWatcher,
    watches: Vec<PathBuf>,
    dir_events: Arc<Mutex<Vec<DebouncedEvent>>>,
    proc_view: Arc<Mutex<ProcView>>,
}

impl Tabbable for TabView<FileBrowser> {
    fn new_tab(&mut self) -> HResult<()> {
        let mut tab = FileBrowser::new_cored(&self.active_tab_().core)?;

        let proc_view = self.active_tab_().proc_view.clone();
        tab.proc_view = proc_view;

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

    fn on_next_tab(&mut self) -> HResult<()> {
        self.active_tab_mut().refresh()
    }

    fn on_key_sub(&mut self, key: Key) -> HResult<()> {
        match key {
            Key::Char('!') => {
                let tab_dirs = self.widgets.iter().map(|w| w.cwd.clone())
                                                  .collect::<Vec<_>>();
                self.widgets[self.active].exec_cmd(tab_dirs)
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
        let coords = core.coordinates.clone();
        let core_ = core.clone();

        let mut miller = MillerColumns::new(core);
        miller.set_coordinates(&coords)?;


        let (_, main_coords, _) = miller.calculate_coordinates();

        let main_path: std::path::PathBuf = cwd.ancestors().take(1).map(|path| std::path::PathBuf::from(path)).collect();
        let main_widget = WillBeWidget::new(&core, Box::new(move |_| {
            let mut list = ListView::new(&core_,
                                         Files::new_from_path(&main_path).unwrap());
            list.set_coordinates(&main_coords).log();
            list.animate_slide_up().log();
            Ok(list)
        }));

        miller.push_widget(main_widget);


        let cwd = File::new_from_path(&cwd).unwrap();
        let dir_events = Arc::new(Mutex::new(vec![]));

        let (tx_watch, rx_watch) = channel();
        let watcher = INotifyWatcher::new(tx_watch, Duration::from_secs(2)).unwrap();
        watch_dir(rx_watch, dir_events.clone(), core.get_sender());

        let mut proc_view = ProcView::new(core);
        proc_view.set_coordinates(&coords).log();

        Ok(FileBrowser { columns: miller,
                         cwd: cwd,
                         core: core.clone(),
                         watcher: watcher,
                         watches: vec![],
                         dir_events: dir_events,
                         proc_view: Arc::new(Mutex::new(proc_view)) })
    }

    pub fn enter_dir(&mut self) -> HResult<()> {
        let file = self.selected_file()?;
        let (_, coords, _) = self.columns.calculate_coordinates();
        let core = self.core.clone();

        match file.read_dir() {
            Ok(files) => {
                std::env::set_current_dir(&file.path).unwrap();
                self.cwd = file.clone();
                let view = WillBeWidget::new(&core.clone(), Box::new(move |_| {
                    let files = files.clone();
                    let mut list = ListView::new(&core, files);
                    list.set_coordinates(&coords).log();
                    list.animate_slide_up().log();
                    Ok(list)
                }));
                self.columns.push_widget(view);
            },
            _ => {
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

                };
            }
        }
        Ok(())
    }

    pub fn go_back(&mut self) -> HResult<()> {
        self.columns.pop_widget();

        if let Ok(new_cwd) = self.cwd.grand_parent_as_file() {
            self.cwd = new_cwd;
        }

        self.refresh()
    }

    pub fn update_preview(&mut self) -> HResult<()> {
        let file = self.selected_file()?.clone();
        let preview = &mut self.columns.preview;
        preview.set_file(&file);
        Ok(())
    }

    pub fn fix_selection(&mut self) -> HResult<()> {
        let cwd = self.cwd()?;
        (*self.left_widget()?.lock()?).as_mut()?.select_file(&cwd);
        Ok(())
    }

    pub fn fix_left(&mut self) -> HResult<()> {
        if self.left_widget().is_err() {
            let cwd = self.selected_file()?.clone();
            if let Ok(grand_parent) = cwd.grand_parent_as_file() {
                let (coords, _, _) = self.columns.calculate_coordinates();
                let core = self.core.clone();
                let left_view = WillBeWidget::new(&self.core, Box::new(move |_| {
                    let mut view
                        = ListView::new(&core,
                                        Files::new_from_path(&grand_parent.path)?);
                    view.set_coordinates(&coords).log();
                    Ok(view)
                }));
                self.columns.prepend_widget(left_view);
            }
        }
        Ok(())
    }

    pub fn cwd(&self) -> HResult<File> {
        let widget = self.columns.get_main_widget()?.widget()?;
        let cwd = (*widget.lock()?).as_ref()?.content.directory.clone();
        Ok(cwd)
    }

    pub fn set_cwd(&mut self) -> HResult<()> {
        let cwd = self.cwd()?;
        std::env::set_current_dir(&cwd.path)?;
        self.cwd = cwd;
        Ok(())
    }

    pub fn left_dir(&self) -> HResult<File> {
        let widget = self.columns.get_left_widget()?.widget()?;
        let dir = (*widget.lock()?).as_ref()?.content.directory.clone();
        Ok(dir)
    }

    fn update_watches(&mut self) -> HResult<()> {
        let watched_dirs = self.watches.clone();
        let cwd = self.cwd()?;
        let left_dir = self.left_dir()?;
        let preview_dir = self.selected_file().ok().map(|f| f.path);

        for watched_dir in watched_dirs.iter() {
            if watched_dir != &cwd.path && watched_dir != &left_dir.path &&
                Some(watched_dir.clone()) != preview_dir {
                self.watcher.unwatch(&watched_dir).unwrap();
                self.watches.remove_item(&watched_dir);
            }
        }
        if !watched_dirs.contains(&cwd.path) {
            self.watcher.watch(&cwd.path, RecursiveMode::NonRecursive).unwrap();
            self.watches.push(cwd.path);
        }
        if !watched_dirs.contains(&left_dir.path) {
            self.watcher.watch(&left_dir.path, RecursiveMode::NonRecursive).unwrap();
            self.watches.push(left_dir.path);
        }
        if let Some(preview_dir) = preview_dir {
            if !watched_dirs.contains(&preview_dir) && preview_dir.is_dir() {
            self.watcher.watch(&preview_dir, RecursiveMode::NonRecursive).unwrap();
            self.watches.push(preview_dir);
            }
        }
        Ok(())
    }

    fn handle_dir_events(&mut self) -> HResult<()> {
        let mut dir_events =  self.dir_events.lock()?;
        for event in dir_events.iter() {
            let main_widget = self.columns.get_main_widget()?.widget()?;
            let main_files = &mut (*main_widget.lock()?);
            let main_files = &mut main_files.as_mut()?.content;
            let main_result = main_files.handle_event(event);

            let left_widget = self.columns.get_left_widget()?.widget()?;
            let left_files = &mut (*left_widget.lock()?);
            let left_files = &mut left_files.as_mut()?.content;
            let left_result = left_files.handle_event(event);

            match main_result {
                Err(HError::WrongDirectoryError { .. }) => {
                    match left_result {
                        Err(HError::WrongDirectoryError { .. }) => {
                            let preview = &mut self.columns.preview;
                            preview.reload();
                        }, _ => {}
                    }
                }, _ => {}
            }
        }
        dir_events.clear();
        Ok(())
    }

    pub fn selected_file(&self) -> HResult<File> {
        let widget = self.main_widget()?;
        let file = widget.lock()?.as_ref()?.selected_file().clone();
        Ok(file)
    }

    pub fn main_widget(&self) -> HResult<Arc<Mutex<Option<ListView<Files>>>>> {
        let widget = self.columns.get_main_widget()?.widget()?;
        Ok(widget)
    }

    pub fn left_widget(&self) -> HResult<Arc<Mutex<Option<ListView<Files>>>>> {
        let widget = self.columns.get_left_widget()?.widget()?;
        Ok(widget)
    }

    pub fn quit_with_dir(&self) -> HResult<()> {
        let cwd = self.cwd()?.path;
        let selected_file = self.selected_file()?;
        let selected_file = selected_file.path.to_string_lossy();

        let mut filepath = dirs_2::home_dir()?;
        filepath.push(".hunter_cwd");

        let output = format!("HUNTER_CWD=\"{}\"\nF=\"{}\"",
                             cwd.to_str()?,
                             selected_file);

        let mut file = std::fs::File::create(filepath)?;
        file.write(output.as_bytes())?;
        panic!("Quitting!");
        Ok(())
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
                let (left_coords, main_coords, _) = self.columns.calculate_coordinates();
                let mcore = self.core.clone();
                let lcore = self.core.clone();

                let middle = WillBeWidget::new(&self.core, Box::new(move |_| {
                    let files = Files::new_from_path(&dir.clone())?;
                    let mut listview = ListView::new(&mcore, files);
                    listview.set_coordinates(&main_coords).log();
                    Ok(listview)
                }));
                let left = WillBeWidget::new(&self.core, Box::new(move |_| {
                    let files = Files::new_from_path(&left_dir.parent()?)?;
                    let mut listview = ListView::new(&lcore, files);
                    listview.set_coordinates(&left_coords).log();
                    Ok(listview)
                }));
                self.columns.push_widget(left);
                self.columns.push_widget(middle);
            },
            Err(_) => {}
        }
        Ok(())
    }

    fn exec_cmd(&mut self, tab_dirs: Vec<File>) -> HResult<()> {
        let filename = self.selected_file()?.name.clone();
        let widget = self.main_widget()?;
        let widget = widget.lock()?;
        let selected_files = (*widget).as_ref()?.content.get_selected();

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
            let tab_identifier = format!("${}", i);
            let tab_path = tab_dir.path.to_string_lossy();
            cmd = cmd.replace(&tab_identifier, &tab_path);
        }

        self.proc_view.lock()?.run_proc(&cmd)?;

        Ok(())
    }
}

impl Widget for FileBrowser {
    fn get_core(&self) -> HResult<&WidgetCore> {
        Ok(&self.core)
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
        let xsize = self.get_coordinates()?.xsize();
        let ypos = self.get_coordinates()?.position().y();
        let file = self.selected_file()?;

        let permissions = file.pretty_print_permissions().unwrap_or("NOPERMS".into());
        let user = file.pretty_user().unwrap_or("NOUSER".into());
        let group = file.pretty_group().unwrap_or("NOGROUP".into());
        let mtime = file.pretty_mtime().unwrap_or("NOMTIME".into());


        let selection = (*self.main_widget().as_ref().unwrap().lock()?).as_ref()?.get_selection();
        let file_count = (*self.main_widget()?.lock()?).as_ref()?.content.len();
        let file_count = format!("{}", file_count);
        let digits = file_count.len();
        let file_count = format!("{:digits$}/{:digits$}",
                                 selection,
                                 file_count,
                                 digits = digits);
        let count_xpos = xsize - file_count.len() as u16;
        let count_ypos = ypos + self.get_coordinates()?.ysize();

        Ok(format!("{} {}:{} {} {} {}", permissions, user, group, mtime,
                   crate::term::goto_xy(count_xpos, count_ypos), file_count))
     }
    fn refresh(&mut self) -> HResult<()> {
        //self.proc_view.lock()?.set_coordinates(self.get_coordinates()?);
        self.handle_dir_events()?;
        self.columns.refresh().ok();
        self.fix_left().ok();
        self.fix_selection().ok();
        self.set_cwd().ok();
        self.update_watches().ok();
        self.update_preview().ok();
        Ok(())
    }

    fn get_drawlist(&self) -> HResult<String> {
        if self.columns.get_left_widget().is_err() {
            Ok(self.columns.get_clearlist()? + &self.columns.get_drawlist()?)
        } else {
            Ok(self.columns.get_drawlist()?)
        }
    }

    fn on_key(&mut self, key: Key) -> HResult<()> {
        match key {
            Key::Char('/') => { self.turbo_cd()?; },
            Key::Char('Q') => { self.quit_with_dir()?; },
            Key::Right | Key::Char('f') => { self.enter_dir()?; },
            Key::Left | Key::Char('b') => { self.go_back()?; },
            Key::Char('w') => {
                self.proc_view.lock()?.popup()?;
            }
                                ,
            _ => { self.columns.get_main_widget_mut()?.on_key(key)?; },
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

