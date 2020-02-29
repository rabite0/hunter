use termion::event::Key;

use std::collections::HashMap;
use std::ffi::{OsStr, OsString};

use crate::coordinates::{Coordinates};
use crate::widget::{Widget, WidgetCore};
use crate::fail::{HResult, HError, ErrorLog};
use crate::term::ScreenExt;

type HMap = HashMap<String, Vec<String>>;

#[derive(Debug)]
struct History {
    history: HMap,
    position: Option<usize>,
    loaded: bool
}

impl History {
    fn new() -> History {
        History {
            history: HashMap::new(),
            position: None,
            loaded: false
        }
    }

    fn load(&mut self) -> HResult<()> {
        if self.loaded { return Ok(()) }

        let hpath = crate::paths::history_path()?;
        let hf_content = std::fs::read_to_string(hpath)?;

        let history = hf_content.lines().fold(HashMap::new(), |mut hm: HMap, line| {
            let parts = line.splitn(2, ":").collect::<Vec<&str>>();
            if parts.len() == 2 {
                let (htype, hline) = (parts[0].to_string(), parts[1].to_string());

                match hm.get_mut(&htype) {
                    Some(hvec) => hvec.push(hline),
                    None => {
                        let hvec = vec![hline];
                        hm.insert(htype, hvec);
                    }
                };
            }
            hm
        });

        self.history = history;
        self.loaded = true;

        Ok(())
    }

    fn save(&self) -> HResult<()> {
        let hpath = crate::paths::history_path()?;

        let history = self.history.iter().map(|(htype, hlines)| {
            hlines.iter().map(|hline| format!("{}:{}\n", htype, hline))
                .collect::<String>()
        }).collect::<String>();

        std::fs::write(hpath, history)?;
        Ok(())
    }

    fn reset(&mut self) {
        self.position = None;
    }

    fn add(&mut self, htype: &str, input: &str) {
        self.load().ok();
        let history = match self.history.get_mut(htype) {
            Some(history) => history,
            None => {
                let hvec = Vec::new();
                self.history.insert(htype.to_string(), hvec);
                self.history.get_mut(htype).unwrap()
            }
        };
        history.push(input.to_string());
        self.save().log();
    }

    fn get_prev(&mut self, htype: &str) -> HResult<String> {
        self.load()?;
        let history = self.history.get(htype)?;
        let mut position = self.position;
        let hist_len = history.len();

        if position == Some(0) { position = None; }
        if hist_len == 0 { return Err(HError::NoHistoryError); }

        if let Some(position) = position {
            let historic = history[position - 1].clone();
            self.position = Some(position - 1);
            Ok(historic)
        } else {
            let historic = history[hist_len - 1].clone();
            self.position = Some(hist_len - 1);
            Ok(historic)
        }

    }

    fn get_next(&mut self, htype: &str) -> HResult<String> {
        self.load()?;
        let history = self.history.get(htype)?;
        let mut position = self.position;
        let hist_len = history.len();

        if hist_len == 0 { return Err(HError::NoHistoryError); }
        if position == Some(hist_len) ||
           position == None
            { position = Some(0); }

        if let Some(position) = position {
            let historic = history[position].clone();
            self.position = Some(position + 1);
            Ok(historic)
        } else {
            let historic = history[0].clone();
            self.position = Some(1);
            Ok(historic)
        }
    }
}

