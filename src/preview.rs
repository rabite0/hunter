use std::io::Write;
use std::sync::Mutex;

use crate::coordinates::{Coordinates, Position, Size};
use crate::files::{File, Files, Kind};
use crate::listview::ListView;
use crate::textview::TextView;
use crate::widget::Widget;


pub struct InstanceCounter(Mutex<usize>);
impl PartialEq for InstanceCounter {
    fn eq(&self, other: &Self) -> bool {
        let instance = self.0.lock().unwrap();
        let other = other.0.lock().unwrap();
        *instance == *other
    }
}


#[derive(PartialEq)]
pub struct Previewer {
    pub file: Option<File>,
    pub buffer: String,
    pub coordinates: Coordinates,
    pub instances: InstanceCounter
}

impl Previewer {
    pub fn new() -> Previewer {
        Previewer {
            file: None,
            buffer: String::new(),
            coordinates: Coordinates::new(),
            instances: InstanceCounter(Mutex::new(0))
        }
    }
    pub fn set_file(&mut self, file: &File) {
        //return;
        let mut instance = self.instances.0.try_lock().unwrap();
        if *instance > 2 { return }
        *instance = *instance + 1;
        let coordinates = self.coordinates.clone();
        let file = file.clone();
        let redraw = crate::term::reset() + &self.get_redraw_empty_list(0);


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
                        file_list.animate_slide_up();
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
                        textview.animate_slide_up();
                    } else {
                        let output =
                            std::process::Command::new("scope.sh").arg(&file.name)
                            .arg("10".to_string())
                            .arg("10".to_string())
                            .arg("".to_string())
                            .arg("false".to_string())
                            .output().unwrap();



                        if output.status.code().unwrap() == 0 {
                            let output = std::str::from_utf8(&output.stdout)
                                .unwrap()
                                .to_string();
                            let mut textview = TextView {
                                lines: output.lines().map(|s| s.to_string()).collect(),
                                buffer: String::new(),
                                coordinates: Coordinates::new() };
                            textview.set_coordinates(&coordinates);
                            textview.refresh();
                            textview.animate_slide_up();

                        } else
                        {
                            write!(std::io::stdout(), "{}", redraw).unwrap();
                        }
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
        let file = self.file.clone();
        if let Some(file) = file {
            self.set_file(&file);
        }
    }
    fn get_drawlist(&self) -> String {
        self.buffer.clone()
    }
}
