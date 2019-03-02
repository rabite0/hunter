use std::io::Write;

use termion::event::Key;

use crate::coordinates::{Coordinates};
use crate::widget::{Widget, WidgetCore};
use crate::fail::{HResult, HError, ErrorLog};
use crate::term::ScreenExt;

#[derive(Debug)]
pub struct MiniBuffer {
    core: WidgetCore,
    query: String,
    input: String,
    position: usize,
    history: Vec<String>,
    history_pos: Option<usize>,
    completions: Vec<String>,
    last_completion: Option<String>
}

impl MiniBuffer {
    pub fn new(core: &WidgetCore) -> MiniBuffer {
        let xsize = crate::term::xsize();
        let ysize = crate::term::ysize();
        let coordinates = Coordinates::new_at(xsize, 1, 1, ysize);
        let mut core = core.clone();
        core.coordinates = coordinates;
        MiniBuffer {
            core: core,
            query: String::new(),
            input: String::new(),
            position: 0,
            history: vec![],
            history_pos: None,
            completions: vec![],
            last_completion: None
        }
    }

    pub fn query(&mut self, query: &str) -> HResult<String> {
        self.query = query.to_string();
        self.input.clear();
        self.position = 0;
        self.history_pos = None;
        self.completions.clear();
        self.last_completion = None;

        self.get_core()?.screen.lock()?.cursor_hide();

        self.popup()?;

        Ok(self.input.clone())
    }

    pub fn complete(&mut self) -> HResult<()> {
        if !self.input.ends_with(" ") {
            if !self.completions.is_empty() {
                self.cycle_completions()?;
                return Ok(());
            }

            let part = self.input
                .rsplitn(2, " ")
                .take(1)
                .map(|s| s.to_string())
                .collect::<String>();
            let completions = find_files(part.clone());

            if let Ok(mut completions) = completions {
                let completion = completions.pop()?;

                self.input
                    = self.input[..self.input.len() - part.len()].to_string();
                self.input.push_str(&completion);
                self.position += &completion.len() - part.len();

                self.last_completion = Some(completion);
                self.completions = completions;
            } else {
                let completions = find_bins(&part);

                if let Ok(mut completions) = completions {
                    let completion = completions.pop()?;

                    self.input = self.input[..self.input.len()
                                            - part.len()].to_string();
                    self.input.push_str(&completion);
                    self.position += &completion.len() - part.len();

                    self.last_completion = Some(completion);
                    self.completions = completions;
                }
            }
        } else {
            self.input += "$s";
            self.position += 2
        }
        Ok(())
    }

    pub fn cycle_completions(&mut self) -> HResult<()> {
        let last_comp = self.last_completion.as_ref()?;
        let last_len = last_comp.len();

        self.input = self.input.trim_end_matches(last_comp).to_string();
        self.position -= last_len;

        let next_comp = self.completions.pop()?;
        self.input.push_str(&next_comp);
        self.position += next_comp.len();
        self.last_completion = Some(next_comp);
        Ok(())
    }

    pub fn history_up(&mut self) -> HResult<()> {
        if self.history_pos == Some(0) { self.history_pos = None; }
        if self.history.len() == 0 { return Err(HError::NoHistoryError); }

        if let Some(history_pos) = self.history_pos {
            let historic = self.history[history_pos - 1].clone();

            self.input = historic;
            self.position = self.input.len();
            self.history_pos = Some(history_pos - 1);
        } else {
            let historic = self.history[self.history.len() - 1].clone();

            self.input = historic;
            self.position = self.input.len();
            self.history_pos = Some(self.history.len() - 1);
        }
        Ok(())
    }

    pub fn history_down(&mut self) -> HResult<()> {
        let hist_len = self.history.len();

        if hist_len == 0 { return Err(HError::NoHistoryError); }
        if self.history_pos == Some(hist_len) ||
           self.history_pos == None
            { self.history_pos = Some(0); }

        if let Some(history_pos) = self.history_pos {
            let historic = self.history[history_pos].clone();

            self.input = historic;
            self.position = self.input.len();
            self.history_pos = Some(history_pos + 1);
        } else {
            let historic = self.history[0].clone();

            self.input = historic;
            self.position = self.input.len();
            self.history_pos = Some(1);
        }
        Ok(())
    }

    pub fn clear_line(&mut self) -> HResult<()> {
        self.input.clear();
        self.position = 0;
        Ok(())
    }

