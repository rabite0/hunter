use std::io::{Stdout, Write, BufWriter};
use std::sync::{Arc, Mutex};

use termion;
use termion::screen::AlternateScreen;
use termion::input::MouseTerminal;
use termion::raw::{IntoRawMode, RawTerminal};

use parse_ansi::parse_bytes;

use crate::fail::HResult;

pub type TermMode = AlternateScreen<MouseTerminal<RawTerminal<BufWriter<Stdout>>>>;

#[derive(Clone)]
pub struct Screen {
    screen: Arc<Mutex<Option<TermMode>>>
}

impl Screen {
    pub fn new() -> HResult<Screen> {
        let screen = BufWriter::new(std::io::stdout()).into_raw_mode()?;
        let mut screen = MouseTerminal::from(screen);
        let mut screen = AlternateScreen::from(screen);

        screen.cursor_hide()?;
        Ok(Screen {
            screen: Arc::new(Mutex::new(Some(screen)))
        })
    }

    pub fn drop_screen(&mut self) {
        self.cursor_show();
        self.to_main_screen();
        self.screen = Arc::new(Mutex::new(None));

        // Terminal stays fucked without this. Why?
        std::process::Command::new("reset").arg("-I").spawn();
    }

    pub fn reset_screen(&mut self) -> HResult<()> {
        let screen = Screen::new()?.screen.lock()?.take()?;
        *self.screen.lock()? = Some(screen);
        Ok(())
    }
}

impl Write for Screen {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.screen.lock().unwrap().as_mut().unwrap().write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.screen.lock().unwrap().as_mut().unwrap().flush()
    }
}

pub trait ScreenExt: Write {
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
        write!(self, "{}", termion::clear::All)?;
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
    fn xsize(&self) -> HResult<usize> {
        let (xsize, _) = termion::terminal_size()?;
        Ok((xsize - 1) as usize)
    }
    fn ysize(&self) -> HResult<usize> {
        let (_, ysize) = termion::terminal_size()?;
        Ok((ysize - 1) as usize)
    }
    fn set_title(&mut self, title: &str) -> HResult<()> {
        write!(self, "\x1b]2;hunter: {}", title)?;
        write!(self, "\x1bkhunter: {}\x1b\\", title)?;
        Ok(())
    }
    fn to_main_screen(&mut self) -> HResult<()> {
        write!(self, "{}", termion::screen::ToMainScreen)?;
        self.flush()?;
        Ok(())
    }
}

impl ScreenExt for Screen {}
impl ScreenExt for TermMode {}

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

fn is_ansi(ansi_pos: &Vec<(usize, usize)>, char_pos: &usize) -> bool {
    ansi_pos.iter().fold(false, |is_ansi, (start, end)| {
        if char_pos >= start && char_pos <= end {
            true
        } else { is_ansi }
    })
}

fn ansi_len_at(ansi_pos: &Vec<(usize, usize)>, char_pos: &usize) -> usize {
    ansi_pos.iter().fold(0, |len, (start, end)| {
        if char_pos >= end {
            len + (end-start)
        } else { len }
    })
}

pub fn sized_string_u(string: &str, xsize: usize) -> String {
    let ansi_pos = parse_bytes(string.as_bytes()).map(|m| {
        (m.start(), m.end())
    }).collect();

    let sized = string.chars().enumerate().fold("".to_string(), |acc, (i, ch)| {
        let width: usize = unicode_width::UnicodeWidthStr::width_cjk(acc.as_str());
        let ansi_len = ansi_len_at(&ansi_pos, &i);

        if width >= xsize as usize + ansi_len {
            acc
        } else {
            acc + &ch.to_string()
        }

    });
    let ansi_len = ansi_len_at(&ansi_pos, &(sized.len().saturating_sub(1)));
    let padded = format!("{:padding$}", sized, padding=xsize + ansi_len + 1);
    padded
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
