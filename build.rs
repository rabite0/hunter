extern crate rustc_version;
extern crate termion;
// use std::process::Command;

fn main() -> Result<(), ()> {
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
