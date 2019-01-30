use crate::widget::Widget;
use crate::coordinates::{Coordinates, Size, Position};
use crate::files::{File, Files, Kind};
use crate::listview::ListView;
use crate::textview::TextView;

pub struct Previewer {
    pub file: Option<File>,
    pub buffer: String,
    pub coordinates: Coordinates,
}

impl Previewer {
    pub fn new() -> Previewer {
        Previewer {
            file: None,
            buffer: String::new(),
            coordinates: Coordinates::new(),
        }
    }
    pub fn set_file(&mut self, file: &File) {
        self.file = Some(file.clone());
        self.refresh();
    }
}

impl Widget for Previewer {
    fn render(&self) -> Vec<String> {
        vec![]
    }
    fn get_size(&self) -> &Size {
        &self.coordinates.size
    }
    fn set_size(&mut self, size: Size) {
        self.coordinates.size = size;
    }
    fn get_position(&self) -> &Position {
        &self.coordinates.position
    }
    fn set_position(&mut self, pos: Position) {
        self.coordinates.position = pos;
    }
    fn get_coordinates(&self) -> &Coordinates {
        &self.coordinates
    }
    fn set_coordinates(&mut self, coordinates: &Coordinates) {
        if self.coordinates == *coordinates { return }
        self.coordinates = coordinates.clone();
        self.refresh();
    }
    fn render_header(&self) -> String { "".to_string() }
    fn refresh(&mut self) {
        if self.file == None { return }

        let file = self.file.as_ref().unwrap();
        self.buffer =
            match &file.kind {
                Kind::Directory => {
                    match Files::new_from_path(&file.path) {
                        Ok(files) => {
                            let len = files.len();
                            let mut file_list = ListView::new(files);
                            file_list.set_size(self.coordinates.size.clone());
                            file_list.set_position(self.coordinates.position.clone());
                            file_list.refresh();
                            file_list.get_drawlist()
                                + &file_list.get_redraw_empty_list(len)
                        }, Err(err) => {
                            self.show_status(&format!("Can't preview because: {}", err));
                            self.get_clearlist()
                        }
                    }
                },
                _ => {
                    if file.get_mime() == Some("text".to_string()) {
                        let mut textview = TextView::new_from_file(&file);
                        textview.set_size(self.coordinates.size.clone());
                        textview.set_position(self.coordinates.position.clone());
                        textview.refresh();
                        let len = textview.lines.len();
                        textview.get_drawlist() + &textview.get_redraw_empty_list(len-1)
                    } else { self.get_clearlist() }
                }
            };
    }
    fn get_drawlist(&self) -> String {
        self.buffer.clone()
    }
}
