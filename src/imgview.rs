use crate::widget::{Widget, WidgetCore};
use crate::coordinates::Coordinates;
use crate::fail::HResult;

use std::path::{Path, PathBuf};

impl std::cmp::PartialEq for ImgView {
    fn eq(&self, other: &Self) -> bool {
        self.core == other.core &&
            self.buffer == other.buffer
    }
}

pub struct ImgView {
    pub core: WidgetCore,
    pub buffer: Vec<String>,
    pub file: PathBuf
}

impl ImgView {
    pub fn new_from_file(core: WidgetCore, file: &Path) -> HResult<ImgView> {
        let mut view = ImgView {
            core: core,
            buffer: vec![],
            file: file.to_path_buf()
        };

        view.encode_file()?;
        Ok(view)
    }

    pub fn encode_file(&mut self) -> HResult<()> {
        let (xsize, ysize) = self.core.coordinates.size_u();
        let file = &self.file;

        let output = std::process::Command::new("preview-gen")
            .arg(format!("{}", (xsize)))
            .arg(format!("{}", (ysize+1)))
            .arg("image")
            .arg(format!("true"))
            .arg(format!("true"))
            .arg(file.to_string_lossy().to_string())
            .output()?
            .stdout;

        let output = std::str::from_utf8(&output)?;
        let output = output.lines()
            .map(|l| l.to_string())
            .collect();

        self.buffer = output;

        Ok(())
    }

    pub fn set_image_data(&mut self, img_data: Vec<String>) {
        self.buffer = img_data;
    }

    pub fn lines(&self) -> usize {
        self.buffer.len()
    }
}


impl Widget for ImgView {
    fn get_core(&self) -> HResult<&WidgetCore> {
        Ok(&self.core)
    }

    fn get_core_mut(&mut self) -> HResult<&mut WidgetCore> {
        Ok(&mut self.core)
    }

    fn set_coordinates(&mut self, coordinates: &Coordinates) -> HResult<()> {
        if &self.core.coordinates == coordinates { return Ok(()) }

        self.core.coordinates = coordinates.clone();
        self.encode_file()?;

        Ok(())
    }

    fn refresh(&mut self) -> HResult<()> {

        Ok(())
    }

    fn get_drawlist(&self) -> HResult<String> {
        let (xpos, ypos) = self.core.coordinates.position_u();

        let mut draw = self.buffer
            .iter()
            .enumerate()
            .fold(String::new(), |mut draw, (pos, line)| {
                draw += &format!("{}", crate::term::goto_xy_u(xpos+1,
                                                              ypos + pos));
                draw += line;
                draw
            });

        draw += &format!("{}", termion::style::Reset);

        Ok(draw)
    }
}