    pub fn delete_word(&mut self) -> HResult<()> {
        let old_input_len = self.input.len();
        let (before_cursor, after_cursor) = self.input.split_at(self.position);

        let no_trim_len = before_cursor.len();
        let before_cursor = before_cursor.trim_end();

        if no_trim_len != before_cursor.len() {
            self.position -= no_trim_len - before_cursor.len();
            self.input = before_cursor.to_string() + after_cursor;
            return Ok(());
        }

        if before_cursor.ends_with("/") {
            let mut new_input = before_cursor.to_string();
            new_input.pop();
            self.input = new_input + after_cursor;
            self.position -= 1;
            return Ok(());
        };

        let dir_boundary = before_cursor.rfind("/");
        let word_boundary = before_cursor.rfind(" ");
        let boundaries = (dir_boundary, word_boundary);

        let new_input = match boundaries {
            (Some(dir_boundary), Some(word_boundary)) => {
                if dir_boundary > word_boundary {
                    before_cursor
                        .split_at(dir_boundary).0
                        .to_string() + "/"
                } else {
                    before_cursor
                        .split_at(word_boundary).0
                        .to_string() + " "
                }
            }
            (Some(dir_boundary), None) => {
                before_cursor
                    .split_at(dir_boundary).0
                    .to_string() + "/"
            }
            (None, Some(word_boundary)) => {
                before_cursor
                    .split_at(word_boundary).0
                    .to_string() + " "
            }
            (None, None) => "".to_string()
        } + after_cursor;

        let len_difference = old_input_len - new_input.len();
        self.position -= len_difference;

        self.input = new_input;

        Ok(())
    }

    pub fn input_finnished(&mut self) -> HResult<()> {
        return Err(HError::PopupFinnished)
    }
}

pub fn find_bins(comp_name: &str) -> HResult<Vec<String>> {
    let paths = std::env::var_os("PATH")?
        .to_string_lossy()
        .split(":")
        .map(|s| s.to_string())
        .collect::<Vec<String>>();

    let completions = paths.iter().map(|path| {
        if let Ok(read_dir) = std::fs::read_dir(path) {
            read_dir.map(|file| {
                let file = file.unwrap();
                let name = file.file_name().into_string().unwrap();
                if name.starts_with(comp_name) {
                    Ok(name)
                } else {
                    Err(HError::NoCompletionsError)
                }
            }).collect::<Vec<HResult<String>>>()
        } else { vec![Err(HError::NoCompletionsError)] }
    }).flatten()
        .filter(|result| result.is_ok())
        .map(|result| result.unwrap())
        .collect::<Vec<String>>();
    if completions.is_empty() { return Err(HError::NoCompletionsError); }
    Ok(completions)
}

pub fn find_files(comp_name: String) -> HResult<Vec<String>> {
    let mut path = std::env::current_dir().unwrap();
    let comp_path = std::path::PathBuf::from(&comp_name);
    path.push(&comp_path);

    let filename_part = path.file_name()?.to_string_lossy().to_string();

    let dir = if path.is_dir() { &path } else { path.parent().unwrap() };
    let dir = std::path::PathBuf::from(dir);

    let prefix = comp_name.trim_end_matches(&filename_part);

    let reader = std::fs::read_dir(&dir)?;

    let completions = reader.map(|file| {
        let file = file?;
        let name = file.file_name().into_string().unwrap();
        if name.starts_with(&filename_part) {
            if file.file_type().unwrap().is_dir() {
                Ok(format!("{}{}/", prefix, name))
            } else {
                Ok(format!("{}{}", prefix, name))
            }
        } else {
            Err(HError::NoCompletionsError)
        }
    }).filter(|res| res.is_ok() )
      .map(|res| res.unwrap() )
      .collect::<Vec<String>>();
    if completions.is_empty() { return Err(HError::NoCompletionsError); }
    Ok(completions)
}

impl Widget for MiniBuffer {
    fn get_core(&self) -> HResult<&WidgetCore> {
        Ok(&self.core)
    }
    fn get_core_mut(&mut self) -> HResult<&mut WidgetCore> {
        Ok(&mut self.core)
    }
    fn refresh(&mut self) -> HResult<()> {
        Ok(())
    }

    fn get_drawlist(&self) -> HResult<String> {
        let (xpos, ypos) = self.get_coordinates()?.u16position();
        Ok(format!("{}{}{}: {}",
                crate::term::goto_xy(xpos, ypos),
                termion::clear::CurrentLine,
                self.query,
                self.input))
    }

    fn on_key(&mut self, key: Key) -> HResult<()> {
        match key {
            Key::Esc | Key::Ctrl('c') => { self.input_finnished()?; },
            Key::Char('\n') => {
                if self.input != "" {
                    self.history.push(self.input.clone());
                }
                self.input_finnished()?;
            }
            Key::Char('\t') => {
                self.complete()?;
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
            Key::Up | Key::Ctrl('p') | Key::Alt('p') => {
                self.history_up()?;
            }
            Key::Down | Key::Ctrl('n') | Key::Alt('n') => {
                self.history_down()?;
            }
            Key::Ctrl('u') => { self.clear_line()?; },
            Key::Ctrl('h') => { self.delete_word()?; },
            Key::Ctrl('a') => { self.position = 0 },
            Key::Ctrl('e') => { self.position = self.input.len(); },
            Key::Char(key) => {
                self.input.insert(self.position, key);
                self.position += 1;
            }
            _ => {  }
        }
        Ok(())
    }

    fn after_draw(&self) -> HResult<()> {
        let cursor_pos = self.query.len() +
                         ": ".len() +
                         self.position;

        let mut screen = self.get_core()?.screen.lock()?;
        let ysize = screen.ysize()?;

        screen.goto_xy(cursor_pos, ysize).log();
        screen.cursor_show().log();

        Ok(())
    }
}
