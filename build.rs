extern crate termion;
extern crate rustc_version;

use rustc_version::{version_meta, Channel};

// use std::process::Command;


fn main() -> Result<(),()> {
    // Bail out if compiler isn't a nightly
    if let Ok(false) = version_meta().map(|m| m.channel == Channel::Nightly) {
        eprint!("{}", termion::color::Fg(termion::color::Red));
        eprint!("{}", termion::style::Bold);
        eprint!("{}", termion::style::Underline);
        eprintln!("NIGHTLY COMPILER required");
        eprintln!("Please install a nighlty compiler to proceed: https://rustup.rs/");
        eprint!("{}", termion::style::Reset);
        eprintln!("rustup toolchain install nightly");
        eprintln!("source ~/.cargo/env");

        return Err(());
    }

    // crates.io doesn't allow question marks in file names
    // So we just stuff that in an archive for distribution

    // // rename so we can just extract this into config dir later
    // Command::new("cp")
    //     .args("-a extra hunter".split(" "))
    //     .status()
    //     .expect("Can't create copy of extra directory");

    // // create archive that will be included in hunter binary
    // Command::new("tar")
    //     .args("cfz config.tar.gz hunter".split(" "))
    //     .status()
    //     .expect("Failed to create archive of defualt config!");

    // // delete directory we just compressed
    // std::fs::remove_dir_all("hunter")
    //     .expect("Couldn't delete temporary config directory \"hunter\"");

    return Ok(());
}
