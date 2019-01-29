use termion::event::{Key};


use crate::widget::Widget;
use crate::files::Files;
//use crate::hbox::HBox;
use crate::listview::ListView;
use crate::coordinates::{Size,Position};
use crate::preview::Previewer;
use crate::miller_columns::MillerColumns;

pub struct FileBrowser {
    pub columns: MillerColumns<ListView<Files>>,
}

impl FileBrowser {
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
        let file
            = self.columns.get_main_widget().as_ref().unwrap().selected_file().clone();
        let (_, _, preview_coordinates) = self.columns.calculate_coordinates();

        match &mut self.columns.preview {
            Some(preview) => preview.set_file(&file),
            None => {
                let preview = Previewer::new(&file, &preview_coordinates);
                self.columns.preview = Some(preview);
            }
        }
        self.columns.refresh();
    }
    
    fn get_drawlist(&self) -> String {
        self.columns.get_drawlist()
    }
            

    fn on_key(&mut self, key: Key) {
        match key {
            Key::Right => {
                match self.columns.get_main_widget() {
                    Some(widget) => {
                        let path = widget.selected_file().path();
                        let files = Files::new_from_path(&path).unwrap();
                        let view = ListView::new(files);
                        let selected_file = view.selected_file();
                        self.columns.set_preview(selected_file);
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
                        self.refresh();
                    }, None => { self.refresh(); }
                    
                }
                //_ => { self.bad(Event::Key(key)); }
            }
        }
    }
}

