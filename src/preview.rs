use std::io::Write;
use std::sync::Mutex;
use std::sync::Arc;

use crate::coordinates::{Coordinates};
use crate::files::{File, Files, Kind};
use crate::listview::ListView;
use crate::textview::TextView;
use crate::widget::Widget;
//use crate::async_widget::AsyncPlug;
use crate::async_widget::AsyncPlug2;

lazy_static! {
    static ref PIDS: Arc<Mutex<Vec<u32>>> = { Arc::new(Mutex::new(vec![])) };
    static ref CURFILE: Arc<Mutex<Option<File>>> = { Arc::new(Mutex::new(None)) };
}

fn kill_procs() {
    let mut pids = PIDS.lock().unwrap();
    for pid in &*pids {
        unsafe { libc::kill(*pid as i32, 9); }
    }
    pids.clear();
}

fn is_current(file: &File) -> bool {
    match CURFILE.lock().unwrap().as_ref() {
        Some(curfile) => curfile == file,
        None => true
    }
}

enum WillBe<T> {
    Is(T),
    Becoming,
    Wont(Box<std::error::Error>)
}

#[derive(PartialEq)]
pub struct AsyncPreviewer {
    pub file: Option<File>,
    pub buffer: String,
    pub coordinates: Coordinates,
    pub async_plug: AsyncPlug2<Box<dyn Widget + Send + 'static>>
}

impl AsyncPreviewer {
    pub fn new() -> AsyncPreviewer {
        let closure = Box::new(|| {
            Box::new(crate::textview::TextView {
                    lines: vec![],
                    buffer: "".to_string(),
                    coordinates: Coordinates::new()
            }) as Box<dyn Widget + Send + 'static>
        });
        
        AsyncPreviewer {
            file: None,
            buffer: String::new(),
            coordinates: Coordinates::new(),
            async_plug: AsyncPlug2::new_from_closure(closure),
        }
    }
    pub fn set_file(&mut self, file: &File) {
        let coordinates = self.coordinates.clone();
        let file = file.clone();
        let redraw = crate::term::reset() + &self.get_redraw_empty_list(0);
        //let pids = PIDS.clone();
        //kill_procs();

        self.async_plug.replace_widget(Box::new(move || {
            kill_procs();
            let mut bufout = std::io::BufWriter::new(std::io::stdout());
            match &file.kind {
                Kind::Directory => match Files::new_from_path(&file.path) {
                    Ok(files) => {
                        //if !is_current(&file) { return }
                        let len = files.len();
                        //if len == 0 { return };
                        let mut file_list = ListView::new(files);
                        file_list.set_coordinates(&coordinates);
                        file_list.refresh();
                        //if !is_current(&file) { return }
                        file_list.animate_slide_up();
                        return Box::new(file_list)

                    }
                    Err(err) => {
                        write!(bufout, "{}", redraw).unwrap();
                        let textview = crate::textview::TextView {
                            lines: vec![],
                            buffer: "".to_string(),
                            coordinates: Coordinates::new(),
                        };
                        return Box::new(textview)
                    },
                }
                _ => {
                    if file.get_mime() == Some("text".to_string()) {
                        let lines = coordinates.ysize() as usize;
                        let mut textview
                            = TextView::new_from_file_limit_lines(&file,
                                                                  lines);
                        //if !is_current(&file) { return }
                        textview.set_coordinates(&coordinates);
                        textview.refresh();
                        //if !is_current(&file) { return }
                        textview.animate_slide_up();
                        return Box::new(textview)
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
                        PIDS.lock().unwrap().push(pid);

                        //if !is_current(&file) { return }

                        let output = process.wait_with_output();
                        match output {
                            Ok(output) => {
                                let status = output.status.code();
                                match status {
                                    Some(status) => {
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
                                            return Box::new(textview)
                                        }
                                    }, None => {}
                                }
                            }, Err(_) => {}
                        }

                        write!(bufout, "{}", redraw).unwrap();
                        //std::io::stdout().flush().unwrap();
                        let textview = crate::textview::TextView {
                            lines: vec![],
                            buffer: "".to_string(),
                            coordinates: Coordinates::new(),
                        };
                        return Box::new(textview)
                    }
                }
            }}))
    }
}



impl Widget for AsyncPreviewer {
    fn get_coordinates(&self) -> &Coordinates {
        &self.coordinates
    }
    fn set_coordinates(&mut self, coordinates: &Coordinates) {
        if self.coordinates == *coordinates {
            return;
        }
        self.coordinates = coordinates.clone();
        self.async_plug.set_coordinates(&coordinates.clone());
        self.async_plug.refresh();
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
        self.async_plug.get_drawlist();
        "".to_string()
    }
}

impl<T> Widget for Box<T> where T: Widget + ?Sized {
    fn get_coordinates(&self) -> &Coordinates {
        (**self).get_coordinates()
    }
    fn set_coordinates(&mut self, coordinates: &Coordinates) {
        if (**self).get_coordinates() == coordinates {
            return;
        }
        (**self).set_coordinates(&coordinates);
        (**self).refresh();
    }
    fn render_header(&self) -> String {
        (**self).render_header()
    }
    fn refresh(&mut self) {
        (**self).refresh()
    }
    fn get_drawlist(&self) -> String {
        (**self).get_drawlist()
    }
}
