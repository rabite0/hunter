use termion::event::{Event};

use crate::widget::{Widget, WidgetCore};
use crate::coordinates::{Coordinates, Size, Position};
use crate::fail::{HResult, ErrorLog};

#[derive(PartialEq)]
pub struct HBox<T: Widget> {
    pub core: WidgetCore,
    pub widgets: Vec<T>,
    pub active: Option<usize>,
}


impl<T> HBox<T> where T: Widget + PartialEq {
    pub fn new(core: &WidgetCore) -> HBox<T> {
        HBox { core: core.clone(),
               widgets: vec![],
               active: None
         }
    }


    pub fn resize_children(&mut self) {
        let coords: Vec<Coordinates>
            = self.widgets.iter().map(
                |w|
                self.calculate_coordinates(w)).collect();
        for (widget, coord) in self.widgets.iter_mut().zip(coords.iter()) {
            widget.set_coordinates(coord).log();
        }
    }

    pub fn push_widget(&mut self, widget: T) where T: PartialEq {
        self.widgets.push(widget);
        self.resize_children();
        self.refresh().log();
    }

    pub fn pop_widget(&mut self) -> Option<T> {
        let widget = self.widgets.pop();
        self.resize_children();
        self.refresh().log();
        widget
    }

    pub fn prepend_widget(&mut self, widget: T) {
        self.widgets.insert(0, widget);
        self.resize_children();
        self.refresh().log();
    }

    pub fn calculate_coordinates(&self, widget: &T)
                                 -> Coordinates where T: PartialEq  {
        let coordinates = self.get_coordinates().unwrap();
        let xsize = coordinates.xsize();
        let ysize = coordinates.ysize();
        let top = coordinates.top().y();

        let pos = self.widgets.iter().position(|w | w == widget).unwrap();
        let num = self.widgets.len();

        let widget_xsize = (xsize / num as u16) + 1;
        let widget_xpos = widget_xsize * pos as u16;

        Coordinates {
            size: Size((widget_xsize,
                        ysize)),
            position: Position((widget_xpos,
                                top))
        }
    }

    pub fn active_widget(&self) -> &T {
        &self.widgets.last().unwrap()
    }

}




impl<T> Widget for HBox<T> where T: Widget + PartialEq {
    fn get_core(&self) -> HResult<&WidgetCore> {
        Ok(&self.core)
    }
    fn get_core_mut(&mut self) -> HResult<&mut WidgetCore> {
        Ok(&mut self.core)
    }
    fn render_header(&self) -> HResult<String> {
        self.active_widget().render_header()
    }

    fn refresh(&mut self) -> HResult<()> {
        self.resize_children();
        for child in &mut self.widgets {
            child.refresh()?
        }
        Ok(())
    }

    fn get_drawlist(&self) -> HResult<String> {
        Ok(self.widgets.iter().map(|child| {
            child.get_drawlist().unwrap()
        }).collect())
    }

    fn on_event(&mut self, event: Event) -> HResult<()> {
        self.widgets.last_mut()?.on_event(event)?;
        Ok(())
    }
}
