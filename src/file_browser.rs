use termion::event::Key;

use std::error::Error;
use std::io::Write;
use std::sync::{Arc, Mutex};

use crate::coordinates::{Coordinates};
use crate::files::{File, Files};
use crate::listview::ListView;
use crate::miller_columns::MillerColumns;
use crate::widget::Widget;
use crate::tabview::{TabView, Tabbable};
use crate::preview::WillBeWidget;
use crate::fail::{HError, HResult};

#[derive(PartialEq)]
pub struct FileBrowser {
    pub columns: MillerColumns<WillBeWidget<ListView<Files>>>,
    pub cwd: File
}

impl Tabbable for TabView<FileBrowser> {
    fn new_tab(&mut self) {
        let tab = FileBrowser::new().unwrap();
        self.push_widget(tab);
        self.active += 1;
    }

    fn close_tab(&mut self) {
        self.close_tab_();
    }

    fn next_tab(&mut self) {
        self.next_tab_();
    }

    fn active_tab(& self) -> & dyn Widget {
        self.active_tab_()
    }

    fn active_tab_mut(&mut self) -> &mut dyn Widget {
        self.active_tab_mut_()
    }

    fn on_next_tab(&mut self) {
        self.active_tab_mut().refresh();
    }
}

impl FileBrowser {
    pub fn new() -> Result<FileBrowser, Box<Error>> {
        let cwd = std::env::current_dir().unwrap();
        let coords = Coordinates::new_at(crate::term::xsize(),
                                         crate::term::ysize() - 2,
                                         1,
                                         2);

        let mut miller = MillerColumns::new();
        miller.set_coordinates(&coords);


        // let lists: Result<Vec<ListView<Files>>, Box<Error>> = cwd
        //     .ancestors()
        //     .map(|path| Ok(ListView::new(Files::new_from_path(path)?)))
        //     .take(2)
        //     .collect();
        // let mut lists = lists?;
        // lists.reverse();
        let (left_coords, main_coords, _) = miller.calculate_coordinates();

        let main_path: std::path::PathBuf = cwd.ancestors().take(1).map(|path| std::path::PathBuf::from(path)).collect();
        let main_widget = WillBeWidget::new(Box::new(move |_| {
            let mut list = ListView::new(Files::new_from_path(&main_path).unwrap());
            list.set_coordinates(&main_coords);
            list.animate_slide_up();
            Ok(list)
        }));

        let left_path: std::path::PathBuf = cwd.ancestors().skip(1).take(1).map(|path| std::path::PathBuf::from(path)).collect();
        let left_widget = WillBeWidget::new(Box::new(move |_| {
            let mut list = ListView::new(Files::new_from_path(&left_path).unwrap());
            list.set_coordinates(&left_coords);
            list.animate_slide_up();
            Ok(list)
        }));




        miller.push_widget(left_widget);
        miller.push_widget(main_widget);

        // for widget in lists {
        //     miller.push_widget(widget);
        // }

        let cwd = File::new_from_path(&cwd).unwrap();

        let mut file_browser = FileBrowser { columns: miller,
                                             cwd: cwd };



        //file_browser.fix_selection();
        //file_browser.animate_columns();
        //file_browser.update_preview();

        Ok(file_browser)
    }

    pub fn enter_dir(&mut self) -> HResult<()> {
        let file = self.selected_file()?;
        let (_, coords, _) = self.columns.calculate_coordinates();

        match file.read_dir() {
            Ok(files) => {
                std::env::set_current_dir(&file.path).unwrap();
                let view = WillBeWidget::new(Box::new(move |_| {
                    let files = files.clone();
                    let mut list = ListView::new(files);
                    list.set_coordinates(&coords);
                    list.animate_slide_up();
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
                                                  "rifle", status)),
                    Err(err) =>
                        self.show_status(&format!("Can't run this \"{}\": {}",
                                                  "rifle", err))

                }
            }
        }
        Ok(())
    }

