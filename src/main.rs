#![feature(vec_remove_item)]
#![feature(trivial_bounds)]
#![feature(try_trait)]

extern crate termion;
extern crate unicode_width;
#[macro_use]
extern crate lazy_static;
extern crate failure;
extern crate failure_derive;
extern crate alphanumeric_sort;
extern crate dirs_2;
extern crate lscolors;
extern crate users;
extern crate chrono;
extern crate mime_detective;
extern crate rayon;
extern crate libc;
extern crate notify;

use failure::Fail;

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
mod hbox;
mod tabview;
mod async_widget;
mod fail;
mod minibuffer;
mod proclist;
mod bookmarks;
mod paths;





use widget::{Widget, WidgetCore};
use term::ScreenExt;
use fail::HResult;
use file_browser::FileBrowser;
use tabview::TabView;

fn main() -> HResult<()> {
    match run() {
        Ok(_) => Ok(()),
        Err(err) => {
            eprintln!("{:?}\n{:?}", err, err.cause());
            return Err(err);
        }
    }
}

fn run() -> HResult<()> {
    // do this early so it might be ready when needed
    crate::files::load_tags()?;


    let bufout = std::io::BufWriter::new(std::io::stdout());
    // Need to do this here to actually turn terminal into raw mode...
    let mut screen = AlternateScreen::from(bufout);
    let mut _stdout = MouseTerminal::from(stdout().into_raw_mode()?);
    screen.cursor_hide()?;
    screen.clear()?;
    screen.flush()?;

    let core = WidgetCore::new()?;

    let filebrowser = FileBrowser::new_cored(&core)?;
    let mut tabview = TabView::new(&core);
    tabview.push_widget(filebrowser)?;

    tabview.handle_input()?;

    screen.cursor_show()?;
    screen.flush()?;

    Ok(())
}
