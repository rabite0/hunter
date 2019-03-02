use termion::event::Key;
use failure::Backtrace;

use crate::coordinates::{Coordinates, Position, Size};
use crate::preview::Previewer;
use crate::widget::{Widget, WidgetCore};
use crate::hbox::HBox;
use crate::fail::{HError, HResult, ErrorLog};

#[derive(PartialEq)]
pub struct MillerColumns<T> where T: Widget {
    pub widgets: HBox<T>,
    pub core: WidgetCore,
    // pub left: Option<T>,
    // pub main: Option<T>,
    //pub preview: AsyncPreviewer,
    pub preview: Previewer,
    pub ratio: (u16, u16, u16),
}

impl<T> MillerColumns<T>
where
    T: Widget + PartialEq,
{
    pub fn new(core: &WidgetCore) -> MillerColumns<T> {
        MillerColumns {
            widgets: HBox::new(core),
            core: core.clone(),
            ratio: (20, 30, 50),
            preview: Previewer::new(core)
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

    pub fn calculate_coordinates(&self) -> (Coordinates, Coordinates, Coordinates) {
        let coordinates = self.get_coordinates().unwrap();
        let xsize = coordinates.xsize();
        let ysize = coordinates.ysize();
        let top = coordinates.top().y();
        let ratio = self.ratio;

        let left_xsize = xsize * ratio.0 / 100;
        let left_size = Size((left_xsize, ysize));
        let left_pos = coordinates.top();

        let main_xsize = xsize * ratio.1 / 100;
        let main_size = Size((main_xsize, ysize));
        let main_pos = Position((left_xsize + 2, top));

        let preview_xsize = xsize * ratio.2 / 100;
        let preview_size = Size((preview_xsize - 1, ysize));
        let preview_pos = Position((left_xsize + main_xsize + 3, top));

        let left_coords = Coordinates {
            size: left_size,
            position: left_pos,
        };

        let main_coords = Coordinates {
            size: main_size,
            position: main_pos,
        };

        let preview_coords = Coordinates {
            size: preview_size,
            position: preview_pos,
        };

        (left_coords, main_coords, preview_coords)
    }

    pub fn get_left_widget(&self) -> HResult<&T> {
        let len = self.widgets.widgets.len();
        if len < 2 {
            return Err(HError::NoWidgetError(Backtrace::new()));
        }
        let widget = self.widgets.widgets.get(len - 2)?;
        Ok(widget)
    }
    pub fn get_left_widget_mut(&mut self) -> HResult<&mut T> {
        let len = self.widgets.widgets.len();
        if len < 2 {
            return Err(HError::NoWidgetError(Backtrace::new()));
        }
        let widget = self.widgets.widgets.get_mut(len - 2)?;
        Ok(widget)
    }
    pub fn get_main_widget(&self) -> HResult<&T> {
        let widget = self.widgets.widgets.last()?;
        Ok(widget)
    }
    pub fn get_main_widget_mut(&mut self) -> HResult<&mut T> {
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
    fn refresh(&mut self) -> HResult<()> {
        let (left_coords, main_coords, preview_coords) = self.calculate_coordinates();

        if let Ok(left_widget) = self.get_left_widget_mut() {
            left_widget.set_coordinates(&left_coords).log();
        }

        if let Ok(main_widget) = self.get_main_widget_mut() {
            main_widget.set_coordinates(&main_coords).log();
        }

        let preview_widget = &mut self.preview;
        preview_widget.set_coordinates(&preview_coords)?;
        Ok(())
    }

    fn get_drawlist(&self) -> HResult<String> {
        let left_widget = self.get_left_widget()?;
        let main_widget = self.get_main_widget()?;
        let preview = self.preview.get_drawlist()?;
        Ok(format!("{}{}{}",
                   main_widget.get_drawlist()?,
                   left_widget.get_drawlist()?,
                   preview))
    }

    fn on_key(&mut self, key: Key) -> HResult<()> {
        self.get_main_widget_mut().unwrap().on_key(key)
    }
}
