use crate::paths;
use crate::fail::{HError, HResult, ErrorLog};

#[derive(Debug, Clone)]
pub struct Config {
    pub animation: bool,
    pub show_hidden: bool,
    pub select_prog: Option<String>,
}


impl Config {
    pub fn new() -> Config {
        Config {
            animation: true,
            show_hidden: false,
            select_prog: None
        }
    }

    pub fn load() -> HResult<Config> {
        let config_path = paths::config_path()?;

        if !config_path.exists() {
            return Ok(Config::new());
        }

        let config_string = std::fs::read_to_string(config_path)?;

        let config = config_string.lines().fold(Config::new(), |mut config, line| {
            match Config::prep_line(line) {
                Ok(("animation", "on")) => { config.animation = true; },
                Ok(("animation", "off")) => { config.animation = false; },
                Ok(("show_hidden", "on")) => { config.show_hidden = true; },
                Ok(("show_hidden", "off")) => { config.show_hidden = false; },
                Ok(("select_prog", prog)) => {
                    let prog = prog.to_string();
                    config.select_prog = Some(prog);
                }
                _ => { HError::config_error::<Config>(line.to_string()).log(); }
            }
            config
        });
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
