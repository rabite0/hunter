use std::io::{stdin, stdout, Stdout, Write};
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{Sender, Receiver, channel};

use termion::event::{Event, Key};
use termion::input::TermRead;
use termion::screen::AlternateScreen;

use crate::term;
use crate::term::ScreenExt;

use crate::coordinates::{Coordinates, Position, Size};
use crate::widget::Widget;
use crate::fail::HResult;

lazy_static! {
    static ref TX_EVENT: Arc<Mutex<Option<Sender<Events>>>> = { Arc::new(Mutex::new(None)) };
}


#[derive(Debug)]
pub enum Events {
    InputEvent(Event),
    WidgetReady
}

pub struct Window<T>
where
    T: Widget,
{
    pub selection: usize,
    pub widget: T,
    pub status: Arc<Mutex<Option<String>>>,
    pub screen: AlternateScreen<Box<Stdout>>,
    pub coordinates: Coordinates,
}

impl<T> Window<T>
where
    T: Widget,
{
    pub fn new(widget: T) -> Window<T> {
        let mut screen = AlternateScreen::from(Box::new(stdout()));
        screen.cursor_hide();
        let (xsize, ysize) = termion::terminal_size().unwrap();
        let mut win = Window::<T> {
            selection: 0,
            widget: widget,
            status: STATUS_BAR_CONTENT.clone(),
            screen: screen,
            coordinates: Coordinates {
                size: Size((xsize, ysize)),
                position: Position((1, 1)),
            },
        };

        win.widget.set_coordinates(&Coordinates {
            size: Size((xsize, ysize - 2)),
            position: Position((1, 2)),
        });
        win.widget.refresh();
        win
    }

    pub fn draw(&mut self) {
        let output = self.widget.get_drawlist() + &self.widget.get_header_drawlist()
            + &self.widget.get_footer_drawlist();
        self.screen.write(output.as_ref()).unwrap();

        self.screen.flush().unwrap();
    }

    // pub fn show_status(status: &str) {
    //     show_status(status);
    // }

    // pub fn draw_status() {
    //     draw_status();
    // }

    // pub fn clear_status() {
    //     Self::show_status("");
    // }


    pub fn handle_input(&mut self) {
        let (tx_event_internal, rx_event_internal) = channel();
        let (tx_event, rx_event) = channel();
        *TX_EVENT.try_lock().unwrap() = Some(tx_event);

        event_thread(rx_event, tx_event_internal.clone());
        input_thread(tx_event_internal);

        for event in rx_event_internal.iter() {
            //Self::clear_status();
            //let event = event.unwrap();
            dbg!(&event);
            match event {
                Events::InputEvent(event) => {
                    self.widget.on_event(event);
                    self.screen.cursor_hide();
                    self.draw();
                }
                _ => {
                    self.widget.refresh();
                    self.draw();
                },
            }
        }
    }
}

fn event_thread(rx: Receiver<Events>, tx: Sender<Events>) {
    std::thread::spawn(move || {
        for event in rx.iter() {
            dbg!(&event);
            tx.send(event).unwrap();
        }
    });
}

fn input_thread(tx: Sender<Events>) {
    std::thread::spawn(move || {
        for input in stdin().events() {
            let input = input.unwrap();
            tx.send(Events::InputEvent(input)).unwrap();
        }
    });
}

pub fn send_event(event: Events) -> HResult<()> {
    let tx = TX_EVENT.lock()?.clone()?.clone();
    tx.send(event)?;
    Ok(())
}

impl<T> Drop for Window<T>
where
    T: Widget,
{
    fn drop(&mut self) {
        // When done, restore the defaults to avoid messing with the terminal.
        self.screen
            .write(
                format!(
                    "{}{}{}{}{}",
                    termion::screen::ToMainScreen,
                    termion::clear::All,
                    termion::style::Reset,
                    termion::cursor::Show,
                    termion::cursor::Goto(1, 1)
                )
                .as_ref(),
            )
            .unwrap();
    }
}

lazy_static! {
    static ref STATUS_BAR_CONTENT: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
}

pub fn draw_status() {
    let xsize = term::xsize() as u16;
    let status = STATUS_BAR_CONTENT.try_lock().unwrap().clone();

    status.or(Some("".to_string())).and_then(|status| {
        write!(
            stdout(),
            "{}{}{:xsize$}{}{}",
            term::move_bottom(),
            term::status_bg(),
            " ",
            term::move_bottom(),
            status,
            xsize = xsize as usize
        )
        .ok()
    });
    stdout().flush().unwrap();
}

