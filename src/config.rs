use lazy_static;
use clap;

use std::sync::RwLock;

use crate::paths;

use crate::fail::{HError, HResult, ErrorLog};

#[derive(Clone)]
// These are options, so we know if they have been set or not
struct ArgvConfig {
    animation: Option<bool>,
    show_hidden: Option<bool>,
    icons: Option<bool>
}

impl ArgvConfig {
    fn new() -> Self {
        ArgvConfig {
            animation: None,
            show_hidden: None,
            icons: None
        }
    }
}

lazy_static! {
    static ref ARGV_CONFIG: RwLock<ArgvConfig>  = RwLock::new(ArgvConfig::new());
}


pub fn set_argv_config(args: clap::ArgMatches) -> HResult<()> {
    let animation = args.is_present("animation-off");
    let show_hidden = args.is_present("show-hidden");
    let icons = args.is_present("icons");

    let mut config = ArgvConfig::new();

    if animation == true {
        config.animation = Some(false);
    }

    if show_hidden == true {
        config.show_hidden = Some(true);
    }

    if icons == true {
        config.icons = Some(true)
    }

    *ARGV_CONFIG.write()? = config;
    Ok(())
}

fn get_argv_config() -> HResult<ArgvConfig> {
        Ok(ARGV_CONFIG.try_read()?.clone())
}

fn infuse_argv_config(mut config: Config) -> Config {
    let argv_config = get_argv_config().unwrap_or(ArgvConfig::new());

    argv_config.animation.map(|val| config.animation = val);
    argv_config.show_hidden.map(|val| config.show_hidden = val);
    argv_config.icons.map(|val| config.icons = val);

    config
}

#[derive(Debug, Clone)]
pub struct Config {
    pub animation: bool,
    pub show_hidden: bool,
    pub select_cmd: String,
    pub cd_cmd: String,
    pub icons: bool,
    pub media_autoplay: bool,
    pub media_mute: bool,
}


impl Config {
    pub fn new() -> Config {
        let config = Config::default();

        infuse_argv_config(config)
    }

    pub fn default() -> Config {
        Config {
            animation: true,
            show_hidden: false,
            select_cmd: "find -type f | fzf -m".to_string(),
            cd_cmd: "find -type d | fzf".to_string(),
            icons: false,
            media_autoplay: false,
            media_mute: false
        }
    }

    pub fn load() -> HResult<Config> {
        let config_path = paths::config_path()?;

        if !config_path.exists() {
            return Ok(infuse_argv_config(Config::new()));
        }

        let config_string = std::fs::read_to_string(config_path)?;

        let config = config_string.lines().fold(Config::new(), |mut config, line| {
            match Config::prep_line(line) {
                Ok(("animation", "on")) => { config.animation = true; },
                Ok(("animation", "off")) => { config.animation = false; },
                Ok(("show_hidden", "on")) => { config.show_hidden = true; },
                Ok(("show_hidden", "off")) => { config.show_hidden = false; },
                Ok(("icons", "on")) => config.icons = true,
                Ok(("icons", "off")) => config.icons = false,
                Ok(("select_cmd", cmd)) => {
                    let cmd = cmd.to_string();
                    config.select_cmd = cmd;
                }
                Ok(("cd_cmd", cmd)) => {
                    let cmd = cmd.to_string();
                    config.cd_cmd = cmd;
                }
                Ok(("media_autoplay", "on")) => { config.media_autoplay = true; },
                Ok(("media_autoplay", "off")) => { config.media_autoplay = false; },
                Ok(("media_mute", "on")) => { config.media_mute = true; },
                Ok(("media_mute", "off")) => { config.media_mute = false; },
                _ => { HError::config_error::<Config>(line.to_string()).log(); }
            }
            config
        });

        let config = infuse_argv_config(config);

        Ok(config)
    }

    fn prep_line<'a>(line: &'a str) -> HResult<(&'a str, &'a str)> {
        let setting = line.split("=").collect::<Vec<&str>>();
        if setting.len() == 2 {
            Ok((setting[0], setting[1]))
        } else {
            HError::config_error(line.to_string())
        }

    }

    pub fn animate(&self) -> bool {
        self.animation
    }

    pub fn show_hidden(&self) -> bool {
        self.show_hidden
    }
}
