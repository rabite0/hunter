use systemstat::{System, Platform};
use systemstat::data::Filesystem;

use std::path::{Path, PathBuf, Component};
use std::collections::HashMap;

use crate::fail::{HResult, ErrorLog, HError};

#[derive(Debug,Clone)]
pub struct FsStat {
    pub stats: HashMap<PathBuf, Filesystem>
}

impl FsStat {
    pub fn new() -> HResult<FsStat> {
        let mut stats = FsStat { stats: HashMap::new() };
        stats.refresh().log();

        Ok(stats)
    }

    pub fn refresh(&mut self) -> HResult<()> {
        let sys = System::new();
        let mounts = sys.mounts()?;

        let stats = mounts.into_iter()
            .fold(HashMap::new(), |mut stats, mount: Filesystem| {
                let path = PathBuf::from(&mount.fs_mounted_on);
                stats.insert(path, mount);
                stats
            });

        self.stats = stats;

        Ok(())
    }

    pub fn find_fs(&self, path: &Path) -> HResult<&Filesystem> {
        let candidates = self
            .stats
            .keys()
            .filter(|mount_point| path.starts_with(&mount_point))
            .collect::<Vec<&PathBuf>>();

        let deepest_match = candidates.iter()
            .fold(PathBuf::new(), |mut deepest, path| {
                let curren_path_len = deepest.components().count();
                let candidate_path_len = path.components().count();

                if candidate_path_len >  curren_path_len {
                    deepest = path.to_path_buf();
                }
               deepest
            });
        let fs = self.stats.get(&deepest_match).ok_or_else(|| HError::NoneError)?;
        Ok(fs)
    }
}

pub trait FsExt {
    fn get_dev(&self) -> Option<String>;
    fn get_total(&self) -> String;
    fn get_free(&self) -> String;
}

impl FsExt for Filesystem {
    fn get_dev(&self) -> Option<String> {
        let path = PathBuf::from(&self.fs_mounted_from);
        let dev = path.components().last()?;
        let dev = match dev {
            Component::Normal(dev) => dev.to_string_lossy().to_string() + ": ",
            // zfs on FBSD doesn't return a device path
            _ => "".to_string()
        };
        Some(dev)
    }

    fn get_total(&self) -> String {
        self.total.to_string_as(false)
    }

    fn get_free(&self) -> String {
        self.avail.to_string_as(false)
    }


}
