#![feature(vec_remove_item)]
#![feature(trivial_bounds)]
#![feature(try_trait)]
#![feature(fnbox)]
#![allow(dead_code)]

extern crate termion;
extern crate unicode_width;
#[macro_use]
extern crate lazy_static;
extern crate alphanumeric_sort;
extern crate chrono;
extern crate dirs_2;
extern crate failure;
extern crate failure_derive;
extern crate libc;
extern crate lscolors;
extern crate notify;
extern crate parse_ansi;
extern crate rayon;
extern crate signal_notify;
extern crate systemstat;
extern crate tree_magic;
extern crate users;

use failure::Fail;

use std::io::Write;

mod bookmarks;
mod config;
mod coordinates;
mod dirty;
mod fail;
mod file_browser;
mod files;
mod foldview;
mod fscache;
mod hbox;
mod listview;
mod miller_columns;
mod minibuffer;
mod paths;
mod preview;
mod proclist;
mod stats;
mod tabview;
mod term;
mod textview;
mod widget;

use fail::{HError, HResult};
use file_browser::FileBrowser;
use tabview::TabView;
use term::ScreenExt;
use widget::{Widget, WidgetCore};

fn main() -> HResult<()> {
    // do this early so it might be ready when needed
    crate::files::load_tags().ok();

    let mut core = WidgetCore::new().expect("Can't create WidgetCore!");

    match run(core.clone()) {
        Ok(_) => Ok(()),
        Err(HError::Quit) => {
            core.screen.drop_screen();
            return Ok(());
        }
        Err(err) => {
            core.screen.drop_screen();
            eprintln!("{:?}\n{:?}", err, err.cause());
            return Err(err);
        }
    }
}

fn run(mut core: WidgetCore) -> HResult<()> {
    core.screen.clear()?;

    let filebrowser = FileBrowser::new(&core, None)?;
    let mut tabview = TabView::new(&core);
    tabview.push_widget(filebrowser)?;

    tabview.handle_input()?;

    core.screen.cursor_show()?;
    core.screen.flush()?;

    Ok(())
}
