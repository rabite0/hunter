use termion::event::{Event, Key, MouseEvent};

use crate::coordinates::{Coordinates, Position, Size};

use std::io::{BufWriter, Write};


pub trait Widget {
    fn get_coordinates(&self) -> &Coordinates;
    fn set_coordinates(&mut self, coordinates: &Coordinates);
    fn render_header(&self) -> String;
    fn render_footer(&self) -> String { "".into() }
    fn refresh(&mut self);
    fn get_drawlist(&self) -> String;


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
            _ => self.bad(Event::Key(key)),
        }
    }

    fn on_mouse(&mut self, event: MouseEvent) {
        match event {
            _ => self.bad(Event::Mouse(event)),
        }
    }

    fn on_wtf(&mut self, event: Vec<u8>) {
        match event {
            _ => self.bad(Event::Unsupported(event)),
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
            "{}{}{:xsize$}{}{}",
            crate::term::goto_xy(1, 1),
            crate::term::header_color(),
            " ",
            crate::term::goto_xy(1, 1),
            self.render_header(),
            xsize = self.get_coordinates().xsize() as usize
        )
    }

    fn get_footer_drawlist(&mut self) -> String {
        let xsize = self.get_coordinates().xsize();
        let ypos = crate::term::ysize();
        format!(
            "{}{}{:xsize$}{}{}",
            crate::term::goto_xy(1, ypos),
            crate::term::header_color(),
            " ",
            crate::term::goto_xy(1, ypos),
            self.render_footer(),
            xsize = xsize as usize)
    }

    fn get_clearlist(&self) -> String {
        let (xpos, ypos) = self.get_coordinates().u16position();
        let (xsize, ysize) = self.get_coordinates().u16size();

        (ypos..ysize + 2)
            .map(|line| {
                format!(
                    "{}{}{:xsize$}",
                    crate::term::reset(),
                    crate::term::goto_xy(xpos, line),
                    " ",
                    xsize = xsize as usize
                )
            })
            .collect()
    }

    fn get_redraw_empty_list(&self, lines: usize) -> String {
        let (xpos, ypos) = self.get_coordinates().u16position();
        let (xsize, ysize) = self.get_coordinates().u16size();

        let start_y = lines + ypos as usize;
        (start_y..(ysize + 2) as usize)
            .map(|i| {
                format!(
                    "{}{:xsize$}",
                    crate::term::goto_xy(xpos, i as u16),
                    " ",
                    xsize = xsize as usize
                )
            })
            .collect()
    }

    fn animate_slide_up(&mut self) {
        let coords = self.get_coordinates().clone();
        let xpos = coords.position().x();
        let ypos = coords.position().y();
        let xsize = coords.xsize();
        let ysize = coords.ysize();
        let clear = self.get_clearlist();
        let pause = std::time::Duration::from_millis(5);
        let mut bufout = std::io::BufWriter::new(std::io::stdout());

        for i in (0..10).rev() {
            let coords = Coordinates { size: Size((xsize,ysize-i)),
                                       position: Position
                                           ((xpos,
                                             ypos+i))
            };
            self.set_coordinates(&coords);
            let buffer = self.get_drawlist();
            write!(bufout, "{}{}",
                   clear, buffer).unwrap();


            std::thread::sleep(pause);
        }
    }
}
