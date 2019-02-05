use std::io::{Stdout, Write};
use termion;
use termion::screen::AlternateScreen;

pub trait ScreenExt: Write {
    fn cursor_hide(&mut self) {
        write!(self, "{}", termion::cursor::Hide).unwrap();
    }
    fn cursor_show(&mut self) {
        write!(self, "{}", termion::cursor::Show).unwrap();
    }
    fn reset(&mut self) {
        write!(self, "{}", termion::style::Reset).unwrap();
    }
}

impl ScreenExt for AlternateScreen<Box<Stdout>> {}

pub fn xsize() -> u16 {
    let (xsize, _) = termion::terminal_size().unwrap();
    xsize
}

pub fn ysize() -> u16 {
    let (_, ysize) = termion::terminal_size().unwrap();
    ysize
}

pub fn sized_string(string: &str, xsize: u16) -> String {
    string.chars().fold("".to_string(), |acc, ch| {
        let width: usize = unicode_width::UnicodeWidthStr::width_cjk(acc.as_str());
        if width + 1 >= xsize as usize {
            acc
        } else {
            acc + &ch.to_string()
        }
    })
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
        termion::color::Fg(termion::color::LightBlue),
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

pub fn from_lscolor(color: &lscolors::Color) -> String {
    match color {
        lscolors::Color::Black => format!("{}", termion::color::Fg(termion::color::Black)),
        lscolors::Color::Red => format!("{}", termion::color::Fg(termion::color::Red)),
        lscolors::Color::Green => format!("{}", termion::color::Fg(termion::color::Green)),
        lscolors::Color::Yellow => format!("{}", termion::color::Fg(termion::color::Yellow)),
        lscolors::Color::Blue => format!("{}", termion::color::Fg(termion::color::Blue)),
        lscolors::Color::Magenta => format!("{}", termion::color::Fg(termion::color::Magenta)),
        lscolors::Color::Cyan => format!("{}", termion::color::Fg(termion::color::Cyan)),
        lscolors::Color::White => format!("{}", termion::color::Fg(termion::color::White)),
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
