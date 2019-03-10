use std::sync::{Arc, Mutex};
use std::sync::mpsc::{Sender, Receiver, channel};

use termion::event::{Event, Key, MouseEvent};
use termion::input::TermRead;
use termion::screen::AlternateScreen;


use crate::coordinates::{Coordinates, Position, Size};
use crate::fail::{HResult, HError, ErrorLog};
use crate::minibuffer::MiniBuffer;
use crate::term;
use crate::term::ScreenExt;

use std::io::{BufWriter, stdin, stdout, Stdout};

#[derive(Debug)]
pub enum Events {
    InputEvent(Event),
    WidgetReady,
    ExclusiveEvent(Option<Sender<Events>>),
    Status(String)
}

impl PartialEq for WidgetCore {
    fn eq(&self, other: &WidgetCore) -> bool {
        if self.coordinates == other.coordinates {
            true
        } else {
            false
        }
    }
}

impl std::fmt::Debug for WidgetCore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        let output = format!("{:?}{:?}{:?}",
                             self.coordinates,
                             self.minibuffer,
                             self.status_bar_content);
        formatter.write_str(&output)
    }
}

#[derive(Clone)]
pub struct WidgetCore {
    pub screen: Arc<Mutex<AlternateScreen<BufWriter<Stdout>>>>,
    pub coordinates: Coordinates,
    pub minibuffer: Arc<Mutex<Option<MiniBuffer>>>,
    pub event_sender: Sender<Events>,
    event_receiver: Arc<Mutex<Option<Receiver<Events>>>>,
    pub status_bar_content: Arc<Mutex<Option<String>>>
}

impl WidgetCore {
    pub fn new() -> HResult<WidgetCore> {
        let screen = AlternateScreen::from(BufWriter::new(stdout()));
        let coords = Coordinates::new_at(term::xsize(),
                                         term::ysize() - 2,
                                         1,
                                         2);
        let (sender, receiver) = channel();
        let status_bar_content = Arc::new(Mutex::new(None));

        let core = WidgetCore {
            screen: Arc::new(Mutex::new(screen)),
            coordinates: coords,
            minibuffer: Arc::new(Mutex::new(None)),
            event_sender: sender,
            event_receiver: Arc::new(Mutex::new(Some(receiver))),
            status_bar_content: status_bar_content };

        let minibuffer = MiniBuffer::new(&core);
        *core.minibuffer.lock().unwrap() = Some(minibuffer);
        Ok(core)
    }

    pub fn get_sender(&self) -> Sender<Events> {
        self.event_sender.clone()
    }
}

pub trait Widget {
    fn get_core(&self) -> HResult<&WidgetCore>; // {
    //     Err(HError::NoWidgetCoreError(Backtrace::new()))
    // }
    fn get_core_mut(&mut self) -> HResult<&mut WidgetCore> ;// {
    //     Err(HError::NoWidgetCoreError(Backtrace::new()))
    // }
    fn get_coordinates(&self) -> HResult<&Coordinates> {
        Ok(&self.get_core()?.coordinates)
    }
    fn set_coordinates(&mut self, coordinates: &Coordinates) -> HResult<()> {
        self.get_core_mut()?.coordinates = coordinates.clone();
        self.refresh()?;
        Ok(())
    }
    fn render_header(&self) -> HResult<String> {
        Err(HError::NoHeaderError)
    }
    fn render_footer(&self) -> HResult<String> {
        Err(HError::NoHeaderError)
    }
    fn refresh(&mut self) -> HResult<()>;
    fn get_drawlist(&self) -> HResult<String>;
    fn after_draw(&self) -> HResult<()> { Ok(()) }



    fn on_event(&mut self, event: Event) -> HResult<()> {
        self.clear_status().log();
        match event {
            Event::Key(Key::Char('q')) => HError::quit(),
            Event::Key(key) => self.on_key(key),
            Event::Mouse(button) => self.on_mouse(button),
            Event::Unsupported(wtf) => self.on_wtf(wtf),
        }
    }

