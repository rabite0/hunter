use termion::event::{Key};

use std::io::Write;

use crate::widget::Widget;
use crate::files::Files;
//use crate::hbox::HBox;
use crate::listview::ListView;
use crate::coordinates::{Size,Position,Coordinates};
use crate::preview::Previewer;
use crate::miller_columns::MillerColumns;

pub struct FileBrowser {
    pub columns: MillerColumns<ListView<Files>>,
}

impl FileBrowser {
    pub fn new() -> FileBrowser {
        let cwd = std::env::current_dir().unwrap();
        let mut miller = MillerColumns::new();

        let mut lists: Vec<_> = cwd.ancestors().map(|path| {
            ListView::new(Files::new_from_path(path).unwrap())
        }).collect();
        lists.reverse();

        for widget in lists {
            miller.push_widget(widget);
        }

        FileBrowser { columns: miller }
    }

    pub fn enter_dir(&mut self) {
        let fileview = self.columns.get_main_widget();

        let path = fileview.selected_file().path();
        match Files::new_from_path(&path) {
            Ok(files) => {
                let view = ListView::new(files);
                self.columns.push_widget(view);
                self.update_preview();
            },
            Err(err) => {
                self.show_status(&format!("Can't open this path: {}", err));
            }
        };
    }

    pub fn go_back(&mut self) {
        if self.columns.get_left_widget().is_none() { return }
        self.columns.pop_widget();

        // Make sure there's a directory on the left unless it's /
        if self.columns.get_left_widget().is_none() {
            let file = self.columns.get_main_widget().selected_file().clone();
            if let Some(grand_parent) = file.grand_parent() {
                let left_view
                    = ListView::new(Files::new_from_path(grand_parent).unwrap());
                self.columns.prepend_widget(left_view);
            }
        }

    }

    pub fn update_preview(&mut self) {
        let file = self.columns.get_main_widget().selected_file().clone();
        let preview = &mut self.columns.preview;
        preview.set_file(&file);
    }

    pub fn quit_with_dir(&self) {
        let selected_file = self.columns.get_main_widget().selected_file();
        let cwd = selected_file.path();
        let cwd = cwd.parent().unwrap();

        let mut filepath = std::env::home_dir().unwrap();
        filepath.push(".hunter_cwd");

        let mut file = std::fs::File::create(filepath).unwrap();
        file.write(cwd.to_str().unwrap().as_bytes());
        panic!("Quitting!");
    }
}


impl Widget for FileBrowser {
    fn render(&self) -> Vec<String> {
        vec![]
    }
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
            _ =>  self.columns.get_main_widget_mut().on_key(key)
        }
        self.update_preview();
    }
}
