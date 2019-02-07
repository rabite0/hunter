use std::io::Write;
use std::sync::Mutex;
use std::sync::Arc;

use crate::coordinates::{Coordinates, Position, Size};
use crate::files::{File, Files, Kind};
use crate::listview::ListView;
use crate::textview::TextView;
use crate::widget::Widget;


lazy_static! {
    static ref PIDS: Arc<Mutex<Vec<i32>>> = { Arc::new(Mutex::new(vec![])) };
    static ref CURFILE: Arc<Mutex<Option<File>>> = { Arc::new(Mutex::new(None)) };
}

fn kill_procs() {
    let mut pids = PIDS.lock().unwrap();
    for pid in &*pids {
        unsafe { libc::kill(*pid, 9); }
    }
    pids.clear();
}

fn is_current(file: &File) -> bool {
    CURFILE.lock().unwrap().as_ref().unwrap() == file
}



#[derive(PartialEq)]
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
        let coordinates = self.coordinates.clone();
        let file = file.clone();
        let redraw = crate::term::reset() + &self.get_redraw_empty_list(0);

        *CURFILE.lock().unwrap() = Some(file.clone());

        std::thread::spawn(move || {
            kill_procs();
            match &file.kind {
                Kind::Directory => match Files::new_from_path(&file.path) {
                    Ok(files) => {
                        if !is_current(&file) { return }
                        let len = files.len();
                        if len == 0 { return };
                        let mut file_list = ListView::new(files);
                        file_list.set_coordinates(&coordinates);
                        file_list.refresh();
                        if !is_current(&file) { return }
                        file_list.animate_slide_up();

                    }
                    Err(err) => {
                        crate::window::show_status(&format!("Can't preview because: {}", err));
                    }

                },
                _ => {
                    if file.get_mime() == Some("text".to_string()) {
                        let mut textview = TextView::new_from_file(&file);
                        if !is_current(&file) { return }
                        textview.set_coordinates(&coordinates);
                        textview.refresh();
                        if !is_current(&file) { return }
                        textview.animate_slide_up();
                    } else {
                        let process =
                            std::process::Command::new("scope.sh")
                            .arg(&file.name)
                            .arg("10".to_string())
                            .arg("10".to_string())
                            .arg("".to_string())
                            .arg("false".to_string())
                            .stdin(std::process::Stdio::null())
                            .stdout(std::process::Stdio::piped())
                            .stderr(std::process::Stdio::null())
                            .spawn().unwrap();

                        let pid = process.id();
                        PIDS.lock().unwrap().push(pid as i32);

                        if !is_current(&file) { return }

                        let output = process.wait_with_output().unwrap();

                        let status = output.status.code().unwrap();

                        if status == 0 || status == 5 && is_current(&file) {
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
                        PIDS.lock().unwrap().pop();
                    }
                }
            }
        });
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
