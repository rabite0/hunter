use termion::event::Key;

use crate::widget::{Widget, WidgetCore, Events};
use crate::fail::{HResult, ErrorLog};
use crate::coordinates::Coordinates;

pub trait Tabbable {
    fn new_tab(&mut self) -> HResult<()>;
    fn close_tab(&mut self) -> HResult<()>;
    fn next_tab(&mut self) -> HResult<()>;
    fn goto_tab(&mut self, index: usize) -> HResult<()>;
    fn on_tab_switch(&mut self) -> HResult<()> {
        Ok(())
    }
    fn get_tab_names(&self) -> Vec<Option<String>>;
    fn active_tab(&self) -> &dyn Widget;
    fn active_tab_mut(&mut self) -> &mut dyn Widget;
    fn on_key_sub(&mut self, key: Key) -> HResult<()>;
    fn on_key(&mut self, key: Key) -> HResult<()> {
        match key {
            Key::F(n) => self.goto_tab(n as usize -1),
            Key::Ctrl('t') => self.new_tab(),
            Key::Ctrl('w') => self.close_tab(),
            Key::Char('\t') => self.next_tab(),
            _ => self.on_key_sub(key)
        }
    }
    fn on_refresh(&mut self) -> HResult<()> { Ok(()) }
}


#[derive(PartialEq)]
pub struct TabView<T> where T: Widget, TabView<T>: Tabbable {
    pub widgets: Vec<T>,
    pub active: usize,
    core: WidgetCore
}

impl<T> TabView<T> where T: Widget, TabView<T>: Tabbable {
    pub fn new(core: &WidgetCore) -> TabView<T> {
        TabView {
            widgets: vec![],
            active: 0,
            core: core.clone()
        }
    }

    pub fn push_widget(&mut self, widget: T) -> HResult<()> {
        self.widgets.push(widget);
        Ok(())
    }

    pub fn pop_widget(&mut self) -> HResult<T> {
        let widget = self.widgets.pop()?;
        if self.widgets.len() <= self.active {
            self.active -= 1;
        }
        Ok(widget)
    }

    pub fn remove_widget(&mut self, index: usize) -> HResult<()> {
        let len = self.widgets.len();
        if len > 1 {
            self.widgets.remove(index);
            if index+1 == len {
                self.active -= 1;
            }
        }
        Ok(())
    }

    pub fn goto_tab_(&mut self, index: usize) -> HResult<()> {
        if index < self.widgets.len() {
            self.active = index;
            self.on_tab_switch().log();
        }
        Ok(())
    }

    pub fn active_tab_(&self) -> &T {
        &self.widgets[self.active]
    }

    pub fn active_tab_mut_(&mut self) -> &mut T {
        &mut self.widgets[self.active]
    }

    pub fn close_tab_(&mut self) -> HResult<()> {
        self.remove_widget(self.active).log();
        Ok(())
    }

    pub fn next_tab_(&mut self) {
        if self.active + 1 == self.widgets.len() {
            self.active = 0;
        } else {
            self.active += 1
        }
        self.on_tab_switch().log();
    }
}

impl<T> Widget for TabView<T> where T: Widget, TabView<T>: Tabbable {
    fn get_core(&self) -> HResult<&WidgetCore> {
        Ok(&self.core)
    }
    fn get_core_mut(&mut self) -> HResult<&mut WidgetCore> {
        Ok(&mut self.core)
    }

    fn set_coordinates(&mut self, coordinates: &Coordinates) -> HResult<()> {
        self.core.coordinates = coordinates.clone();
        for widget in &mut self.widgets {
            widget.set_coordinates(coordinates).log();
        }
        Ok(())
    }

    fn render_header(&self) -> HResult<String> {
        let xsize = self.get_coordinates()?.xsize();
        let header = self.active_tab_().render_header()?;
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


        let nums_pos = xsize.saturating_sub(nums_length as u16);

        Ok(format!("{}{}{}{}",
                header,
                crate::term::header_color(),
                crate::term::goto_xy(nums_pos, 1),
                tabnums))
    }

    fn render_footer(&self) -> HResult<String>
    {
        self.active_tab_().render_footer()
    }

    fn refresh(&mut self) -> HResult<()> {
        Tabbable::on_refresh(self).log();
        self.active_tab_mut().refresh()
    }

    fn get_drawlist(&self) -> HResult<String> {
        self.active_tab_().get_drawlist()
    }

    fn on_key(&mut self, key: Key) -> HResult<()> {
        Tabbable::on_key(self, key)?;
        Ok(())
    }
}
