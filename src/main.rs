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
extern crate parse_ansi;
extern crate signal_notify;

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
mod hbox;
mod tabview;
mod fail;
mod minibuffer;
mod proclist;
mod bookmarks;
mod paths;
mod foldview;
mod dirty;







use widget::{Widget, WidgetCore};
use term::ScreenExt;
use fail::{HResult, HError};
use file_browser::FileBrowser;
use tabview::TabView;
use preview::Async;


fn main() -> HResult<()> {
    // do this early so it might be ready when needed
    crate::files::load_tags().ok();

    let mut core = WidgetCore::new().expect("Can't create WidgetCore!");

    match run(core.clone()) {
        Ok(_) => Ok(()),
        Err(HError::Quit) => {
            core.screen.drop_screen();
            return Ok(())
        },
        Err(err) => {
            core.screen.drop_screen();
            eprintln!("{:?}\n{:?}", err, err.cause());
            return Err(err);
        }
    }
}

fn run(mut core: WidgetCore) -> HResult<()> {
    core.screen.clear()?;

    let filebrowser = FileBrowser::new_cored(&core)?;
    let mut tabview = TabView::new(&core);
    tabview.push_widget(filebrowser)?;

    tabview.handle_input()?;

    core.screen.cursor_show()?;
    core.screen.flush()?;

    Ok(())
}