    pub fn go_back(&mut self) -> HResult<()> {
        let path = self.selected_file()?.grand_parent()?;
        std::env::set_current_dir(path)?;
        self.columns.pop_widget();

        // Make sure there's a directory on the left unless it's /
        if self.left_widget().is_err() {
            let file = self.selected_file()?.clone();
            if let Some(grand_parent) = file.grand_parent() {
                let mut left_view = WillBeWidget::new(Box::new(move |_| {
                    let mut view
                        = ListView::new(Files::new_from_path(&grand_parent)?);
                    Ok(view)
                }));
                self.columns.prepend_widget(left_view);
            }
        }
        self.columns.refresh();
        Ok(())
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

    pub fn cwd(&self) -> HResult<File> {
        let widget = self.columns.get_main_widget()?.widget()?;
        let cwd = (*widget.lock()?).as_ref()?.content.directory.clone();
        Ok(cwd)
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

    pub fn animate_columns(&mut self) {
        self.columns.get_left_widget_mut().map(|w| w.animate_slide_up());
        self.columns.get_main_widget_mut().unwrap().animate_slide_up();
    }

    pub fn turbo_cd(&mut self) {
        let dir = self.minibuffer("cd: ");

        // match dir {
        //     Some(dir) => {
        //         Files::new_from_path(&std::path::PathBuf::from(&dir)).and_then(|files| {
        //             let cwd = files.directory.clone();
        //             self.columns.widgets.widgets.clear();
        //             self.columns.push_widget(ListView::new(files));

        //             std::env::set_current_dir(&cwd.path).unwrap();

        //             if let Some(grand_parent) = cwd.path.parent() {
        //                 let left_view =
        //                     ListView::new(Files::new_from_path(&grand_parent).unwrap());
        //                 self.columns.prepend_widget(left_view);
        //             }
        //             self.fix_selection();
        //             self.update_preview();
        //             self.refresh();
        //             self.columns.refresh();
        //             Ok(())
        //         }).ok();
        //     } None => {}
        // }
    }
}

impl Widget for FileBrowser {
    fn get_coordinates(&self) -> &Coordinates {
        &self.columns.coordinates
    }
    fn set_coordinates(&mut self, coordinates: &Coordinates) {
        self.columns.coordinates = coordinates.clone();
        self.refresh();
    }
    fn render_header(&self) -> String {
        if self.main_widget().is_err() { return "".to_string() }
        let xsize = self.get_coordinates().xsize();
        let file = self.selected_file().unwrap();
        let name = &file.name;

        let color = if file.is_dir() || file.color.is_none() {
            crate::term::highlight_color() } else {
            crate::term::from_lscolor(file.color.as_ref().unwrap()) };

        let path = file.path.parent().unwrap().to_string_lossy().to_string();

        let pretty_path = format!("{}/{}{}", path, &color, name );
        let sized_path = crate::term::sized_string(&pretty_path, xsize);
        sized_path
    }
    fn render_footer(&self) -> String {
        if self.main_widget().is_err() { return "".to_string() }
        let xsize = self.get_coordinates().xsize();
        let ypos = self.get_coordinates().position().y();
        let file = self.selected_file().unwrap();

        let permissions = file.pretty_print_permissions().unwrap_or("NOPERMS".into());
        let user = file.pretty_user().unwrap_or("NOUSER".into());
        let group = file.pretty_group().unwrap_or("NOGROUP".into());
        let mtime = file.pretty_mtime().unwrap_or("NOMTIME".into());


        let selection = (*self.main_widget().as_ref().unwrap().lock().unwrap()).as_ref().unwrap().get_selection();
        let file_count = (*self.main_widget().unwrap().lock().unwrap()).as_ref().unwrap().content.len();
        let file_count = format!("{}", file_count);
        let digits = file_count.len();
        let file_count = format!("{:digits$}/{:digits$}",
                                 selection,
                                 file_count,
                                 digits = digits);
        let count_xpos = xsize - file_count.len() as u16;
        let count_ypos = ypos + self.get_coordinates().ysize();

        format!("{} {}:{} {} {} {}", permissions, user, group, mtime,
                crate::term::goto_xy(count_xpos, count_ypos), file_count)
     }
    fn refresh(&mut self) {
        self.update_preview();
        self.fix_selection();
        self.columns.refresh();
    }

    fn get_drawlist(&self) -> String {
        if self.columns.get_left_widget().is_err() {
            self.columns.get_clearlist() + &self.columns.get_drawlist()
        } else {
            self.columns.get_drawlist()
        }
    }

    fn on_key(&mut self, key: Key) {
        match key {
            Key::Char('/') => self.turbo_cd(),
            Key::Char('Q') => { self.quit_with_dir(); },
            Key::Right | Key::Char('f') => { self.enter_dir(); },
            Key::Left | Key::Char('b') => { self.go_back(); },
            _ => self.columns.get_main_widget_mut().unwrap().on_key(key),
        }
        self.update_preview();
    }
}
