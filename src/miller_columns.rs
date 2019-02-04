use termion::event::Key;

use crate::coordinates::{Coordinates, Position, Size};
use crate::preview::Previewer;
use crate::widget::Widget;
use crate::hbox::HBox;

#[derive(PartialEq)]
pub struct MillerColumns<T> where T: Widget {
    pub widgets: HBox<T>,
    // pub left: Option<T>,
    // pub main: Option<T>,
    pub preview: Previewer,
    pub ratio: (u16, u16, u16),
    pub coordinates: Coordinates,
}

impl<T> MillerColumns<T>
where
    T: Widget,
{
    pub fn new() -> Self {
        Self {
            widgets: HBox::new(),
            coordinates: Coordinates::new(),
            ratio: (33, 33, 33),
            preview: Previewer::new(),
        }
    }

    pub fn push_widget(&mut self, widget: T) {
        self.widgets.push_widget(widget);
        self.refresh();
    }

    pub fn pop_widget(&mut self) -> Option<T> {
        let widget = self.widgets.pop_widget();
        self.refresh();
        widget
    }

    pub fn prepend_widget(&mut self, widget: T) {
        self.widgets.prepend_widget(widget);
    }

    pub fn calculate_coordinates(&self) -> (Coordinates, Coordinates, Coordinates) {
        let xsize = self.coordinates.xsize();
        let ysize = self.coordinates.ysize();
        let top = self.coordinates.top().x();
        let ratio = self.ratio;

        let left_xsize = xsize * ratio.0 / 100;
        let left_size = Size((left_xsize, ysize));
        let left_pos = self.coordinates.top();

        let main_xsize = xsize * ratio.1 / 100;
        let main_size = Size((main_xsize, ysize));
        let main_pos = Position((left_xsize + 2, top));

        let preview_xsize = xsize * ratio.2 / 100;
        let preview_size = Size((preview_xsize, ysize));
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

    pub fn get_left_widget(&self) -> Option<&T> {
        let len = self.widgets.widgets.len();
        if len < 2 {
            return None;
        }
        self.widgets.widgets.get(len - 2)
    }
    pub fn get_left_widget_mut(&mut self) -> Option<&mut T> {
        let len = self.widgets.widgets.len();
        if len < 2 {
            return None;
        }
        self.widgets.widgets.get(len - 2)?.get_position();
        self.widgets.widgets.get_mut(len - 2)
    }
    pub fn get_main_widget(&self) -> &T {
        self.widgets.widgets.last().unwrap()
    }
    pub fn get_main_widget_mut(&mut self) -> &mut T {
        self.widgets.widgets.last_mut().unwrap()
    }
}

impl<T> Widget for MillerColumns<T>
where
    T: Widget,
{
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
    fn render_header(&self) -> String {
        "".to_string()
    }
    fn refresh(&mut self) {
        let (left_coords, main_coords, preview_coords) = self.calculate_coordinates();

        if let Some(left_widget) = self.get_left_widget_mut() {
            left_widget.set_coordinates(&left_coords);
        }

        let main_widget = self.get_main_widget_mut();
        main_widget.set_coordinates(&main_coords);

        let preview_widget = &mut self.preview;
        preview_widget.set_coordinates(&preview_coords);
    }

    fn get_drawlist(&self) -> String {
        let left_widget = match self.get_left_widget() {
            Some(widget) => widget.get_drawlist(),
            None => "".into(),
        };
        let main_widget = self.get_main_widget().get_drawlist();
        let preview = self.preview.get_drawlist();
        format!("{}{}{}", main_widget, left_widget, preview)
    }

    fn on_key(&mut self, key: Key) {
        self.get_main_widget_mut().on_key(key);
    }
}
