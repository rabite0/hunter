use termion::event::Key;
use failure::Backtrace;

use crate::coordinates::{Coordinates};
use crate::widget::{Widget, WidgetCore};
use crate::hbox::HBox;
use crate::fail::{HError, HResult, ErrorLog};

#[derive(PartialEq)]
pub struct MillerColumns<T> where T: Widget {
    pub widgets: HBox<T>,
    pub core: WidgetCore,
}

impl<T> MillerColumns<T>
where
    T: Widget + PartialEq,
{
    pub fn new(core: &WidgetCore) -> MillerColumns<T> {
        MillerColumns {
            widgets: HBox::new(core),
            core: core.clone(),
        }
    }

    pub fn push_widget(&mut self, widget: T) {
        self.widgets.push_widget(widget);
        self.refresh().log();
    }

    pub fn pop_widget(&mut self) -> Option<T> {
        let widget = self.widgets.pop_widget();
        self.refresh().log();
        widget
    }

    pub fn prepend_widget(&mut self, widget: T) {
        self.widgets.prepend_widget(widget);
    }

    pub fn set_ratios(&mut self, ratios: Vec<usize>) {
        self.widgets.set_ratios(ratios);
    }

    pub fn calculate_coordinates(&self) -> HResult<Vec<Coordinates>> {
        self.widgets.calculate_coordinates()
    }

    pub fn get_left_widget(&self) -> HResult<&T> {
        let len = self.widgets.widgets.len();
        if len < 3 {
            return Err(HError::NoWidgetError(Backtrace::new()));
        }
        let widget = self.widgets.widgets.get(len - 3)?;
        Ok(widget)
    }
    pub fn get_left_widget_mut(&mut self) -> HResult<&mut T> {
        let len = self.widgets.widgets.len();
        if len < 3 {
            return Err(HError::NoWidgetError(Backtrace::new()));
        }
        let widget = self.widgets.widgets.get_mut(len - 3)?;
        Ok(widget)
    }
    pub fn get_main_widget(&self) -> HResult<&T> {
        let len = self.widgets.widgets.len();
        let widget = self.widgets.widgets.get(len-2)?;
        Ok(widget)
    }
    pub fn get_main_widget_mut(&mut self) -> HResult<&mut T> {
        let len = self.widgets.widgets.len();
        let widget = self.widgets.widgets.get_mut(len-2)?;
        Ok(widget)
    }
    pub fn get_right_widget(&self) -> HResult<&T> {
        let widget = self.widgets.widgets.last()?;
        Ok(widget)
    }
    pub fn get_right_widget_mut(&mut self) -> HResult<&mut T> {
        let widget = self.widgets.widgets.last_mut()?;
        Ok(widget)
    }
}

impl<T> Widget for MillerColumns<T>
where
    T: Widget,
    T: PartialEq
{
    fn get_core(&self) -> HResult<&WidgetCore> {
        Ok(&self.core)
    }
    fn get_core_mut(&mut self) -> HResult<&mut WidgetCore> {
        Ok(&mut self.core)
    }

    fn set_coordinates(&mut self, coordinates: &Coordinates) -> HResult<()> {
        self.core.coordinates = coordinates.clone();
        self.widgets.set_coordinates(&coordinates)
    }

    fn refresh(&mut self) -> HResult<()> {
        self.widgets.refresh()
    }

    fn get_drawlist(&self) -> HResult<String> {
        let left_widget = self.get_left_widget()?;
        let main_widget = self.get_main_widget()?;
        let right_widget = self.get_right_widget()?;
        Ok(format!("{}{}{}",
                   main_widget.get_drawlist()?,
                   left_widget.get_drawlist()?,
                   right_widget.get_drawlist()?))
    }

    fn on_key(&mut self, key: Key) -> HResult<()> {
        self.get_main_widget_mut().unwrap().on_key(key)
    }
}
