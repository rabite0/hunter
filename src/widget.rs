use termion::event::{Key, MouseEvent, Event};
use unicode_width::{UnicodeWidthStr};

use crate::term;


pub trait Widget {
    fn render(&self) -> Vec<String>;
    fn get_dimensions(&self) -> (u16, u16);
    fn get_position(&self) -> (u16, u16);
    fn render_line(&self, left: &str, right: &str, highlight: bool) -> String {
        let (xsize, _) = self.get_dimensions();
        let text_color = match highlight {
            true => term::highlight_color(),
            false => term::normal_color(),
        };
        let sized_string = self.sized_string(left);
        let padding = xsize - sized_string.width() as u16;

        format!(
            "{}{}{:padding$}{}{}{}",
            text_color,
            sized_string,
            " ",
            term::highlight_color(),
            term::cursor_left(right.width()),
            right,
            padding = padding as usize
        )
    }
    // fn add_highlight(&self, line: &str) -> String {
    //     line.to_string()
    // }
    fn render_header(&self) -> String;
    fn sized_string(&self, string: &str) -> String {
        let (xsize, _) = self.get_dimensions();
        let lenstr: String = string.chars().fold("".into(), |acc,ch| {
            if acc.width() + 1  >= xsize as usize { acc }
            else { acc + &ch.to_string() }
        });
        lenstr
    }

      
    fn on_key(&mut self, key: Key) {
        match key {
            _ => {
                self.bad(Event::Key(key))
            }
        }
    }
    
    fn on_mouse(&mut self, event: MouseEvent) {
        match event {
            _ => {
                self.bad(Event::Mouse(event))
            }
        }
    }

    fn on_wtf(&mut self, event: Vec<u8>) {
        match event {
            _ => {
                self.bad(Event::Unsupported(event))
            }
        }
    }
        
                

    fn show_status(&mut self, status: &str) {
        crate::window::show_status(status);
    }
    
    fn bad(&mut self, event: Event) {
        self.show_status(&format!("Stop the nasty stuff!! {:?} does nothing!", event));
    }

    //fn get_window(&self) -> Window<Widget>;
    //fn get_window_mut(&mut self) -> &mut Window<dyn Widget>;

    //fn run(&mut self) {
        // self.draw();
        // self.handle_input();
    //}


    //fn get_buffer(&self) -> &Vec<String>;
    fn refresh(&mut self);
    fn get_drawlist(&mut self) -> String;
}
