use rayon as rayon;

use std::io::{stdout, Write};
use std::sync::atomic::AtomicUsize;
use std::sync::Mutex;

use crate::coordinates::{Coordinates, Position, Size};
use crate::files::{File, Files, Kind};
use crate::listview::ListView;
use crate::textview::TextView;
use crate::widget::Widget;

pub struct Previewer {
    pub file: Option<File>,
    pub buffer: String,
    pub coordinates: Coordinates,
    pub instances: Mutex<usize>
}

impl Previewer {
    pub fn new() -> Previewer {
        Previewer {
            file: None,
            buffer: String::new(),
            coordinates: Coordinates::new(),
            instances: From::from(0)
        }
    }
    pub fn set_file(&mut self, file: &File) {
        //return;
        let mut instance = self.instances.try_lock().unwrap();
        if *instance > 2 { return }
        *instance = *instance + 1;
        let coordinates = self.coordinates.clone();
        let file = file.clone();

        
        
        //self.threads.install(|| {
        std::thread::spawn(move || {
            match &file.kind {
                Kind::Directory => match Files::new_from_path(&file.path) {
                    Ok(files) => {
                        let len = files.len();
                        if len == 0 { return };
                        let mut file_list = ListView::new(files);
                        file_list.set_coordinates(&coordinates);
                        file_list.refresh();
                        write!(std::io::stdout(),
                               "{}{}",
                               &file_list.get_drawlist(),
                               &file_list.get_redraw_empty_list(len)).unwrap();
                        
                    }
                    Err(err) => {
                        crate::window::show_status(&format!("Can't preview because: {}", err));
                    }
                    
                },
                _ => {
                    if file.get_mime() == Some("text".to_string()) {
                        let mut textview = TextView::new_from_file(&file);
                        textview.set_coordinates(&coordinates);
                        textview.refresh();
                        let len = textview.lines.len();
                        let output = textview.get_drawlist()
                            + &textview.get_redraw_empty_list(len - 1);
                        write!(std::io::stdout(), "{}", output).unwrap();
                    }
                    
                }
            }
        });
        *instance = *instance - 1;
    }
}

impl Widget for Previewer {
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
        if self.coordinates == *coordinates {
            return;
        }
        self.coordinates = coordinates.clone();
        self.refresh();
    }
    fn render_header(&self) -> String {
        "".to_string()
    }
    fn refresh(&mut self) {
        if self.file == None {
            return;
        }

        
    }
    fn get_drawlist(&self) -> String {
        self.buffer.clone()
    }
}
