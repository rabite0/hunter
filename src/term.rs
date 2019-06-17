use std::io::{Stdout, Write, BufWriter, BufRead};
use std::sync::{Arc, Mutex, RwLock};

use termion;
use termion::screen::AlternateScreen;
use termion::raw::{IntoRawMode, RawTerminal};

use parse_ansi::parse_bytes;
use crate::unicode_width::{UnicodeWidthStr, UnicodeWidthChar};

use crate::fail::{HResult, ErrorLog};
use crate::trait_ext::ExtractResult;

pub type TermMode = AlternateScreen<RawTerminal<BufWriter<Stdout>>>;

#[derive(Clone)]
pub struct Screen {
    screen: Arc<Mutex<TermMode>>,
    size: Arc<RwLock<Option<(usize, usize)>>>,
    terminal: String
}

impl Screen {
    pub fn new() -> HResult<Screen> {
        let screen = BufWriter::new(std::io::stdout()).into_raw_mode()?;
        let mut screen = AlternateScreen::from(screen);
        let terminal = std::env::var("TERM").unwrap_or("xterm".into());

        screen.cursor_hide()?;
        Ok(Screen {
            screen: Arc::new(Mutex::new(screen)),
            size: Arc::new(RwLock::new(None)),
            terminal: terminal
        })
    }

    pub fn set_size(&self, size: (usize, usize)) -> HResult<()> {
        *self.size.write()? = Some(size);
        Ok(())
    }

    pub fn is_resized(&self) -> HResult<bool> {
        Ok(self.size.read()?.is_some())
    }

    pub fn get_size(&self) -> HResult<(usize, usize)> {
        match self.size.read()?.clone() {
            Some((xsize, ysize)) => Ok((xsize, ysize)),
            None => Ok((self.xsize()?, self.ysize()?))
        }
    }

    pub fn take_size(&self) -> HResult<(usize, usize)> {
        Ok(self.size.write()?.take()?)
    }

    pub fn set_title(&mut self, title: &str) -> HResult<()> {
        if self.terminal.starts_with("xterm") ||
            self.terminal.starts_with("screen") ||
            self.terminal.starts_with("tmux"){
             write!(self, "\x1b]2;hunter: {}\x1b\\", title)?;
        }
        if self.terminal.starts_with("tmux") {
            write!(self, "\x1bkhunter: {}\x1b\\", title)?;
        }
        Ok(())
    }
}

impl Write for Screen {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.screen
            .lock()
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other,
                                             "Screen Mutex poisoned!"))
            .and_then(|mut s| s.write(buf))
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.screen
            .lock()
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other,
                                             "Screen Mutex poisoned!"))
            .and_then(|mut s| s.flush())
    }
}

pub trait ScreenExt: Write {
    fn suspend_raw_mode(&mut self) -> HResult<()>;
    fn activate_raw_mode(&mut self) -> HResult<()>;
    fn suspend(&mut self) -> HResult<()> {
        self.cursor_show().log();
        self.suspend_raw_mode().log();
        self.to_main_screen()
    }
    fn activate(&mut self) -> HResult<()> {
        self.cursor_hide().log();
        self.activate_raw_mode().log();
        self.to_alternate_screen()
    }
    fn cursor_hide(&mut self) -> HResult<()> {
        write!(self, "{}", termion::cursor::Hide)?;
        self.flush()?;
        Ok(())
    }
    fn cursor_show(&mut self) -> HResult<()> {
        write!(self, "{}", termion::cursor::Show)?;
        self.flush()?;
        Ok(())
    }
    fn reset(&mut self) -> HResult<()> {
        write!(self, "{}", termion::style::Reset)?;
        Ok(())
    }
    fn clear(&mut self) -> HResult<()> {
        write!(self, "{}{}",
               termion::style::Reset,
               termion::clear::All)?;
        Ok(())
    }
    fn write_str(&mut self, str: &str) -> HResult<()> {
        write!(self, "{}", str)?;
        Ok(())
    }
    fn goto_xy(&mut self, x: usize, y: usize) -> HResult<()> {
        let x = x as u16;
        let y = y as u16;
        write!(self, "{}", goto_xy(x + 1, y + 1))?;
        Ok(())
    }
    fn size(&self) -> HResult<(usize, usize)> {
        let (xsize, ysize) = termion::terminal_size()?;
        Ok(((xsize-1) as usize, (ysize-1) as usize))
    }
    fn xsize(&self) -> HResult<usize> {
        let (xsize, _) = termion::terminal_size()?;
        Ok((xsize - 1) as usize)
    }
    fn ysize(&self) -> HResult<usize> {
        let (_, ysize) = termion::terminal_size()?;
        Ok((ysize - 1) as usize)
    }
    fn to_main_screen(&mut self) -> HResult<()> {
        write!(self, "{}", termion::screen::ToMainScreen)?;
        self.flush()?;
        Ok(())
    }
    fn to_alternate_screen(&mut self) -> HResult<()> {
        write!(self, "{}", termion::screen::ToAlternateScreen)?;
        self.flush()?;
        Ok(())
    }
}