#[derive(Debug)]
pub struct MiniBuffer {
    core: WidgetCore,
    query: String,
    input: String,
    position: usize,
    history: History,
    completions: Vec<OsString>,
    last_completion: Option<String>,
    continuous: bool
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
            history: History::new(),
            completions: vec![],
            last_completion: None,
            continuous: false
        }
    }

    pub fn query(&mut self, query: &str, cont: bool) -> HResult<String> {
        self.continuous = cont;

        if !cont || self.query != query {
            self.query = query.to_string();

            self.clear();
        }

        self.core.screen()?.cursor_hide().log();

        match self.popup() {
            Err(HError::MiniBufferCancelledInput) => self.input_cancelled()?,
            err @ Err(HError::MiniBufferInputUpdated(_)) => err?,
            _ => {}
        };

        if self.input == "" {
            self.clear();
            self.input_empty()?; }

        Ok(self.input.clone())
    }

    pub fn clear(&mut self) {
        self.input.clear();
        self.position = 0;
        self.history.reset();
        self.completions.clear();
        self.last_completion = None;
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
            let completions = find_files(&part);

            if let Ok(mut completions) = completions {
                let completion = completions.pop()?;
                let completion = completion.to_string_lossy();

                self.input
                    = self.input[..self.input.len() - part.len()].to_string();
                self.input.push_str(&completion);
                self.position += &completion.len() - part.len();

                self.last_completion = Some(completion.to_string());
                self.completions = completions;
            } else {
                let completions = find_bins(&part);

                if let Ok(mut completions) = completions {
                    let completion = completions.pop()?;
                    let completion = completion.to_string_lossy();

                    self.input = self.input[..self.input.len()
                                            - part.len()].to_string();
                    self.input.push_str(&completion);
                    self.position += &completion.len() - part.len();

                    self.last_completion = Some(completion.to_string());
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
        self.position = self.position.saturating_sub(last_len);

        let next_comp = self.completions.pop()?;
        let next_comp = next_comp.to_string_lossy();
        self.input.push_str(&next_comp);
        self.position += next_comp.len();
        self.last_completion = Some(next_comp.to_string());
        Ok(())
    }

    pub fn history_up(&mut self) -> HResult<()> {
        if let Ok(historic) = self.history.get_prev(&self.query) {
            self.position = historic.len();
            self.input = historic;
        }
        Ok(())
    }

    pub fn history_down(&mut self) -> HResult<()> {
        if let Ok(historic) = self.history.get_next(&self.query) {
            self.position = historic.len();
            self.input = historic;
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

    pub fn input_finnished(&self) -> HResult<()> {
        return HError::popup_finnished()
    }

    pub fn input_cancelled(&self) -> HResult<()> {
        self.core.show_status("Input cancelled").log();
        return HError::minibuffer_cancel()
    }

    pub fn input_updated(&self) -> HResult<()> {
        return HError::input_updated(self.input.clone())
    }

    pub fn input_empty(&self) -> HResult<()> {
        self.core.show_status("Empty!").log();
        return HError::minibuffer_empty()
    }
}

pub fn find_bins(comp_name: &str) -> HResult<Vec<OsString>> {
    use osstrtools::OsStrTools;

    let paths = std::env::var_os("PATH")?;
    let paths = paths.split(":");

    let completions = paths.iter().map(|path| {
        std::fs::read_dir(path).map(|read_dir| {
            read_dir.map(|file| {
                let file = file?;
                let name = file.file_name();

                // If length is different that means the file starts with comp_name
                if &name.trim_start(comp_name).len() != &name.len() {
                    Ok(name)
                } else {
                    Err(HError::NoCompletionsError)
                }

            })
        })
    }).flatten()
      .flatten()
      .filter(|s| s.is_ok())
      .map(|s| s.unwrap())
      .collect::<Vec<OsString>>();

    if completions.is_empty() { return Err(HError::NoCompletionsError); }

    Ok(completions)
}

pub fn find_files(comp_name: &str) -> HResult<Vec<OsString>> {
    use osstrtools::OsStrTools;

    let mut path = std::env::current_dir()?;
    let comp_path = std::path::PathBuf::from(&comp_name);
    path.push(&comp_path);

    // Tried to complete on an incorrect path
    if comp_name.ends_with("/") && !path.is_dir() {
        return Err(HError::NoCompletionsError)
    }

    let comp_name = OsStr::new(comp_name);
    let filename_part = path.file_name()?;

    let dir = if path.is_dir() { &path } else { path.parent()? };
    let dir = std::path::PathBuf::from(dir);

    let prefix = comp_name.trim_end(&filename_part);

    let reader = std::fs::read_dir(&dir)?;

    let completions = reader.map(|file| {
        let file = file?;
        let name = file.file_name();
        if name.trim_start(&filename_part).len() != name.len() {
            let mut completion = OsString::new();
            if file.file_type()?.is_dir() {
                completion.push(prefix.trim_end("/"));

                // When completing something in the curren dir this will be empty
                if completion != "" {
                    completion.push("/");
                }
                completion.push(name);

                // Add final slash to directory
                completion.push("/");
                Ok(completion)
            } else {
                completion.push(prefix);
                completion.push(name);
                Ok(completion)
            }
        } else {
            Err(HError::NoCompletionsError)
        }
    }).filter_map(|res| res.ok())
      .collect::<Vec<OsString>>();
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
        Ok(format!("{}{}{}{}: {}",
                crate::term::goto_xy(xpos, ypos),
                termion::clear::CurrentLine,
                crate::term::header_color(),
                self.query,
                self.input))
    }

    fn on_key(&mut self, key: Key) -> HResult<()> {
        let prev_input = self.input.clone();

        self.do_key(key)?;

        if self.continuous && prev_input != self.input {
            self.input_updated()?;
        }

        Ok(())
    }

    fn after_draw(&self) -> HResult<()> {
        let cursor_pos = crate::term::string_len(&self.query) +
                         ": ".len() +
                         self.position;

        let mut screen = self.core.screen()?;
        let ysize = screen.ysize()?;

        screen.goto_xy(cursor_pos, ysize).log();
        screen.cursor_show().log();

        Ok(())
    }
}

use crate::keybind::*;

impl Acting for MiniBuffer {
    type Action = MiniBufferAction;

    fn search_in(&self) -> Bindings<Self::Action> {
        self.core.config().keybinds.minibuffer
    }

    fn do_action(&mut self, action: &Self::Action) -> HResult<()> {
        use MiniBufferAction::*;

        match action {
            InsertChar(ch) => {
                self.input.insert(self.position, *ch);
                self.position += 1;
            }
            InsertTab(n) => {
                let fnstr = format!("${}", n-1);
                self.input.insert_str(self.position, &fnstr);
                self.position += 2;
            }
            Cancel => { self.clear(); self.input_cancelled()? }
            Finish => {
                if self.input != "" {
                    self.history.add(&self.query, &self.input);
                }
                self.input_finnished()?
            },
            Complete => self.complete()?,
            DeleteChar => {
                if self.position != self.input.len() {
                    self.input.remove(self.position);
                }
            },
            BackwardDeleteChar => {
                if self.position != 0 {
                    self.input.remove(self.position - 1);
                    self.position -= 1;
                }
            }
            CursorLeft => {
                if self.position != 0 {
                    self.position -= 1;
                }
            },
            CursorRight => {
                if self.position != self.input.len() {
                    self.position += 1;
                }
            },
            HistoryUp => self.history_up()?,
            HistoryDown => self.history_down()?,
            ClearLine => self.clear_line()?,
            DeleteWord => self.delete_word()?,
            CursorToStart => self.position = 0,
            CursorToEnd => self.position = self.input.len(),
        }
        Ok(())
    }
}
