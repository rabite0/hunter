use termion::event::{Key,Event};

use crate::term;
use crate::files::Files;
use crate::widget::Widget;
use crate::window::{STATUS_BAR_MARGIN, HEADER_MARGIN};

pub struct ListView<T> {
    pub content: T,
    selection: usize,
    offset: usize,
    buffer: Vec<String>,
    dimensions: (u16, u16),
    position: (u16, u16),
}

impl<T: 'static> ListView<T> where ListView<T>: Widget {
    pub fn new(content: T, dimensions: (u16, u16), position: (u16, u16)) -> Self {
        let view = ListView::<T> {
            content: content,
            selection: 0,
            offset: 0,
            buffer: Vec::new(),
            dimensions: dimensions,
            position: position
        };
        view
    }
    pub fn to_trait(self) -> Box<Widget> {
        Box::new(self)
    }
    
    fn move_up(&mut self) {
        if self.selection == 0 {
            return;
        }

        if self.selection - self.offset <= 0 {
            self.offset -= 1;
        }

        self.selection -= 1;
    }
    fn move_down(&mut self) {
        let lines = self.buffer.len();
        let y_size = self.dimensions.1 as usize;

        if self.selection == lines - 1 {
            return;
        }

        if self.selection + 1 >= y_size && self.selection + 1 - self.offset >= y_size
        {
            self.offset += 1;
        }

        self.selection += 1;
    }

}

impl Widget for ListView<Files> {
    fn get_dimensions(&self) -> (u16, u16) {
        self.dimensions
    }
    fn get_position(&self) -> (u16, u16) {
        self.position
    }
    fn refresh(&mut self) {
        self.buffer = self.render();
    }
    
    fn render(&self) -> Vec<String> {
        self.content.iter().map(|file| {
            self.render_line(&file.name,
                             &format!("{:?}", file.size),
                             false)
        }).collect()
    }
    
    fn get_drawlist(&mut self) -> String {
        let mut output = term::reset();
        let (xsize, ysize) = self.dimensions;
        let (xpos, ypos) = self.position;
        output += &term::reset();

                
        for (i, item) in self.buffer
            .iter()
            .skip(self.offset)
            .take(ysize as usize)
            .enumerate()
        {
            output += &term::normal_color();

            if i == (self.selection - self.offset) {
                output += &term::invert();
            }
            output += &format!("{}{}{}",
                               term::goto_xy(xpos, i as u16 + ypos),
                               item,
                               term::reset());
        }

        
        if ysize as usize > self.buffer.len() {
            let start_y = self.buffer.len() + 1 + ypos as usize;
            for i in start_y..ysize as usize { 
               output += &format!("{}{:xsize$}{}", term::gotoy(i), " ", xsize = xsize as usize);
            }
        }

        output
    }
    fn render_header(&self) -> String {
        format!("{} files", self.content.len())
    }
    
    fn on_key(&mut self, key: Key) {
        match key {
            Key::Up => { self.move_up(); self.refresh() },
            Key::Down => { self.move_down(); self.refresh() },
            //Key::Right => self.go(),
            _ => { self.bad(Event::Key(key)); }
        }
    }
}