impl ScreenExt for Screen {
    fn suspend_raw_mode(&mut self) -> HResult<()> {
        self.screen
            .lock()?
            .suspend_raw_mode()
    }

    fn activate_raw_mode(&mut self) -> HResult<()> {
        self.screen
            .lock()?
            .activate_raw_mode()
    }
}

impl ScreenExt for TermMode {
    fn suspend_raw_mode(&mut self) -> HResult<()> {
        Ok(RawTerminal::suspend_raw_mode(self)?)
    }

    fn activate_raw_mode(&mut self) -> HResult<()> {
        Ok(RawTerminal::activate_raw_mode(self)?)
    }
}

pub fn flush_stdin() {
    let stdin = std::io::stdin();
    let mut stdin = stdin.lock();

    // Not 100% sure if it's OK to just call consume like this
    stdin.consume(10);
}

pub fn xsize() -> u16 {
    let (xsize, _) = termion::terminal_size().unwrap();
    xsize
}

pub fn xsize_u() -> usize {
    let (xsize, _) = termion::terminal_size().unwrap();
    xsize as usize - 1
}

pub fn ysize() -> u16 {
    let (_, ysize) = termion::terminal_size().unwrap();
    ysize
}

pub fn size() -> HResult<(usize, usize)> {
    let (xsize, ysize) = termion::terminal_size()?;
    Ok(((xsize-1) as usize, (ysize-1) as usize))
}

pub fn sized_string(string: &str, xsize: u16) -> String {
    string.chars().fold("".to_string(), |acc, ch| {
        let width: usize = unicode_width::UnicodeWidthStr::width(acc.as_str());
        if width + 1 >= xsize as usize {
            acc
        } else {
            acc + &ch.to_string()
        }
    })
}

