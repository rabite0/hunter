use termion::event::{Key, MouseEvent, Event};

use crate::coordinates::{Coordinates, Size, Position};


pub trait Widget {
    fn render(&self) -> Vec<String>;
    fn get_size(&self) -> &Size;
    fn get_position(&self) -> &Position;
    fn set_size(&mut self, size: Size);
    fn set_position(&mut self, position: Position);
    fn render_header(&self) -> String;


    fn on_event(&mut self, event: Event) {
        match event {
            Event::Key(Key::Char('q')) => panic!("It's your fault!"),
            Event::Key(key) => self.on_key(key),
            Event::Mouse(button) => self.on_mouse(button),
            Event::Unsupported(wtf) => self.on_wtf(wtf),
        }
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

    fn show_status(&self, status: &str) {
        crate::window::show_status(status);
    }

    fn minibuffer(&self, query: &str) -> Option<String> {
        crate::window::minibuffer(query)
    }

    fn bad(&mut self, event: Event) {
        self.show_status(&format!("Stop the nasty stuff!! {:?} does nothing!", event));
    }

    fn get_header_drawlist(&mut self) -> String {
        format!(
            "{}{}{}{:xsize$}",
            crate::term::goto_xy(1,1),
            crate::term::header_color(),
            self.render_header(),
            " ",
            xsize = crate::term::xsize()
        )
    }

    fn get_clearlist(&self) -> String {
        let (xpos, ypos) = self.get_position().position();
        let (xsize, ysize) = self.get_size().size();

        let mut clearcmd = String::from("");
        for line in ypos..ysize {
            clearcmd += &format!("{}{:xsize$}",
                                 crate::term::goto_xy(xpos, line),
                                 " ",
                                 xsize=xsize as usize);
        }

        clearcmd
    }

    fn get_redraw_empty_list(&self, lines: usize) -> String {
        let (xpos, ypos) = self.get_position().position();
        let (xsize, ysize) = self.get_size().size();
        
        let mut output = String::new();

        if ysize as usize > lines {
            let start_y = lines + 1 + ypos as usize;
            for i in start_y..(ysize+2) as usize {
                output += &format!("{}{:xsize$}",
                                   crate::term::goto_xy(xpos,i as u16),
                                   " ",
                                   xsize = xsize as usize);
            }
        }

        output
    }


    fn refresh(&mut self);
    fn get_drawlist(&self) -> String;
}
