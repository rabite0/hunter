use termion::event::Key;

use crate::coordinates::{Coordinates, Position, Size};
use crate::widget::Widget;

pub trait Tabbable<T: Widget> {
    fn new_tab(&self) -> T;
    fn on_next_tab(&mut self);
}


#[derive(PartialEq)]
pub struct TabView<T> where T: Widget {
    widgets: Vec<T>,
    active: usize,
    coordinates: Coordinates
}

impl<T> TabView<T> where T: Widget + Tabbable<T> {
    pub fn new() -> Self {
        Self {
            widgets: vec![],
            active: 0,
            coordinates: Coordinates::new()
        }
    }

    pub fn push_widget(&mut self, widget: T) {
        self.widgets.push(widget);
        self.refresh();
    }

    pub fn pop_widget(&mut self) -> Option<T> {
        let widget = self.widgets.pop();
        self.refresh();
        widget
    }

    pub fn active_widget(&self) -> &T {
        &self.widgets[self.active]
    }
    
    pub fn active_widget_mut(&mut self) -> &mut T {
        &mut self.widgets[self.active]
    }

    pub fn new_tab(&mut self) {
        let tab = self.active_widget().new_tab();
        self.push_widget(tab);
        self.active += 1;
    }

    pub fn close_tab(&mut self) {
        if self.active == 0 { return }
        if self.active + 1 >= self.widgets.len() { self.active -= 1 }
            
        self.pop_widget();
    }

    pub fn next_tab(&mut self) {
        if self.active + 1 == self.widgets.len() {
            self.active = 0;
        } else {
            self.active += 1
        }
        self.active_widget_mut().on_next_tab();
    }
}

impl<T> Widget for TabView<T> where T: Widget + Tabbable<T> + PartialEq {
    fn render_header(&self) -> String {
        let xsize = self.get_coordinates().xsize();
        let header = self.active_widget().render_header();
        let mut nums_length = 0;
        let tabnums = (0..self.widgets.len()).map(|num| {
            nums_length += format!("{} ", num).len();
            if num == self.active {
                format!(" {}{}{}{}",
                        crate::term::invert(),
                        num,
                        crate::term::reset(),
                        crate::term::header_color())
            } else {
                format!(" {}", num)
            }
        }).collect::<String>();

        let nums_pos = xsize - nums_length as u16;
        
        format!("{}{}{}{}",
                header,
                crate::term::header_color(),
                crate::term::goto_xy(nums_pos, 1),
                tabnums)
    }

    fn render_footer(&self) -> String {
        self.active_widget().render_footer()
    }

    fn refresh(&mut self) {
        self.active_widget_mut().refresh();
    }

    fn get_drawlist(&self) -> String {
        self.active_widget().get_drawlist()
    }

    fn get_size(&self) -> &Size {
        &self.coordinates.size
    }
    fn get_position(&self) -> &Position {
        &self.coordinates.position
    }
    fn set_size(&mut self, size: Size) {
        self.coordinates.size = size;
    }
    fn set_position(&mut self, position: Position) {
        self.coordinates.position = position;
    }
    fn get_coordinates(&self) -> &Coordinates {
        &self.coordinates
    }
    fn set_coordinates(&mut self, coordinates: &Coordinates) {
        if self.coordinates == *coordinates {
            return;
        }
        self.coordinates = coordinates.clone();
        self.refresh();
    }
    
    fn on_key(&mut self, key: Key) {
        match key {
            Key::Ctrl('t') => self.new_tab(),
            Key::Ctrl('w') => self.close_tab(),
            Key::Char('\t') => self.next_tab(),
            _ => self.active_widget_mut().on_key(key)
        }
        
    }
}
