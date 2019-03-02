use std::io::BufRead;

use crate::files::File;
use crate::term::sized_string;
use crate::widget::{Widget, WidgetCore};
use crate::fail::HResult;

#[derive(PartialEq)]
pub struct TextView {
    pub lines: Vec<String>,
    pub buffer: String,
    pub core: WidgetCore
}

impl TextView {
    pub fn new_blank(core: &WidgetCore) -> TextView {
        TextView {
            lines: vec![],
            buffer: String::new(),
            core: core.clone()
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
            buffer: String::new(),
            core: core.clone()
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
            buffer: String::new(),
            core: core.clone()
        })
    }

    pub fn set_text(&mut self, text: &str) -> HResult<()> {
        let lines = text.lines().map(|l| l.to_string()).collect();
        self.lines = lines;
        self.refresh()
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
        let (xsize, ysize) = self.get_coordinates()?.size().size();
        let (xpos, ypos) = self.get_coordinates()?.position().position();

        self.buffer = self.get_clearlist()? +
            &self
            .lines
            .iter()
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
        Ok(())
    }

    fn get_drawlist(&self) -> HResult<String> {
        Ok(self.buffer.clone())
    }
}
