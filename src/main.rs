#![feature(vec_remove_item)]
#![feature(trivial_bounds)]

extern crate termion;
extern crate unicode_width;
#[macro_use]
extern crate lazy_static;
extern crate failure;
#[macro_use]
extern crate failure_derive;
extern crate alphanumeric_sort;
extern crate dirs_2;
extern crate lscolors;
extern crate users;
extern crate chrono;
extern crate mime_detective;
extern crate rayon;
extern crate libc;

use termion::input::MouseTerminal;
use termion::raw::IntoRawMode;
use termion::screen::AlternateScreen;

use std::io::{stdout, Write};

mod coordinates;
mod file_browser;
mod files;
mod listview;
mod miller_columns;
mod preview;
mod term;
mod textview;
mod widget;
mod win_main;
mod window;
mod hbox;
mod tabview;
mod async_widget;
mod fail;

use window::Window;


fn main() {
    let bufout = std::io::BufWriter::new(std::io::stdout());
    // Need to do this here to actually turn terminal into raw mode...
    let mut _screen = AlternateScreen::from(Box::new(bufout));
    let mut _stdout = MouseTerminal::from(stdout().into_raw_mode().unwrap());


    let filebrowser = crate::file_browser::FileBrowser::new().unwrap();
    let mut tabview = crate::tabview::TabView::new();
    tabview.push_widget(filebrowser);

    let mut win = Window::new(tabview);
    win.draw();
    win.handle_input();

    write!(_stdout, "{}", termion::cursor::Show).unwrap();
}
