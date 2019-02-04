extern crate termion;
extern crate unicode_width;
#[macro_use]
extern crate lazy_static;
extern crate alphanumeric_sort;
extern crate dirs_2;
extern crate lscolors;
extern crate mime_detective;
extern crate rayon;

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

use window::Window;

fn main() {
    // Need to do this here to actually turn terminal into raw mode...
    let mut _screen = AlternateScreen::from(Box::new(stdout()));
    let mut _stdout = MouseTerminal::from(stdout().into_raw_mode().unwrap());

    

    let filebrowser = crate::file_browser::FileBrowser::new().unwrap();

    let mut win = Window::new(filebrowser);
    win.handle_input();

    write!(_stdout, "{}", termion::cursor::Show).unwrap();
}
