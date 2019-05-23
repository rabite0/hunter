use std::io::BufRead;

use crate::files::File;
use crate::term::sized_string;
use crate::widget::{Widget, WidgetCore};
use crate::fail::HResult;
use crate::dirty::Dirtyable;

#[derive(PartialEq)]
pub struct TextView {
    pub lines: Vec<String>,
    pub core: WidgetCore,
    pub follow: bool,
    pub offset: usize,
}

impl TextView {
    pub fn new_blank(core: &WidgetCore) -> TextView {
        TextView {
            lines: vec![],
            core: core.clone(),
            follow: false,
            offset: 0,
        }
    }
    pub fn new_from_file(core: &WidgetCore, file: &File) -> HResult<TextView> {
        let file = std::fs::File::open(&file.path)?;
        let file = std::io::BufReader::new(file);
        let lines = file.lines().map(|line|
                                     Ok(line?
                                        .replace("\t", "    ")))
            .filter_map(|l: HResult<String>| l.ok())
            .collect();

        Ok(TextView {
            lines: lines,
            core: core.clone(),
            follow: false,
            offset: 0,
        })
    }
    pub fn new_from_file_limit_lines(core: &WidgetCore,
                                     file: &File,
                                     num: usize) -> HResult<TextView> {
        let file = std::fs::File::open(&file.path).unwrap();
        let file = std::io::BufReader::new(file);
        let lines = file.lines()
                        .take(num)
                        .map(|line|
                             Ok(line?
                                .replace("\t", "    ")))
            .filter_map(|l: HResult<String>| l.ok())
            .collect();

        Ok(TextView {
            lines: lines,
            core: core.clone(),
            follow: false,
            offset: 0,
        })
    }

    pub fn set_text(&mut self, text: &str) -> HResult<()> {
        let lines = text.lines().map(|l| l.to_string()).collect();
        self.lines = lines;
        self.core.set_dirty();
        self.refresh()
    }

    pub fn toggle_follow(&mut self) {
        self.follow = !self.follow
    }

    pub fn scroll(&mut self, amount: isize) {
        let ysize = self.get_coordinates().unwrap().ysize() as isize;
        let offset = self.offset as isize;
        let len = self.lines.len() as isize;

        if len <= ysize + offset { return }

        if amount > 0 {
            if  ysize + amount + offset + 1 >= len {
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

        let output = self.core.get_clearlist()? +
            &self
            .lines
            .iter()
            .skip(self.offset)
            .take(ysize as usize)
            .enumerate()
            .map(|(i, line)| {
                format!(
                    "{}{}{}",
                    crate::term::goto_xy(xpos, i as u16 + ypos),
                    crate::term::reset(),
                    sized_string(&line, xsize))
            })
            .collect::<String>();
        Ok(output)
    }
}
