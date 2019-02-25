use termion::event::Key;
use termion::input::TermRead;

use std::io::{stdin, stdout, Write};

use crate::coordinates::{Coordinates};
use crate::widget::Widget;
use crate::window::{send_event, Events};
use crate::fail::HResult;

pub struct MiniBuffer {
    coordinates: Coordinates,
    query: String,
    input: String,
    done: bool,
    position: usize,
    history: Vec<String>
}

impl MiniBuffer {
    pub fn new() -> MiniBuffer {
        let xsize = crate::term::xsize();
        let ysize = crate::term::ysize();
        let coordinates = Coordinates::new_at(xsize, 1, 1, ysize);
        MiniBuffer {
            coordinates: coordinates,
            query: String::new(),
            input: String::new(),
            done: false,
            position: 0,
            history: vec![]
        }
    }

    pub fn query(&mut self, query: &str) -> HResult<String> {
        self.query = query.to_string();
        self.input.clear();
        self.done = false;
        self.position = 0;

        send_event(Events::ExclusiveInput(true))?;

        self.draw()?;
        write!(stdout(), "{}{}",
               termion::cursor::Show,
               termion::cursor::Save)?;
        stdout().flush()?;


        for event in stdin().events() {
            let event = event?;
            self.on_event(event);
            if self.done {
                break
            }
            self.draw()?;

            write!(stdout(), "{}", termion::cursor::Restore)?;
            if self.position != 0 {
                write!(stdout(),
                       "{}",
                       termion::cursor::Right(self.position as u16))?;
            }
            stdout().flush()?;
        }

        self.done = false;

        send_event(Events::ExclusiveInput(false))?;

        Ok(self.input.clone())
    }
}

pub fn find_bins(comp_name: &str) -> Vec<String> {
    let paths = std::env::var_os("PATH").unwrap()
        .to_string_lossy()
        .split(":")
        .map(|s| s.to_string())
        .collect::<Vec<String>>();

    paths.iter().map(|path| {
        std::fs::read_dir(path).unwrap().flat_map(|file| {
            let file = file.unwrap();
            let name = file.file_name().into_string().unwrap();
            if name.starts_with(comp_name) {
                Some(name)
            } else {
                None
            }
        }).collect::<Vec<String>>()
    }).flatten().collect::<Vec<String>>()
}

pub fn find_files(mut comp_name: String) -> Vec<String> {
    let mut path = std::path::PathBuf::from(&comp_name);

    let dir = if comp_name.starts_with("/") {
        comp_name = path.file_name().unwrap().to_string_lossy().to_string();
        path.pop();
        path.to_string_lossy().to_string()
    } else {
        std::env::current_dir().unwrap().to_string_lossy().to_string()
    };

    let reader = std::fs::read_dir(dir.clone());
    if reader.is_err() { return vec![]  }
    let reader = reader.unwrap();

    reader.flat_map(|file| {
        let file = file.unwrap();
        let name = file.file_name().into_string().unwrap();
        if name.starts_with(&comp_name) {
            if file.file_type().unwrap().is_dir() {
                Some(format!("{}/{}/", &dir, name))
            } else {
                Some(format!("/{}/", name))
            }
        } else {
            None
        }
    }).collect::<Vec<String>>()
}

impl Widget for MiniBuffer {
    fn get_coordinates(&self) -> &Coordinates {
        &self.coordinates
    }
    fn set_coordinates(&mut self, coordinates: &Coordinates) {
        self.coordinates = coordinates.clone();
        self.refresh();
    }
    fn render_header(&self) -> String {
        "".to_string()
    }
    fn refresh(&mut self) {
    }

    fn get_drawlist(&self) -> String {
        let (xpos, ypos) = self.get_coordinates().u16position();
        format!("{}{}{}: {}",
                crate::term::goto_xy(xpos, ypos),
                termion::clear::CurrentLine,
                self.query,
                self.input)
    }

    fn on_key(&mut self, key: Key) {
        match key {
            Key::Esc | Key::Ctrl('c') => { self.input.clear(); self.done = true; },
            Key::Char('\n') => {
                if self.input != "" {
                    self.history.push(self.input.clone());
                }
                self.done = true;
            }
            Key::Char('\t') => {
                if !self.input.ends_with(" ") {
                    let part = self.input.rsplitn(2, " ").take(1)
                        .map(|s| s.to_string()).collect::<String>();
                    let completions = find_files(part.clone());
                    if !completions.is_empty() {
                        self.input
                            = self.input[..self.input.len() - part.len()].to_string();
                        self.input.push_str(&completions[0]);
                        self.position += &completions[0].len() - part.len();
                    } else {
                        let completions = find_bins(&part);
                        if !completions.is_empty() {
                            self.input = self.input[..self.input.len()
                                                    - part.len()].to_string();
                            self.input.push_str(&completions[0]);
                            self.position += &completions[0].len() - part.len();
                        }
                    }
                } else {
                    self.input += "$s";
                    self.position += 2
                }
            }
            Key::Backspace => {
                if self.position != 0 {
                    self.input.remove(self.position - 1);
                    self.position -= 1;
                }
            }
            Key::Delete | Key::Ctrl('d') => {
                if self.position != self.input.len() {
                    self.input.remove(self.position);
                }
            }
            Key::Left | Key::Ctrl('b') => {
                if self.position != 0 {
                    self.position -= 1;
                }
            }
            Key::Right | Key::Ctrl('f') => {
                if self.position != self.input.len() {
                    self.position += 1;
                }
            }
            Key::Ctrl('a') => { self.position = 0 },
            Key::Ctrl('e') => { self.position = self.input.len(); },
            Key::Char(key) => {
                self.input.insert(self.position, key);
                self.position += 1;
            }
            _ => {}
        }
    }
}
