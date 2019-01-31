use ::rayon::prelude::*;

use std::io::BufRead;

use crate::coordinates::{Coordinates, Position, Size};
use crate::files::File;
use crate::term::sized_string;
use crate::widget::Widget;

pub struct TextView {
    pub lines: Vec<String>,
    pub buffer: String,
    pub coordinates: Coordinates,
}

impl TextView {
    pub fn new_from_file(file: &File) -> TextView {
        let file = std::fs::File::open(&file.path).unwrap();
        let file = std::io::BufReader::new(file);
        let lines = file.lines().take(100).map(|line| line.unwrap()).collect();

        TextView {
            lines: lines,
            buffer: String::new(),
            coordinates: Coordinates::new(),
        }
    }
}

impl Widget for TextView {
    fn render(&self) -> Vec<String> {
        vec![]
    }
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
        self.coordinates = coordinates.clone();
    }
    fn render_header(&self) -> String {
        "".to_string()
    }
    fn refresh(&mut self) {
        let (xsize, ysize) = self.get_size().size();
        let (xpos, _) = self.get_position().position();

        self.buffer = self
            .lines
            .par_iter()
            .take(ysize as usize)
            .enumerate()
            .map(|(i, line)| {
                format!(
                    "{}{:xsize$}",
                    crate::term::goto_xy(xpos, i as u16),
                    sized_string(&line, xsize),
                    xsize = xsize as usize
                )
            })
            .collect();
    }

    fn get_drawlist(&self) -> String {
        self.buffer.clone()
    }
}
