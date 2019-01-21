//use crate::listview::ListView;


// struct MainWindow {
//     active_widget: usize,
//     main: ListView<Files>,
//     parent: ListView<Files>,
//     child: ListView<Files>
// }

// impl Widget for ListView<Files>
// where
//     Files: std::ops::Index<usize>
// {
//     // fn go(&mut self) {
//     //     let pos = self.current_selection();
//     //     let name = &self.content.content[pos].name.clone();
//     //     let path = &self.content.content[pos].path.clone();
//     //     let newfiles = crate::files::get_files(path).unwrap();
        
//     //     let listview = ListView::new(newfiles, (80,80), (10,10));

//     //     let mut win = Window::new(listview);
//     //     win.draw();
//     //     win.handle_input();
//     // }
// }

// impl Renderable for Window<ListView<Files>> {
//     fn get_dimensions(&self) -> (u16, u16) {
//         self.content.get_dimensions()
//     }
//     fn get_position(&self) -> (u16, u16) {
//         self.content.get_position()
//     }
//     fn render(&self) -> Vec<String> {
//         self.content.render()
//     }
//     fn render_header(&self) -> String {
//         self.content.render_header()
//     }
// }

// impl Window<ListView<Files>> {
//     pub fn run(&mut self) {
//         self.draw();
//         self.handle_input();
//     }
// }
