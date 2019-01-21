use termion::event::{Event};

use crate::widget::Widget;

// pub struct Child<T> {
//     widget: T,
//     position: (u16, u16),
//     size: (u16, u16),
//     active: bool
// }

pub struct HBox {
    dimensions: (u16, u16),
    position: (u16, u16),
    children: Vec<Box<Widget>>,
    main: usize
}

impl HBox {
    pub fn new(widgets: Vec<Box<Widget>>,
               dimensions: (u16, u16),
               position: (u16, u16),
               main: usize) -> HBox {
        HBox {
            dimensions: dimensions,
            position: position,
            children: widgets,
            main: main
        }
    }
}

impl Widget for HBox {
    fn render(&self) -> Vec<String> {
        // HBox doesnt' draw anything itself
        vec![]
    }

    fn render_header(&self) -> String {
        self.children[self.active].render_header()
    }

    fn refresh(&mut self) {
        for child in &mut self.children {
            child.refresh();
        }
    }

    fn get_drawlist(&mut self) -> String {
        self.children.iter_mut().map(|child| {
            child.get_drawlist()
        }).collect()
    }

    fn get_dimensions(&self) -> (u16, u16) {
        self.dimensions
    }
    fn get_position(&self) -> (u16, u16) {
        self.position
    }
    fn set_dimensions(&mut self, size: (u16, u16)) {
        self.dimensions = size;
    }
    fn set_position(&mut self, position: (u16, u16)) {
        self.position = position;
    }


    fn on_event(&mut self, event: Event) {
        self.children[self.active].on_event(event);
    }
}
