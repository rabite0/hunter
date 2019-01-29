use termion::event::{Key,Event};


use crate::widget::Widget;
use crate::files::Files;
//use crate::hbox::HBox;
use crate::listview::ListView;
use crate::coordinates::{Coordinates, Size,Position};
use crate::files::File;
use crate::miller_columns::MillerColumns;

pub struct FileBrowser {
    pub columns: MillerColumns<ListView<Files>>,
}

impl FileBrowser {
    pub fn set_left_directory(&mut self) {
        
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
    fn render_header(&self) -> String {
        "".to_string()
    }
    fn refresh(&mut self) {
        self.columns.refresh();
    }
    
    fn get_drawlist(&self) -> String {
        self.columns.get_drawlist()
        // let view_count = self.columns.widgets.len();

        // if view_count < 2 {
        //     // TODO: Special handling
        // } else if view_count < 1 {
        //     // etc.
        // }
        
        // self.views
        //     .iter()
        //     .skip(view_count - 2)
        //     .map(|view| {
        //         eprintln!("{}", view.get_drawlist());
        //         view.get_drawlist()
        //     }).collect()
    }
            

    fn on_key(&mut self, key: Key) {
        match key {
            Key::Right => {
                match self.columns.get_main_widget() {
                    Some(widget) => {
                        let path = widget.selected_file().path();
                        let files = Files::new_from_path(&path).unwrap();
                        let view = ListView::new(files);
                        self.columns.widgets.push(view);
                        self.refresh();
                    }, None => { }
                }
            },
            Key::Left => {
                if self.columns.get_left_widget().is_some() {
                    self.columns.widgets.pop();
                }
            }
           
            _ => {
                match self.columns.get_main_widget_mut() {
                    Some(widget) => {
                        widget.on_key(key);
                        self.set_left_directory();
                        self.refresh();
                    }, None => { self.refresh(); }
                    
                }
                //_ => { self.bad(Event::Key(key)); }
            }
        }
    }
}