    fn on_key(&mut self, key: Key) -> HResult<()> {
        match key {
            _ => { self.bad(Event::Key(key)).unwrap() },
        }
        Ok(())
    }

    fn on_mouse(&mut self, event: MouseEvent) -> HResult<()> {
        match event {
            _ => { self.bad(Event::Mouse(event)).unwrap() },
        }
        Ok(())
    }

    fn on_wtf(&mut self, event: Vec<u8>) -> HResult<()> {
        match event {
            _ => { self.bad(Event::Unsupported(event)).unwrap() },
        }
        Ok(())
    }

    fn bad(&mut self, event: Event) -> HResult<()> {
        self.show_status(&format!("Stop the nasty stuff!! {:?} does nothing!", event))
    }

    fn get_header_drawlist(&mut self) -> HResult<String> {
        Ok(format!(
            "{}{}{:xsize$}{}{}",
            crate::term::goto_xy(1, 1),
            crate::term::header_color(),
            " ",
            crate::term::goto_xy(1, 1),
            self.render_header()?,
            xsize = self.get_coordinates()?.xsize() as usize
        ))
    }

    fn get_footer_drawlist(&mut self) -> HResult<String> {
        let xsize = self.get_coordinates()?.xsize();
        let ypos = crate::term::ysize();
        Ok(format!(
            "{}{}{:xsize$}{}{}",
            crate::term::goto_xy(1, ypos),
            crate::term::header_color(),
            " ",
            crate::term::goto_xy(1, ypos),
            self.render_footer()?,
            xsize = xsize as usize))
    }

    fn get_clearlist(&self) -> HResult<String> {
        let (xpos, ypos) = self.get_coordinates()?.u16position();
        let (xsize, ysize) = self.get_coordinates()?.u16size();
        let endpos = ypos + ysize;

        Ok((ypos..endpos)
           .map(|line| {
               format!(
                    "{}{}{:xsize$}",
                    crate::term::reset(),
                    crate::term::goto_xy(xpos, line),
                    " ",
                    xsize = xsize as usize
                )
            })
            .collect())
    }

    fn get_redraw_empty_list(&self, lines: usize) -> HResult<String> {
        let (xpos, ypos) = self.get_coordinates()?.u16position();
        let (xsize, ysize) = self.get_coordinates()?.u16size();

        let start_y = lines + ypos as usize;
        Ok((start_y..(ysize + 2) as usize)
            .map(|i| {
                format!(
                    "{}{:xsize$}",
                    crate::term::goto_xy(xpos, i as u16),
                    " ",
                    xsize = xsize as usize
                )
            })
            .collect())
    }

    fn popup(&mut self) -> HResult<()> {
        self.run_widget().log();
        self.clear().log();
        self.get_core()?.get_sender().send(Events::ExclusiveEvent(None))?;
        Ok(())
    }

    fn run_widget(&mut self) -> HResult<()> {
        let (tx_event, rx_event) = channel();
        self.get_core()?.get_sender().send(Events::ExclusiveEvent(Some(tx_event)))?;

        self.clear()?;
        self.refresh().log();
        self.draw()?;

        for event in rx_event.iter() {
            match event {
                Events::InputEvent(input) => {
                    if let Err(HError::PopupFinnished) = self.on_event(input) {
                        return Err(HError::PopupFinnished)
                    }
                }
                Events::WidgetReady => {
                    self.refresh().log();
                }
                _ => {}
            }
            self.draw().log();
            self.after_draw().log();
        }
        Ok(())
    }

    fn clear(&self) -> HResult<()> {
        let clearlist = self.get_clearlist()?;
        self.write_to_screen(&clearlist)
    }

