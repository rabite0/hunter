use lazy_static;
use clap;

use std::sync::RwLock;

use crate::paths;

use crate::fail::{HError, HResult, ErrorLog};
use crate::keybind::KeyBinds;


#[derive(Clone)]
// These are options, so we know if they have been set or not
struct ArgvConfig {
    animation: Option<bool>,
    show_hidden: Option<bool>,
    icons: Option<bool>,
    graphics: Option<String>,
}

impl ArgvConfig {
    fn new() -> Self {
        ArgvConfig {
            animation: None,
            show_hidden: None,
            icons: None,
            graphics: None
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

    if let Some(mode) = args.value_of("graphics") {
        if mode == "auto" {
            config.graphics = Some(detect_g_mode());
        } else {
            config.graphics = Some(String::from(mode));
        }
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
    argv_config.graphics.map(|val| config.graphics = val);

    config
}

#[derive(Debug, Clone)]
pub struct Config {
    pub animation: bool,
    pub animation_refresh_frequency: usize,
    pub show_hidden: bool,
    pub select_cmd: String,
    pub cd_cmd: String,
    pub icons: bool,
    pub icons_space: bool,
    pub media_autoplay: bool,
    pub media_mute: bool,
    pub media_previewer: String,
    pub media_previewer_exists: bool,
    pub ratios: Vec::<usize>,
    pub graphics: String,
    pub keybinds: KeyBinds,
}


impl Config {
    pub fn new() -> Config {
        let config = Config::default();

        infuse_argv_config(config)
    }

    pub fn default() -> Config {
        Config {
            animation: true,
            animation_refresh_frequency: 60,
            show_hidden: false,
            select_cmd: "find -type f | fzf -m".to_string(),
            cd_cmd: "find -type d | fzf".to_string(),
            icons: false,
            icons_space: false,
            media_autoplay: false,
            media_mute: false,
            media_previewer: "hunter-media".to_string(),
            media_previewer_exists: false,
            ratios: vec![20,30,49],
            graphics: detect_g_mode(),
            keybinds: KeyBinds::default(),
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
                Ok(("animation", "on")) => config.animation = true,
                Ok(("animation", "off")) => config.animation = false,
                Ok(("animation_refresh_frequency", frequency)) => {
                    match frequency.parse::<usize>() {
                        Ok(parsed_freq) => config.animation_refresh_frequency = parsed_freq,
                        _ => HError::config_error::<Config>(line.to_string()).log()
                    }
                }
                Ok(("show_hidden", "on")) => config.show_hidden = true,
                Ok(("show_hidden", "off")) => config.show_hidden = false,
                Ok(("icons", "on")) => config.icons = true,
                Ok(("icons", "off")) => config.icons = false,
                Ok(("icons_space", "on")) => config.icons_space = true,
                Ok(("icons_space", "off")) => config.icons_space = false,
                Ok(("select_cmd", cmd)) => {
                    let cmd = cmd.to_string();
                    config.select_cmd = cmd;
                }
                Ok(("cd_cmd", cmd)) => {
                    let cmd = cmd.to_string();
                    config.cd_cmd = cmd;
                }
                Ok(("media_autoplay", "on")) => config.media_autoplay = true,
                Ok(("media_autoplay", "off")) => config.media_autoplay = false,
                Ok(("media_mute", "on")) => config.media_mute = true,
                Ok(("media_mute", "off")) => config.media_mute = false,
                Ok(("media_previewer", cmd)) => {
                    let cmd = cmd.to_string();
                    config.media_previewer = cmd;
                },
                Ok(("ratios", ratios)) => {
                    let ratios_str = ratios.to_string();
                    if ratios_str.chars().all(|x| x.is_digit(10) || x.is_whitespace()
                        || x == ':' || x == ',' ) {
                        let ratios: Vec<usize> = ratios_str.split([',', ':'].as_ref())
                            .map(|r| r.trim()
                                 .parse().unwrap())
                            .collect();
                        let ratios_sum: usize = ratios.iter().sum();
                        if ratios.len() == 3 && ratios_sum > 0 &&
                            ratios
                            .iter()
                            .filter(|&r| *r > u16::max_value() as usize)
                            .next() == None {
                                config.ratios = ratios;
                            }
                    }
                }
                #[cfg(feature = "sixel")]
                Ok(("graphics",
                    "sixel")) => config.graphics = "sixel".to_string(),
                Ok(("graphics",
                    "kitty")) => config.graphics = "kitty".to_string(),
                Ok(("graphics",
                    "auto")) => config.graphics = detect_g_mode(),
                _ => { HError::config_error::<Config>(line.to_string()).log(); }
            }

            #[cfg(feature = "img")]
            match has_media_previewer(&config.media_previewer) {
                t @ _ => config.media_previewer_exists = t
            }

            config
        });

        let mut config = infuse_argv_config(config);

        //use std::iter::Extend;
        KeyBinds::load()
            .map(|kb| config.keybinds = kb)
            .log();

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

    pub fn media_available(&self) -> bool {
        self.media_previewer_exists
    }
}

fn detect_g_mode() -> String {
    let term = std::env::var("TERM").unwrap_or(String::new());
    match term.as_str() {
        "xterm-kitty" => "kitty",
        #[cfg(feature = "sixel")]
        "xterm" => "sixel",
        _ => "unicode"
    }.to_string()
}

fn has_media_previewer(name: &str) -> bool {
    use crate::minibuffer::find_bins;
    let previewer = std::path::Path::new(name);
    match previewer.is_absolute() {
        true => previewer.exists(),
        false => find_bins(name).is_ok()
    }
}
