use std::io::BufRead;

use strip_ansi_escapes::strip;
use termion::event::Key;

use crate::dirty::Dirtyable;
use crate::fail::{HError, HResult};
use crate::files::File;
use crate::term::sized_string_u;
use crate::widget::{Widget, WidgetCore};

#[derive(Debug, PartialEq)]
pub struct TextView {
    pub lines: Vec<String>,
    pub core: WidgetCore,
    pub follow: bool,
    pub offset: usize,
    file: Option<File>,
    limited: bool,
}

impl TextView {
    pub fn new_blank(core: &WidgetCore) -> TextView {
        TextView {
            lines: vec![],
            core: core.clone(),
            follow: false,
            offset: 0,
            file: None,
            limited: false,
        }
    }

    pub fn new_from_file(core: &WidgetCore, file: &File) -> HResult<TextView> {
        let mut view = TextView::new_from_file_limit_lines(core, file, 0)?;
        view.limited = false;
        Ok(view)
    }

    pub fn new_from_file_limit_lines(
        core: &WidgetCore,
        file: &File,
        num: usize,
    ) -> HResult<TextView> {
        let buf = std::fs::File::open(&file.path).map(|f| std::io::BufReader::new(f))?;

        let lines = buf
            .lines()
            .enumerate()
            .take_while(|(i, _)| num == 0 || i <= &num)
            .map(|(_, l)| {
                l.map_err(HError::from)
                    .and_then(|l| {
                        let l = strip(&l);
                        Ok(String::from_utf8_lossy(&l?).to_string())
                    })
                    .map_err(HError::from)
            })
            .collect::<HResult<_>>()?;

        Ok(TextView {
            lines: lines,
            core: core.clone(),
            follow: false,
            offset: 0,
            file: Some(file.clone()),
            limited: true,
        })
    }

    pub fn set_text(&mut self, text: &str) -> HResult<()> {
        let lines = text.lines().map(|l| l.to_string()).collect();
        self.lines = lines;
        self.limited = false;
        self.file = None;
        self.core.set_dirty();
        self.refresh()
    }

    pub fn set_lines(&mut self, lines: Vec<String>) -> HResult<()> {
        self.lines = lines;
        self.limited = false;
        self.file = None;
        self.core.set_dirty();
        self.refresh()
    }

    pub fn load_full(&mut self) {
        if self.limited {
            self.file
                .as_ref()
                .and_then(|f| TextView::new_from_file(&self.core, f).ok())
                .map(|v| {
                    *self = v;
                    self.limited = false;
                });
        }
    }

    pub fn toggle_follow(&mut self) {
        self.follow = !self.follow
    }

    pub fn scroll(&mut self, amount: isize) {
        let ysize = self.get_coordinates().unwrap().ysize() as isize;
        let offset = self.offset as isize;
        let len = self.lines.len() as isize;

        if len <= ysize + offset {
            return;
        }

        if amount > 0 {
            if ysize + amount + offset + 1 >= len {
                // Too far down
                self.offset = (len - ysize - 1) as usize;
            } else {
                self.offset = (offset as isize + amount) as usize;
            }
        } else if amount < 0 {
            if offset + amount >= 0 {
                self.offset = (offset + amount) as usize;
            } else {
                self.offset = 0;
            }
        }

        if offset != self.offset as isize {
            self.core.set_dirty();
        }
    }

    pub fn scroll_up(&mut self) {
        self.scroll(-1);
    }

    pub fn scroll_down(&mut self) {
        self.scroll(1);
    }

    pub fn page_up(&mut self) {
        let ysize = self.get_coordinates().unwrap().ysize() as isize;
        self.scroll(0 - ysize + 1);
    }

    pub fn page_down(&mut self) {
        let ysize = self.get_coordinates().unwrap().ysize() as isize;
        self.scroll(ysize - 1);
    }

    pub fn scroll_top(&mut self) {
        self.offset = 0;
    }

    pub fn scroll_bottom(&mut self) {
        let len = self.lines.len() as isize;
        self.scroll(len);
    }
}

impl Widget for TextView {
    fn get_core(&self) -> HResult<&WidgetCore> {
        Ok(&self.core)
    }
    fn get_core_mut(&mut self) -> HResult<&mut WidgetCore> {
        Ok(&mut self.core)
    }
    fn refresh(&mut self) -> HResult<()> {
        // let (xsize, ysize) = self.get_coordinates()?.size().size();
        // let (xpos, ypos) = self.get_coordinates()?.position().position();
        // let len = self.lines.len();

        if self.follow {
            self.scroll_bottom();
        }

        if self.core.is_dirty() {
            self.core.set_clean();
        }
        Ok(())
    }

    fn get_drawlist(&self) -> HResult<String> {
        let (xsize, ysize) = self.get_coordinates()?.size().size();
        let (xpos, ypos) = self.get_coordinates()?.position().position();

        let mut output = crate::term::reset();

        output += &self
            .lines
            .iter()
            .skip(self.offset)
            .take(ysize as usize)
            .enumerate()
            .map(|(i, line)| {
                format!(
                    "{}{}",
                    crate::term::goto_xy(xpos, i as u16 + ypos),
                    sized_string_u(&line, (xsize - 1) as usize)
                )
            })
            .collect::<String>();
        Ok(output)
    }

    fn render_footer(&self) -> HResult<String> {
        let (xsize, ysize) = self.core.coordinates.size_u();
        let (_, ypos) = self.core.coordinates.position_u();
        let lines = self.lines.len().saturating_sub(1);
        let current_line_top = self.offset;
        let current_line_bot = std::cmp::min(current_line_top + ysize + 1, lines);
        let line_hint = format!("{} - {} / {}", current_line_top, current_line_bot, lines);
        let hint_xpos = xsize - line_hint.len();
        let hint_ypos = ysize + ypos + 1;

        let footer = format!(
            "{}{}",
            crate::term::goto_xy_u(hint_xpos, hint_ypos),
            line_hint
        );

        Ok(footer)
    }

    fn on_key(&mut self, key: Key) -> HResult<()> {
        self.do_key(key)
    }
}

use crate::keybind::{Acting, Bindings, Movement};

impl Acting for TextView {
    type Action = Movement;

    fn search_in(&self) -> Bindings<Self::Action> {
        Bindings::default()
    }

    fn movement(&mut self, movement: &Movement) -> HResult<()> {
        use Movement::*;

        self.load_full();

        match movement {
            Up(n) => {
                for _ in 0..*n {
                    self.scroll_up();
                }
                self.refresh()?;
            }
            Down(n) => {
                for _ in 0..*n {
                    self.scroll_down();
                }
                self.refresh()?;
            }
            PageUp => self.page_up(),
            PageDown => self.page_down(),
            Top => self.scroll_top(),
            Bottom => self.scroll_bottom(),
            Left | Right => {}
        }

        Ok(())
    }

    fn do_action(&mut self, _action: &Self::Action) -> HResult<()> {
        Ok(())
    }
}
