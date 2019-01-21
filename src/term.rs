use unicode_width::UnicodeWidthStr;

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

pub fn size() ->  (u16, u16) {
    termion::terminal_size().unwrap()
}

pub fn xsize() -> usize {
    let (xsize, _) = termion::terminal_size().unwrap();
    xsize as usize
}

pub fn ysize() -> usize {
    let (_, ysize) = termion::terminal_size().unwrap();
    ysize as usize
}

pub fn sized_string(string: &str, xsize: u16) -> String {
    let lenstr: String = string.chars().fold("".into(), |acc,ch| {
            if acc.width() + 1  >= xsize as usize { acc }
            else { acc + &ch.to_string() }
    });
    lenstr
}

// Do these as constants


pub fn highlight_color() -> String {
    format!(
        "{}{}",
        termion::color::Fg(termion::color::LightGreen),
        termion::color::Bg(termion::color::Black)
    )
}

pub fn normal_color() -> String {
    format!(
        "{}{}",
        termion::color::Fg(termion::color::LightBlue),
        termion::color::Bg(termion::color::Black)
    )
}


pub fn cursor_left(n: usize) -> String {
    format!("{}", termion::cursor::Left(n as u16))
}

pub fn gotoy(y: usize) -> String {
    format!("{}", termion::cursor::Goto(1, y as u16))
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
