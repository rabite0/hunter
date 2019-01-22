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
    active: usize
}


impl HBox {
    pub fn new(widgets: Vec<Box<Widget>>,
               dimensions: (u16, u16),
               position: (u16, u16),
               main: usize) -> HBox {
        let mut hbox = HBox {
            dimensions: dimensions,
            position: position,
            children: widgets,
            active: main
        };
        hbox.resize_children();
        hbox
        }


    pub fn resize_children(&mut self) {
        let hbox_size = dbg!(self.dimensions);
        let hbox_position = dbg!(self.position);
        let cell_size = dbg!(hbox_size.0 / self.children.len() as u16);
        let mut current_pos = dbg!(hbox_position.1);
        
        for widget in &mut self.children {
            widget.set_dimensions(dbg!((cell_size, hbox_size.1)));
            widget.set_position(dbg!((current_pos, hbox_position.1)));
            widget.refresh();
            dbg!(current_pos += cell_size);
        }
    }

    // pub fn widget(&self, index: usize) -> &Box<Widget> {
    //     &self.children[index]
    // }

    pub fn active_widget(&self) -> &Box<Widget> {
        &self.children[self.active]
    }

}




impl Widget for HBox {
    fn render(&self) -> Vec<String> {
        // HBox doesnt' draw anything itself
        vec![]
    }

    fn render_header(&self) -> String {
        self.active_widget().render_header()
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
