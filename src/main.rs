#![feature(trivial_bounds)]
#![feature(try_trait)]
#![allow(dead_code)]

extern crate termion;
extern crate unicode_width;
#[macro_use]
extern crate lazy_static;
extern crate failure;
extern crate failure_derive;
extern crate natord;
extern crate dirs_2;
extern crate lscolors;
extern crate users;
extern crate chrono;
extern crate rayon;
extern crate libc;
extern crate notify;
extern crate parse_ansi;
extern crate signal_notify;
extern crate tree_magic_fork;
extern crate systemstat;
extern crate mime_guess;
extern crate mime;
extern crate clap;
extern crate strum;
#[macro_use]
extern crate strum_macros;
#[macro_use]
extern crate derivative;
extern crate nix;
extern crate strip_ansi_escapes;
extern crate crossbeam;

extern crate osstrtools;
extern crate pathbuftools;
extern crate async_value;

use failure::Fail;
use clap::{App, Arg};

use std::panic;

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
mod fscache;
mod config;
mod stats;
mod icon;
mod quick_actions;
mod trait_ext;
mod config_installer;
mod imgview;
mod mediaview;
mod keybind;





use widget::{Widget, WidgetCore};
use term::ScreenExt;
use fail::{HResult, HError, MimeError, ErrorLog};
use file_browser::FileBrowser;
use tabview::TabView;
use trait_ext::PathBufMime;


fn reset_screen(core: &mut WidgetCore) -> HResult<()> {
    core.screen.suspend()
}

fn die_gracefully(core: &WidgetCore) {
    let panic_hook = panic::take_hook();
    let core = core.clone();

    panic::set_hook(Box::new(move |info| {
        let mut core = core.clone();
        reset_screen(&mut core).ok();
        panic_hook(info);
    }));
}

fn main() -> HResult<()> {
    let args = parse_args();

    // do this early so it might be ready when needed
    crate::files::load_tags().ok();

    let mut core = WidgetCore::new().expect("Can't create WidgetCore!");

    process_args(args, core.clone());

    // Resets terminal when hunter crashes :(
    die_gracefully(&core);

    match run(core.clone()) {
        Ok(_) | Err(HError::Quit) => reset_screen(&mut core),
        Err(err) => {
            reset_screen(&mut core)?;
            eprintln!("{:?}\n{:?}", err, err.cause());
            return Err(err);
        }
    }
}

fn run(mut core: WidgetCore) -> HResult<()> {
    core.screen.clear()?;

    let core2 = core.clone();

    // I hate waiting!!!
    std::thread::spawn(move || {
        crate::config_installer::ensure_config(core2).log();
    });

    let filebrowser = FileBrowser::new(&core, None)?;
    let mut tabview = TabView::new(&core);
    tabview.push_widget(filebrowser)?;

    tabview.handle_input()?;

    // core.screen.cursor_show()?;
    // core.screen.flush()?;

    Ok(())
}


fn parse_args() -> clap::ArgMatches<'static> {
    App::new(clap::crate_name!())
        .version(clap::crate_version!())
        .author(clap::crate_authors!())
        .about(clap::crate_description!())
        .setting(clap::AppSettings::ColoredHelp)
        .arg(
            Arg::with_name("update")
                .short("u")
                .long("update-conf")
                .help("Update configuration\n(WARNING: Overwrites modified previewers/actions with default names!\nMain config/keys are safe!)")
                .takes_value(false))
        .arg(
            Arg::with_name("animation-off")
                .short("a")
                .long("animation-off")
                .help("Turn off animations")
                .takes_value(false))
        .arg(
            Arg::with_name("show-hidden")
                .short("h")
                .long("show-hidden")
                .help("Show hidden files")
                .takes_value(false),
        )
        .arg(
            Arg::with_name("icons")
                .short("i")
                .long("icons")
                .help("Show icons for different file types")
                .takes_value(false))
        .arg(
            Arg::with_name("graphics")
                .short("g")
                .long("graphics")
                .help("Show HQ graphics using sixel/kitty")
                .takes_value(true))
        // For "Add Action" action
        .arg(
            Arg::with_name("mime")
                .short("m")
                .long("mime")
                .help("Print MIME type of file")
                .takes_value(false))
        .arg(
            Arg::with_name("path")
                .index(1)
                .help("Start in <path>"))
        .get_matches()
}



fn process_args(args: clap::ArgMatches, core: WidgetCore) {
    let path = args.value_of("path");

    // Just print MIME and quit
    if args.is_present("mime") {
        get_mime(path)
            .map_err(|e| eprintln!("{}", e)).
            ok();
        // If we get heres something went wrong.
        std::process::exit(1)
    }

    if args.is_present("update") {
        crate::config_installer::update_config(core, true).log();
    }

    if let Some(path) = path {
        std::env::set_current_dir(&path)
            .map_err(HError::from)
            .log();
    }

    crate::config::set_argv_config(args).log();
}






fn get_mime(path: Option<&str>) -> HResult<()> {
    let path = path.ok_or(MimeError::NoFileProvided)?;
    let path = std::path::PathBuf::from(path);
    path.get_mime()
        .map(|mime| println!("{}", mime))
        .map(|_| std::process::exit(0))
        .map_err(|e| eprintln!("{}", e))
        .map_err(|_| std::process::exit(1))
}
