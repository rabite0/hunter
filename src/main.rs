extern crate termion;
extern crate unicode_width;
#[macro_use]
extern crate lazy_static;

use std::io::{stdout, Write};


use termion::screen::AlternateScreen;

use termion::input::MouseTerminal;
use termion::raw::IntoRawMode;

mod term;
mod window;
mod listview;
mod files;
mod win_main;
mod widget;
mod hbox;

use listview::ListView;
use window::Window;
use hbox::HBox;


fn main() {
    // Need to do this here to actually turn terminal into raw mode...
    let mut _screen = AlternateScreen::from(Box::new(stdout()));
    let mut _stdout = MouseTerminal::from(stdout().into_raw_mode().unwrap());

    let files = files::get_files("/home/project/code").unwrap();
    let listview = ListView::new(files, (50,50), (10,10));

    let files = files::get_files("/home/project/code").unwrap();
    let listview2 = ListView::new(files, (50,50), (80,10));

    let boxed = vec![listview.to_trait(), listview2.to_trait()];

    let hbox = HBox::new(boxed);

    let mut win = Window::new(hbox);
    win.handle_input();

    write!(_stdout, "{}", termion::cursor::Show).unwrap();
}
