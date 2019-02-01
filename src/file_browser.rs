use termion::event::Key;

use std::error::Error;
use std::io::Write;

use crate::coordinates::{Coordinates, Position, Size};
use crate::files::{File, Files};
use crate::listview::ListView;
use crate::miller_columns::MillerColumns;
use crate::widget::Widget;

pub struct FileBrowser {
    pub columns: MillerColumns<ListView<Files>>,
}

impl FileBrowser {
    pub fn new() -> Result<FileBrowser, Box<Error>> {
        let cwd = std::env::current_dir().unwrap();
        let mut miller = MillerColumns::new();

        let lists: Result<Vec<ListView<Files>>, Box<Error>> = cwd
            .ancestors()
            .map(|path| Ok(ListView::new(Files::new_from_path(path)?)))
            .collect();
        let mut lists = lists?;
        lists.reverse();

        for widget in lists {
            miller.push_widget(widget);
        }

        let mut file_browser = FileBrowser { columns: miller };

        file_browser.update_preview();
        file_browser.fix_selection();

        Ok(file_browser)
    }

    pub fn enter_dir(&mut self) {
        let fileview = self.columns.get_main_widget();

        let path = fileview.selected_file().path();
        match Files::new_from_path(&path) {
            Ok(files) => {
                std::env::set_current_dir(path).unwrap();
                let view = ListView::new(files);
                self.columns.push_widget(view);
                self.update_preview();
            }
            Err(_) => {
                //self.show_status(&format!("Can't open this path: {}", err));

                let status = std::process::Command::new("xdg-open")
                    .args(dbg!(path.file_name()))
                    .status();
                match status {
                    Ok(status) => {
                        self.show_status(&format!("\"{}\" exited with {}", "xdg-open", status))
                    }
                    Err(err) => {
                        self.show_status(&format!("Can't run this \"{}\": {}", "xdg-open", err))
                    }
                }
            }
        };
    }

    pub fn go_back(&mut self) {
        if self.columns.get_left_widget().is_none() {
            return;
        }
        let fileview = self.columns.get_main_widget();
        let path = fileview.selected_file().grand_parent().unwrap();
        std::env::set_current_dir(path).unwrap();
        self.columns.pop_widget();

        // Make sure there's a directory on the left unless it's /
        if self.columns.get_left_widget().is_none() {
            let file = self.columns.get_main_widget().clone_selected_file();
            if let Some(grand_parent) = file.grand_parent() {
                let mut left_view = ListView::new(Files::new_from_path(&grand_parent).unwrap());
                left_view.select_file(&file);
                self.columns.prepend_widget(left_view);
            }
        }
    }

    pub fn update_preview(&mut self) {
        let file = self.columns.get_main_widget().selected_file().clone();
        let preview = &mut self.columns.preview;
        preview.set_file(&file);
    }

    pub fn fix_selection(&mut self) {
        let cwd = self.cwd();
        self.columns.get_left_widget_mut()
            .map(|w|
                 w.select_file(&cwd));
    }

    pub fn cwd(&self) -> File {
        self.columns.get_main_widget().content.directory.clone()
    }

    pub fn quit_with_dir(&self) {
        let cwd = self.cwd().path;

        let mut filepath = dirs_2::home_dir().unwrap();
        filepath.push(".hunter_cwd");

        let mut file = std::fs::File::create(filepath).unwrap();
        file.write(cwd.to_str().unwrap().as_bytes()).unwrap();
        panic!("Quitting!");
    }
}

impl Widget for FileBrowser {
    fn get_size(&self) -> &Size {
        &self.columns.get_size()
    }
    fn get_position(&self) -> &Position {
        &self.columns.get_position()
    }
    fn set_size(&mut self, size: Size) {
        self.columns.set_size(size);
    }
    fn set_position(&mut self, position: Position) {
        self.columns.set_position(position);
    }
    fn get_coordinates(&self) -> &Coordinates {
        &self.columns.coordinates
    }
    fn set_coordinates(&mut self, coordinates: &Coordinates) {
        self.columns.coordinates = coordinates.clone();
    }
    fn render_header(&self) -> String {
        "".to_string()
    }
    fn refresh(&mut self) {
        self.columns.refresh();
    }

    fn get_drawlist(&self) -> String {
        if self.columns.get_left_widget().is_none() {
            self.columns.get_clearlist() + &self.columns.get_drawlist()
        } else {
            self.columns.get_drawlist()
        }
    }

    fn on_key(&mut self, key: Key) {
        match key {
            Key::Char('Q') => self.quit_with_dir(),
            Key::Right => self.enter_dir(),
            Key::Left => self.go_back(),
            _ => self.columns.get_main_widget_mut().on_key(key),
        }
        self.update_preview();
    }
}
