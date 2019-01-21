use crate::widget::Widget;

pub struct HBox {
    dimensions: (u16, u16),
    position: (u16, u16),
    children: Vec<Box<Widget>>,
    main: usize
}

impl HBox {
    pub fn new(widgets: Vec<Box<Widget>>) -> HBox {
        HBox {
            dimensions: (100, 100),
            position: (1, 1),
            children: widgets,
            main: 0
        }
    }
}

impl Widget for HBox {
    fn render(&self) -> Vec<String> {
        // self.children.iter().map(|child| {
        //     child.render()
        // }).collect()
        vec![]                  
    }

    fn render_header(&self) -> String {
        self.children[self.main].render_header()
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
}
