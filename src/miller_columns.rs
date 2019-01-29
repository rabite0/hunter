use termion::event::{Key,Event};


use crate::widget::Widget;
use crate::files::Files;
//use crate::hbox::HBox;
use crate::listview::ListView;
use crate::coordinates::{Coordinates, Size,Position};
use crate::files::File;

pub struct MillerColumns<T> {
    pub widgets: Vec<T>,
    // pub left: Option<T>,
    // pub main: Option<T>,
    // pub preview: Option<T>,
    ratio: (u16,u16,u16),
    coordinates: Coordinates,
}





impl<T> MillerColumns<T> where T: Widget {
    pub fn new(widgets: Vec<T>,
               coordinates: Coordinates, 
               ratio: (u16, u16, u16))
               -> Self { Self { widgets: widgets,
                                coordinates: coordinates,
                                ratio: ratio } }       


    pub fn push_widget(&mut self, widget: T) {
        
    }

    pub fn calculate_coordinates(&self) -> (Coordinates, Coordinates, Coordinates) {
        let xsize = self.coordinates.xsize();
        let ysize = self.coordinates.ysize();
        let top = self.coordinates.top().x();
        let ratio = self.ratio;
        
        let left_xsize = xsize * ratio.0 / 100;
        let left_size = Size ((left_xsize, ysize));
        let left_pos = self.coordinates.top();
        

        let main_xsize = xsize * ratio.1 / 100;
        let main_size = Size ( (main_xsize, ysize) );
        let main_pos = Position ( (left_xsize + 2,  top ));

        let preview_xsize = xsize * ratio.2 / 100;
        let preview_size = Size ( (preview_xsize, ysize) );
        let preview_pos = Position ( (left_xsize + main_xsize + 2, top) );

        let left_coords = Coordinates { size: left_size,
                                        position: left_pos };
                                        
        
        let main_coords = Coordinates { size: main_size,
                                        position: main_pos };
                                        
        
        let preview_coords = Coordinates { size: preview_size,
                                           position: preview_pos };
                                           

        (left_coords, main_coords, preview_coords)
    }

    pub fn get_left_widget(&self) -> Option<&T> {
        let len = self.widgets.len();
        self.widgets.get(len-2)
    }
    pub fn get_left_widget_mut(&mut self) -> Option<&mut T> {
        let len = self.widgets.len();
        dbg!((self.widgets[len-2]).get_position());
        Some(&mut self.widgets[len-2])
    } 
    pub fn get_main_widget(&self) -> Option<&T> {
        self.widgets.last()
    }
    pub fn get_main_widget_mut(&mut self) -> Option<&mut T> {
        self.widgets.last_mut()
    }

    
}

impl<T> Widget for MillerColumns<T> where T: Widget {
    fn render(&self) -> Vec<String> {
        vec![]
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
    fn render_header(&self) -> String {
        "".to_string()
    }
    fn refresh(&mut self) {
        let (left_coords, main_coords, preview_coords) = self.calculate_coordinates(); 
        
        // self.left.as_mut().unwrap().set_size(left_coords.size);
        // self.left.as_mut().unwrap().set_position(left_coords.position);

        // self.get_main_widget_mut().map(|widget| {
        //     widget.set_size(main_coords.size);
        //     widget.set_position(main_coords.position);
        // });

        
        let (left_coords, main_coords, preview_coords) = self.calculate_coordinates();  

        let widget2 = self.get_left_widget_mut().unwrap();
        widget2.set_size( left_coords.size );
        widget2.set_position( left_coords.position );
        widget2.refresh();
        

        let widget = self.get_main_widget_mut().unwrap();
        widget.set_size(main_coords.size);
        widget.set_position(main_coords.position);
        widget.refresh();
        
        
        // self.main.as_mut().unwrap().set_size(main_coords.size);
        // self.main.as_mut().unwrap().set_position(main_coords.position);

        // self.preview.as_mut().unwrap().set_size(preview_coords.size);
        // self.preview.as_mut().unwrap().set_position(preview_coords.position);

        // self.left.as_mut().unwrap().refresh();
        // self.main.as_mut().unwrap().refresh();
        // self.preview.as_mut().unwrap().refresh()
    }
    
    fn get_drawlist(&self) -> String {
        let left_widget = self.get_left_widget().unwrap().get_drawlist();
        let main_widget = self.get_main_widget().unwrap().get_drawlist();
        format!("{}{}", main_widget, left_widget)
        // let left_drawlist = &self.left.as_ref().unwrap().get_drawlist();
        // let main_drawlist = &self.main.as_ref().unwrap().get_drawlist();
        // let preview_drawlist = &self.preview.as_ref().unwrap().get_drawlist();
        
        // format!("{}{}{}", left_drawlist, &main_drawlist, &preview_drawlist)
        // let main_widget_drawlist = self.get_main_widget().map(|widget| {
            
        //     widget.get_drawlist()
        // });

        
        // match main_widget_drawlist {
        //     Some(drawlist) => { drawlist },
        //     None => "Can't draw this".to_string()
        // }
    
    }

    fn on_key(&mut self, key: Key) {
        match key {
            _ => {
                self.refresh();
                self.get_main_widget_mut().unwrap().on_key(key);
                //self.set_left_directory();
                self.refresh();
            },
            //_ => { self.bad(Event::Key(key)); }
        }
    }
// }

// impl MillerColumns<ListView<Files>>
// {
    // pub fn godir(&mut self) -> Result<(),Box<dyn std::error::Error>> {
    //     let current_dir = self.widgets.iter().last().unwrap();
    //     let selected_path = &current_dir.selected_file().path;
    //     let files = Files::new_from_path(selected_path)?;
    //     let dir_list = ListView::new(files);
    //     Ok(())
    // }
    // fn set_left_directory(&mut self, dir: File) {
    //     let parent_dir = self.main.as_ref().unwrap().grand_parent().unwrap();
    //     self.left.as_mut().unwrap().goto_path(&parent_dir);
    // }
}