#[derive(Debug)]
enum Token<'a> {
    Text(&'a str),
    Ansi(&'a str)
}

fn get_tokens(string: &str) -> Vec<Token> {
    let mut tokens = parse_bytes(string.as_bytes())
        .fold((Vec::new(), 0), |(mut tokens, last_tok), ansi_pos| {
            if last_tok == 0 {
                // first iteration
                if ansi_pos.start() != 0 {
                    // there is text before first ansi code
                    tokens.push(Token::Text(&string[0..ansi_pos.start()]));
                }
                tokens.push(Token::Ansi(&string[ansi_pos.start()..ansi_pos.end()]));
                (tokens, ansi_pos.end())
            } else if last_tok == ansi_pos.start() {
                // next token is another ansi code
                tokens.push(Token::Ansi(&string[ansi_pos.start()..ansi_pos.end()]));
                (tokens, ansi_pos.end())
            } else {
                // there is text before the next ansi code
                tokens.push(Token::Text(&string[last_tok..ansi_pos.start()]));
                tokens.push(Token::Ansi(&string[ansi_pos.start()..ansi_pos.end()]));
                (tokens, ansi_pos.end())
            }
        });

    // last part is just text, add it to tokens
    if string.len() > tokens.1 {
        tokens.0.push(Token::Text(&string[tokens.1..string.len()]));
    }

    tokens.0
}


pub fn sized_string_u(string: &str, xsize: usize) -> String {
    let tokens = get_tokens(&string);

    let sized = tokens.iter().try_fold((String::new(), 0), |(mut sized, width), token| {
        let (tok, tok_width) = match token {
            Token::Text(text) => {
                let tok_str = text;
                let tok_width = text.width();
                (tok_str, tok_width)
            },
            Token::Ansi(ansi) => (ansi, 0)
        };

        // adding this token makes string larger than xsise
        if width + tok_width > xsize {
            let chars_left = xsize + 1 - width;

            // fill up with chars from token until xsize is reached
            let fillup = tok.chars().try_fold((String::new(), 0),
                                              |(mut fillup, fillup_width), chr| {
                let chr_width = chr.width().unwrap_or(0);

                if fillup_width + chr_width > chars_left {
                    Err((fillup, fillup_width))
                } else {
                    fillup.push(chr);
                    Ok((fillup, fillup_width + chr_width))
                }
            });

            let (fillup, fillup_width) = fillup.extract();
            sized.push_str(&fillup);

            // we're done here, stop looping
            Err((sized, width + fillup_width))
        } else {
            sized.push_str(&tok);
            Ok((sized, width + tok_width))
        }

    });


    let (mut sized_str, sized_width) = sized.extract();

    // pad out string
    if sized_width < xsize {
        let padding = xsize-sized_width;
        for _ in 0..padding {
            sized_str += " ";
        }
    }


    sized_str
}


// Do these as constants




pub fn highlight_color() -> String {
    format!(
        "{}",
        termion::color::Fg(termion::color::LightGreen),
        //termion::color::Bg(termion::color::Black)
    )
}

pub fn normal_color() -> String {
    format!(
        "{}",
        termion::color::Fg(termion::color::White),
        //termion::color::Bg(termion::color::Black)
    )
}

pub fn color_red() -> String {
    format!("{}", termion::color::Fg(termion::color::Red))
}

pub fn color_yellow() -> String {
    format!("{}", termion::color::Fg(termion::color::Yellow))
}

pub fn color_green() -> String {
    format!("{}", termion::color::Fg(termion::color::Green))
}

pub fn color_light_green() -> String {
    format!("{}", termion::color::Fg(termion::color::LightGreen))
}

pub fn color_cyan() -> String {
    format!("{}", termion::color::Fg(termion::color::Cyan))
}

pub fn color_light_yellow() -> String {
    format!("{}", termion::color::Fg(termion::color::LightYellow))
}

pub fn color_orange() -> String {
    let color = termion::color::Fg(termion::color::AnsiValue::rgb(5 as u8 ,
                                                                  4 as u8,
                                                                  0 as u8));
    format!("{}", color)
}


pub fn from_lscolor(color: &lscolors::Color) -> String {
    match color {
        lscolors::Color::Black
            => format!("{}", termion::color::Fg(termion::color::Black)),
        lscolors::Color::Red
            => format!("{}", termion::color::Fg(termion::color::Red)),
        lscolors::Color::Green
            => format!("{}", termion::color::Fg(termion::color::Green)),
        lscolors::Color::Yellow
            => format!("{}", termion::color::Fg(termion::color::Yellow)),
        lscolors::Color::Blue
            => format!("{}", termion::color::Fg(termion::color::Blue)),
        lscolors::Color::Magenta
            => format!("{}", termion::color::Fg(termion::color::Magenta)),
        lscolors::Color::Cyan
            => format!("{}", termion::color::Fg(termion::color::Cyan)),
        lscolors::Color::White
            => format!("{}", termion::color::Fg(termion::color::White)),
        _ => format!("{}", normal_color()),
    }
}

// pub fn cursor_left(n: u16) -> String {
//     format!("{}", termion::cursor::Left(n))
// }

pub fn gotoy(y: u16) -> String {
    format!("{}", termion::cursor::Goto(1, y))
}

pub fn goto_xy(x: u16, y: u16) -> String {
    format!("{}", termion::cursor::Goto(x, y))
}

pub fn goto_xy_u(x: usize, y: usize) -> String {
    let x = (x+1) as u16;
    let y = (y+1) as u16;
    format!("{}", termion::cursor::Goto(x, y))
}

// pub fn move_top() -> String {
//     gotoy(1)
// }

pub fn move_bottom() -> String {
    gotoy(ysize())
}

pub fn reset() -> String {
    format!("{}", termion::style::Reset)
}

pub fn invert() -> String {
    format!("{}", termion::style::Invert)
}

pub fn cursor_save() -> String {
    format!("{}", termion::cursor::Save)
}

pub fn cursor_restore() -> String {
    format!("{}", termion::cursor::Restore)
}

pub fn header_color() -> String {
    format!(
        "{}{}",
        termion::color::Fg(termion::color::White),
        termion::color::Bg(termion::color::Blue)
    )
}

pub fn status_bg() -> String {
    format!("{}", termion::color::Bg(termion::color::LightBlue))
}