    fn animate_slide_up(&mut self) -> HResult<()> {
        let coords = self.get_coordinates()?.clone();
        let xpos = coords.position().x();
        let ypos = coords.position().y();
        let xsize = coords.xsize();
        let ysize = coords.ysize();
        let clear = self.get_clearlist()?;
        let pause = std::time::Duration::from_millis(5);

        self.write_to_screen(&clear).log();

        for i in (0..10).rev() {
            let coords = Coordinates { size: Size((xsize,ysize-i)),
                                       position: Position
                                           ((xpos,
                                             ypos+i))
            };
            self.set_coordinates(&coords).log();
            let buffer = self.get_drawlist()?;
            self.write_to_screen(&buffer).log();

            std::thread::sleep(pause);
        }
        Ok(())
    }

    fn draw(&mut self) -> HResult<()> {
        let output =
            self.get_drawlist().unwrap_or("".to_string()) +
            &self.get_header_drawlist().unwrap_or("".to_string()) +
            &self.get_footer_drawlist().unwrap_or("".to_string());
        self.write_to_screen(&output).log();
        Ok(())
    }

    fn handle_input(&mut self) -> HResult<()> {
        let (tx_event, rx_event) = channel();
        let (tx_internal_event, rx_internal_event) = channel();
        let rx_global_event = self.get_core()?.event_receiver.lock()?.take()?;

        input_thread(tx_event.clone());
        global_event_thread(rx_global_event, tx_event.clone());
        dispatch_events(rx_event, tx_internal_event);

        for event in rx_internal_event.iter() {
            match event {
                Events::InputEvent(event) => {
                    match self.on_event(event) {
                        Err(HError::Quit) => { HError::quit()?; },
                        _ => {}
                    }
                    self.draw().ok();
                },
                Events::Status(status) => {
                    self.show_status(&status).log();
                }
                _ => {
                    self.refresh().ok();
                    self.draw().ok();
                },
            }
        }
        Ok(())
    }

    fn draw_status(&self) -> HResult<()> {
        let xsize = term::xsize() as u16;
        let status = match self.get_core()?.status_bar_content.lock()?.as_ref() {
            Some(status) => status.to_string(),
            None => "".to_string(),
        };

        self.write_to_screen(
            &format!(
                "{}{}{:xsize$}{}{}",
                term::move_bottom(),
                term::status_bg(),
                " ",
                term::move_bottom(),
                status,
                xsize = xsize as usize
            )).log();

        Ok(())
    }

    fn show_status(&self, status: &str) -> HResult<()> {
        {
            let mut status_content = self.get_core()?.status_bar_content.lock()?;
            *status_content = Some(status.to_string());
        }
        self.draw_status()?;
        Ok(())
    }

    fn clear_status(&self) -> HResult<()> {
        if self.get_core()?.status_bar_content.lock()?.take().is_some() {
            self.draw_status().log();
        }
        Ok(())
    }

    fn minibuffer(&self, query: &str) -> HResult<String> {
        let answer = self.get_core()?.minibuffer.lock()?.as_mut()?.query(query);
        let mut screen = self.get_core()?.screen.lock()?;
        screen.cursor_hide().log();
        answer
    }

    fn write_to_screen(&self, s: &str) -> HResult<()> {
        let mut screen = self.get_core()?.screen.lock()?;
        screen.write_str(s)
    }
}

fn dispatch_events(rx: Receiver<Events>, tx: Sender<Events>) {
    std::thread::spawn(move || {
        let mut tx_exclusive_event: Option<Sender<Events>> = None;
        for event in rx.iter() {
            match &event {
                Events::ExclusiveEvent(tx_event) => {
                    tx_exclusive_event = tx_event.clone();
                }
                _ => {}
            }
            if let Some(tx_event) = &tx_exclusive_event {
                tx_event.send(event).ok();
            } else {
                tx.send(event).ok();
            }
        }
    });
}

fn global_event_thread(rx_global: Receiver<Events>,
                       tx: Sender<Events>) {
    std::thread::spawn(move || {
        for event in rx_global.iter() {
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
