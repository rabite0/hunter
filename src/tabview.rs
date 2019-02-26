use termion::event::Key;

use crate::coordinates::{Coordinates};
use crate::widget::Widget;

pub trait Tabbable {
    fn new_tab(&mut self);
    fn close_tab(&mut self);
    fn next_tab(&mut self);
    fn on_next_tab(&mut self);
    fn get_tab_names(&self) -> Vec<Option<String>>;
    fn active_tab(&self) -> &dyn Widget;
    fn active_tab_mut(&mut self) -> &mut dyn Widget;
    fn on_key_sub(&mut self, key: Key);
    fn on_key(&mut self, key: Key) {
        match key {
            Key::Ctrl('t') => { self.new_tab(); },
            Key::Ctrl('w') => self.close_tab(),
            Key::Char('\t') => self.next_tab(),
            _ => self.on_key_sub(key)
        }
    }
}


#[derive(PartialEq)]
pub struct TabView<T> where T: Widget, TabView<T>: Tabbable {
    pub widgets: Vec<T>,
    pub active: usize,
    coordinates: Coordinates
}

impl<T> TabView<T> where T: Widget, TabView<T>: Tabbable {
    pub fn new() -> TabView<T> {
        TabView {
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

    pub fn active_tab_(&self) -> &T {
        &self.widgets[self.active]
    }

    pub fn active_tab_mut_(&mut self) -> &mut T {
        &mut self.widgets[self.active]
    }

    pub fn close_tab_(&mut self) {
        if self.active == 0 { return }
        if self.active + 1 >= self.widgets.len() { self.active -= 1 }

        self.pop_widget();
    }

    pub fn next_tab_(&mut self) {
        if self.active + 1 == self.widgets.len() {
            self.active = 0;
        } else {
            self.active += 1
        }
        self.on_next_tab();
    }
}

impl<T> Widget for TabView<T> where T: Widget, TabView<T>: Tabbable {
    fn render_header(&self) -> String {
        let xsize = self.get_coordinates().xsize();
        let header = self.active_tab_().render_header();
        let tab_names = self.get_tab_names();
        let mut nums_length = 0;
        let tabnums = (0..self.widgets.len()).map(|num| {
            nums_length += format!("{}:{} ",
                                   num,
                                   tab_names[num].as_ref().unwrap()).len();
            if num == self.active {
                format!(" {}{}:{}{}{}",
                        crate::term::invert(),
                        num,
                        tab_names[num].as_ref().unwrap(),
                        crate::term::reset(),
                        crate::term::header_color())
            } else {
                format!(" {}:{}", num, tab_names[num].as_ref().unwrap())
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
        self.active_tab_().render_footer()
    }

    fn refresh(&mut self) {
        self.active_tab_mut().refresh();
    }

    fn get_drawlist(&self) -> String {
        self.active_tab_().get_drawlist()
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
        Tabbable::on_key(self, key);
    }
}