pub fn show_status(status: &str) {
    {
        let mut status_content = STATUS_BAR_CONTENT.try_lock().unwrap();
        *status_content = Some(status.to_string());
    }
    draw_status();
}

pub fn minibuffer(query: &str) -> Option<String> {
    show_status(&(query.to_string() + ": "));
    write!(stdout(), "{}{}",
            termion::cursor::Show,
            termion::cursor::Save).unwrap();
    stdout().flush().unwrap();

    let mut buffer = "".to_string();
    let mut pos = 0;

    for key in stdin().events() {

        match key {
            Ok(Event::Key(key)) => match key {
                Key::Esc | Key::Ctrl('c') => break,
                Key::Char('\n') => {
                    if buffer == "" {
                        write!(stdout(), "{}", termion::cursor::Hide).unwrap();
                        stdout().flush().unwrap();
                        return None;
                    } else {
                        write!(stdout(), "{}", termion::cursor::Hide).unwrap();
                        stdout().flush().unwrap();
                        return Some(buffer);
                    }
                }
                Key::Char('\t') => {
                    if !buffer.ends_with(" ") {
                        let part = buffer.rsplitn(2, " ").take(1)
                            .map(|s| s.to_string()).collect::<String>();
                        let completions = find_files(part.clone());
                        if !completions.is_empty() {
                            buffer = buffer[..buffer.len() - part.len()].to_string();
                            buffer.push_str(&completions[0]);
                            pos += &completions[0].len() - part.len();
                        } else {
                            let completions = find_bins(&part);
                            if !completions.is_empty() {
                                buffer = buffer[..buffer.len() - part.len()].to_string();
                                buffer.push_str(&completions[0]);
                                pos += &completions[0].len() - part.len();
                            }
                        }
                    } else {
                        buffer += "$s";
                        pos += 2
                    }
                }
                Key::Backspace => {
                    if pos != 0 {
                        buffer.remove(pos - 1);
                        pos -= 1;
                    }
                }
                Key::Delete | Key::Ctrl('d') => {
                    if pos != buffer.len() {
                        buffer.remove(pos);
                    }
                }
                Key::Left | Key::Ctrl('b') => {
                    if pos != 0 {
                        pos -= 1;
                    }
                }
                Key::Right | Key::Ctrl('f') => {
                    if pos != buffer.len() {
                        pos += 1;
                    }
                }
                Key::Ctrl('a') => { pos = 0 },
                Key::Ctrl('e') => { pos = buffer.len(); },
                Key::Char(key) => {
                    buffer.insert(pos, key);
                    pos += 1;
                }
                _ => {}
            },
            _ => {}
        }
        show_status(&(query.to_string() + ": " + &buffer));

        write!(stdout(), "{}", termion::cursor::Restore).unwrap();
        stdout().flush().unwrap();
        if pos != 0 {
            write!(stdout(),
                   "{}",
                   format!("{}", termion::cursor::Right(pos as u16))).unwrap();
        }
        stdout().flush().unwrap();
    }
    None
}

pub fn find_bins(comp_name: &str) -> Vec<String> {
    let paths = std::env::var_os("PATH").unwrap()
        .to_string_lossy()
        .split(":")
        .map(|s| s.to_string())
        .collect::<Vec<String>>();

    paths.iter().map(|path| {
        std::fs::read_dir(path).unwrap().flat_map(|file| {
            let file = file.unwrap();
            let name = file.file_name().into_string().unwrap();
            if name.starts_with(comp_name) {
                Some(name)
            } else {
                None
            }
        }).collect::<Vec<String>>()
    }).flatten().collect::<Vec<String>>()
}

pub fn find_files(mut comp_name: String) -> Vec<String> {
    let mut path = std::path::PathBuf::from(&comp_name);

    let dir = if comp_name.starts_with("/") {
        comp_name = path.file_name().unwrap().to_string_lossy().to_string();
        path.pop();
        path.to_string_lossy().to_string()
    } else {
        std::env::current_dir().unwrap().to_string_lossy().to_string()
    };

    let reader = std::fs::read_dir(dir.clone());
    if reader.is_err() { return vec![]  }
    let reader = reader.unwrap();

    reader.flat_map(|file| {
        let file = file.unwrap();
        let name = file.file_name().into_string().unwrap();
        if name.starts_with(&comp_name) {
            if file.file_type().unwrap().is_dir() {
                Some(format!("{}/{}/", &dir, name))
            } else {
                Some(format!("/{}/", name))
            }
        } else {
            None
        }
    }).collect::<Vec<String>>()
}
