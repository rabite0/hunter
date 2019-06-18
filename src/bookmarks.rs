use termion::event::Key;

use std::collections::HashMap;

use crate::fail::{HResult, HError, ErrorLog};
use crate::widget::{Widget, WidgetCore};
use crate::coordinates::Coordinates;
use crate::term;

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Bookmarks {
    mapping: HashMap<char, String>,
}

impl Bookmarks {
    pub fn new() -> Bookmarks {
        let mut bm = Bookmarks { mapping: HashMap::new() };
        bm.load().log();
        bm
    }
    pub fn add(&mut self, key: char, path: &str) -> HResult<()> {
        self.mapping.insert(key, path.to_string());
        self.save()?;
        Ok(())
    }
    pub fn get(&self, key: char) -> HResult<&String> {
        let path = self.mapping.get(&key)?;
        Ok(path)
    }
    pub fn load(&mut self) -> HResult<()> {
        let bm_file = crate::paths::bookmark_path()?;

        if !bm_file.exists() {
            self.import().log();
        }

        let bm_content = std::fs::read_to_string(bm_file)?;
        let mapping = bm_content.lines()
            .fold(HashMap::new(), |mut bm, line| {
            let parts = line.splitn(2, ":").collect::<Vec<&str>>();
            if parts.len() == 2 {
                if let Some(key) = parts[0].chars().next() {
                    let path = parts[1].to_string();
                    bm.insert(key, path);
                }
            }
            bm
        });

        self.mapping = mapping;
        Ok(())
    }
    pub fn import(&self) -> HResult<()> {
        let mut ranger_bm_path = crate::paths::ranger_path()?;
        ranger_bm_path.push("bookmarks");

        if ranger_bm_path.exists() {
            let bm_file = crate::paths::bookmark_path()?;
            std::fs::copy(ranger_bm_path, bm_file)?;
        }
        Ok(())
    }
    pub fn save(&self) -> HResult<()> {
        let bm_file = crate::paths::bookmark_path()?;
        let bookmarks = self.mapping.iter().map(|(key, path)| {
            format!("{}:{}\n", key, path)
        }).collect::<String>();

        std::fs::write(bm_file, bookmarks)?;

        Ok(())
    }
}


pub struct BMPopup {
    core: WidgetCore,
    bookmarks: Bookmarks,
    bookmark_path: Option<String>,
    add_mode: bool,
}

impl BMPopup {
    pub fn new(core: &WidgetCore) -> BMPopup {
        let mut bmpopup = BMPopup {
            core: core.clone(),
            bookmarks: Bookmarks::new(),
            bookmark_path: None,
            add_mode: false
        };
        bmpopup.set_coordinates(&core.coordinates).log();
        bmpopup
    }

    pub fn pick(&mut self, cwd: String) -> HResult<String> {
        self.bookmark_path = Some(cwd);
        self.refresh()?;
        match self.popup() {
            Ok(_) => {},
            Err(HError::PopupFinnished) => {},
            err @ Err(HError::TerminalResizedError) => err?,
            err @ Err(HError::WidgetResizedError) => err?,
            err @ Err(_) => err?,
        }
        self.get_core()?.clear()?;

        let bookmark = self.bookmark_path.take();
        Ok(bookmark?)
    }

    pub fn add(&mut self, path: &str) -> HResult<()> {
        self.add_mode = true;
        self.bookmark_path = Some(path.to_string());
        self.refresh()?;
        self.get_core()?.clear()?;
        self.popup()?;
        self.get_core()?.clear()?;
        Ok(())
    }

    fn resize(&mut self) -> HResult<()> {
        HError::terminal_resized()?
    }

    pub fn render_line(&self, n: u16, key: &char, path: &str) -> String {
        let xsize = term::xsize();
        let padding = xsize - 4;

        format!(
            "{}{}{}: {:padding$}",
            crate::term::goto_xy(1, n),
            crate::term::reset(),
            key,
            path,
            padding = padding as usize)
    }
}


impl Widget for BMPopup {
    fn get_core(&self) -> HResult<&WidgetCore> {
        Ok(&self.core)
    }
    fn get_core_mut(&mut self) -> HResult<&mut WidgetCore> {
        Ok(&mut self.core)
    }
    fn refresh(&mut self) -> HResult<()> {
        Ok(())
    }

    fn resize(&mut self) -> HResult<()> {
        HError::terminal_resized()
    }

    fn set_coordinates(&mut self, _: &Coordinates) -> HResult<()> {
        let (xsize, ysize) = crate::term::size()?;
        let len = self.bookmarks.mapping.len();
        let ysize = ysize.saturating_sub( len + 1 );

        self.core.coordinates.set_size_u(xsize.saturating_sub(1), len);
        self.core.coordinates.set_position_u(1, ysize);

        Ok(())
    }

    fn get_drawlist(&self) -> HResult<String> {
        let ypos = self.get_coordinates()?.ypos();

        let mut drawlist = String::new();

        if !self.add_mode {
            let cwd = self.bookmark_path.as_ref()?;
            drawlist += &self.render_line(ypos, &'`', cwd);
        }

        let bm_list = self.bookmarks.mapping.iter().enumerate().map(|(i, (key, path))| {
            let line = i as u16 + ypos + 1;
            self.render_line(line, key, path)
        }).collect::<String>();

        drawlist += &bm_list;

        Ok(drawlist)
    }
    fn on_key(&mut self, key: Key) -> HResult<()> {
        match key {
            Key::Ctrl('c') | Key::Esc => {
                self.bookmark_path = None;
                return HError::popup_finnished()
            },
            Key::Char('`') => return HError::popup_finnished(),
            Key::Char(key) => {
                if self.add_mode {
                    let path = self.bookmark_path.take()?;
                    self.bookmarks.add(key, &path)?;
                    self.add_mode = false;
                    self.bookmarks.save().log();
                    return HError::popup_finnished();
                }
                if let Ok(path) = self.bookmarks.get(key) {
                    self.bookmark_path.replace(path.clone());
                    return HError::popup_finnished();
                }
            }
            Key::Alt(key) => {
                self.bookmarks.mapping.remove(&key);
                self.bookmarks.save().log();
                return HError::widget_resized();
            }
            _ => {}
        }
        Ok(())
    }
}
