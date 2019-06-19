use std::fs::*;
use std::io::Write;
use std::process::Command;
use std::ffi::OsStr;
use std::path::Path;

use crate::fail::{HError, HResult};
use crate::widget::WidgetCore;


pub fn ensure_config(core: WidgetCore) -> HResult<()> {
    if has_config()? {
        let previewers_path = crate::paths::previewers_path()?;
        let actions_path = crate::paths::actions_path()?;

        if !previewers_path.exists() {
            core.show_status("Coulnd't find previewers in config dir! Adding!")?;
            install_config_previewers()
                .or_else(|_|
                         core.show_status("Error installing previewers! Check log!"))?;
        }

        if !actions_path.exists() {
            core.show_status("Coulnd't find actions in config dir! Adding!")?;
            install_config_actions()
                .or_else(|_|
                         core.show_status("Error installing actions! Check log!"))?;
        }

        return Ok(());
    }

    let msg = match install_config_all() {
        Ok(_) => format!("Config installed in: {}",
                         crate::paths::hunter_path()?.to_string_lossy()),
        Err(_) => format!("{}Problems with installation of default configuration! Look inside log.",
                          crate::term::color_red()),
    };
    core.show_status(&msg)?;

    Ok(())
}


fn default_config_archive() -> &'static [u8] {
    let default_config = include_bytes!("../config.tar.gz");
    default_config
}

fn has_config() -> HResult<bool> {
    let config_dir = crate::paths::hunter_path()?;

    if config_dir.exists() {
        return Ok(true);
    } else {
        return Ok(false);
    }
}


fn install_config_all() -> HResult<()> {
    let hunter_dir = crate::paths::hunter_path()?;
    let config_dir = hunter_dir.parent()?;

    if !hunter_dir.exists() {
        // create if non-existing
        std::fs::create_dir(&hunter_dir)
            .or_else(|_| HError::log(&format!("Couldn't create directory: {}",
                                              hunter_dir.as_os_str()
                                                        .to_string_lossy())))?;
    }

    let archive_path = create_archive()?;
    extract_archive(config_dir, &archive_path)?;
    delete_archive(archive_path)?;

    Ok(())
}

fn move_dir(from: &str, to: &Path) -> HResult<()> {
    let success = Command::new("mv")
        .arg(from)
        .arg(to.as_os_str())
        .status()
        .map(|s| s.success());

    if success.is_err() || !success.unwrap() {
        HError::log(&format!("Couldn't move {} to {} !",
                             from,
                             to.to_string_lossy()))
    } else {
        Ok(())
    }
}

fn install_config_previewers() -> HResult<()> {
    let hunter_dir = crate::paths::hunter_path()?;
    let archive_path = create_archive()?;
    extract_archive(Path::new("/tmp"), &archive_path)?;
    move_dir("/tmp/hunter/previewers", &hunter_dir)?;
    delete_archive(&archive_path)
}

fn install_config_actions() -> HResult<()> {
    let hunter_dir = crate::paths::hunter_path()?;
    let archive_path = create_archive()?;
    extract_archive(Path::new("/tmp"), &archive_path)?;
    move_dir("/tmp/hunter/actions", &hunter_dir)?;
    delete_archive(&archive_path)
}

fn create_archive() -> HResult<&'static str> {
    let archive_path = "/tmp/hunter-config.tar.gz";
    let def_config = default_config_archive();

    File::create(archive_path)
        .and_then(|mut f| {
            f.write_all(def_config).map(|_| f)
        })
        .and_then(|mut f| f.flush())
        .or_else(|_| {
            HError::log(&format!("Failed to write config archive to: {}",
                                 archive_path))
        })?;
    Ok(archive_path)
}


fn extract_archive(to: &Path, archive_path: &str) -> HResult<()> {
    let success = Command::new("tar")
        .args(&[OsStr::new("-C"),
                to.as_os_str(),
                OsStr::new("-xf"),
                OsStr::new(archive_path)])
        .status()
        .or_else(|_| HError::log(&format!("Couldn't run tar!")))
        .map(|s| s.success())?;

    if !success {
        HError::log(&format!("Extraction of archive failed! Archive: {}",
                             archive_path))?
    }

    Ok(())
}

fn delete_archive(archive_path: &str) -> HResult<()> {
    std::fs::remove_file(archive_path)
        .or_else(|_| HError::log(&format!("Deletion of archive failed! Archive: {}",
                                          archive_path)))
}
