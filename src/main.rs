extern crate termion;
extern crate unicode_width;
#[macro_use]
extern crate lazy_static;
extern crate alphanumeric_sort;
extern crate lscolors;

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
//mod hbox;
mod miller_columns;
mod coordinates;

use listview::ListView;
use window::Window;
//use hbox::HBox;
use miller_columns::MillerColumns;
use widget::Widget;
use coordinates::{Coordinates,Size,Position};
mod file_browser;
use file_browser::FileBrowser;

fn main() {
    // Need to do this here to actually turn terminal into raw mode...
    let mut _screen = AlternateScreen::from(Box::new(stdout()));
    let mut _stdout = MouseTerminal::from(stdout().into_raw_mode().unwrap());

    let (xsize, ysize) = term::size();
    let ysize = ysize - 1;

    let coordinates = Coordinates { size: Size ((xsize, ysize - 1)) ,
                                    position: Position( (1, 2 )) };
                                    

    let files = files::Files::new_from_path("/home/project/").unwrap();
    let mut listview = ListView::new(files);

    let files = files::Files::new_from_path("/home/project/Downloads/").unwrap();
     let mut listview2 = ListView::new(files);

    let files = files::Files::new_from_path("/home/").unwrap();
    let mut listview3 = ListView::new(files);

    // let files = files::Files::new_from_path("/").unwrap();
    // let mut listview4 = ListView::new(files);

    // listview.set_size( Size ((20,30)));
    // listview.set_position(Position((160,1)));

    // listview2.set_size( Size ((20,30)));
    // listview2.set_position(Position((160,1)));

    // listview3.set_size( Size ((20,30)));
    // listview3.set_position(Position((160,1)));

    // listview4.set_size( Size ((20,30)));
    // listview4.set_position(Position((160,1)));
    

    // listview2.set_dimensions((95,53));
    // listview2.set_position((95,1));

    // let boxed = vec![listview.to_trait(), listview2.to_trait()];

    // let hbox = HBox::new(boxed, (xsize, ysize-1), (1,2) , 0);

    

    let mut miller
        = MillerColumns::new(vec![listview3,listview,listview2],
                             coordinates,
                             (33, 33, 33));

    // miller.main = Some(listview2);
    // miller.left = Some(listview3);
    // miller.preview = Some(listview4);

    let coords = dbg!(miller.calculate_coordinates());


    miller.refresh();
    let filebrowser = crate::file_browser::FileBrowser { columns: miller };
    
    let mut win = Window::new(filebrowser);
    win.handle_input();

    write!(_stdout, "{}", termion::cursor::Show).unwrap();
    }
