use termion::event::{Event};

use crate::widget::Widget;
use crate::coordinates::{Coordinates, Size, Position};

#[derive(PartialEq)]
pub struct HBox<T: Widget> {
    pub coordinates: Coordinates,
    pub widgets: Vec<T>,
    pub active: Option<usize>,
}


impl<T> HBox<T> where T: Widget {
    pub fn new() -> HBox<T> {
        HBox { coordinates: Coordinates::new(),
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
            widget.set_coordinates(coord);
        }
    }

    pub fn push_widget(&mut self, widget: T) where T: PartialEq {
        self.widgets.push(widget);
        self.resize_children();
        self.refresh();
    }

    pub fn pop_widget(&mut self) -> Option<T> {
        let widget = self.widgets.pop();
        self.resize_children();
        self.refresh();
        widget
    }

    pub fn prepend_widget(&mut self, widget: T) {
        self.widgets.insert(0, widget);
        self.resize_children();
        self.refresh();
    }

    pub fn calculate_coordinates(&self, widget: &T) 
                                 -> Coordinates where T: PartialEq  {
        let xsize = self.coordinates.xsize();
        let ysize = self.coordinates.ysize();
        let top = self.coordinates.top().x();

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




impl<T> Widget for HBox<T> where T: Widget {
    fn render_header(&self) -> String {
        self.active_widget().render_header()
    }

    fn refresh(&mut self) {
        self.resize_children();
        for child in &mut self.widgets {
            child.refresh();
        }
    }

    fn get_drawlist(&self) -> String {
        self.widgets.iter().map(|child| {
            child.get_drawlist()
        }).collect()
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
    fn on_event(&mut self, event: Event) {
        self.widgets.last_mut().unwrap().on_event(event);
    }
}
