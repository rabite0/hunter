use std::sync::{Arc, Mutex, RwLock};
use std::sync::mpsc::{Sender, Receiver, channel};
use std::io::{Write, stdin};

use termion::event::{Event, Key, MouseEvent};
use termion::input::TermRead;

use crate::coordinates::{Coordinates, Position, Size};
use crate::fail::{HResult, HError, ErrorLog};
use crate::minibuffer::MiniBuffer;
use crate::term;
use crate::term::{Screen, ScreenExt};
use crate::dirty::{Dirtyable, DirtyBit};
use crate::preview::Stale;
use crate::signal_notify::{notify, Signal};
use crate::config::Config;
use crate::preview::Async;



#[derive(Debug)]
pub enum Events {
    InputEvent(Event),
    WidgetReady,
    TerminalResized,
    ExclusiveEvent(Option<Mutex<Option<Sender<Events>>>>),
    InputEnabled(bool),
    RequestInput,
    Status(String),
    ConfigLoaded,
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
    pub screen: Screen,
    pub coordinates: Coordinates,
    pub minibuffer: Arc<Mutex<Option<MiniBuffer>>>,
    pub event_sender: Arc<Mutex<Sender<Events>>>,
    event_receiver: Arc<Mutex<Option<Receiver<Events>>>>,
    pub status_bar_content: Arc<Mutex<Option<String>>>,
    term_size: (usize, usize),
    dirty: DirtyBit,
    pub config: Arc<RwLock<Async<Config>>>
}

impl WidgetCore {
    pub fn new() -> HResult<WidgetCore> {
        let screen = Screen::new()?;
        let (xsize, ysize) = screen.size()?;
        let coords = Coordinates::new_at(term::xsize(),
                                         term::ysize() - 2,
                                         1,
                                         2);
        let (sender, receiver) = channel();
        let status_bar_content = Arc::new(Mutex::new(None));

        let mut config = Async::new(Box::new(|_| Config::load()));
        let confsender = Arc::new(Mutex::new(sender.clone()));
        config.on_ready(Box::new(move || {
            confsender.lock()?.send(Events::ConfigLoaded).ok();
            Ok(())
        }));
        config.run().log();

        let core = WidgetCore {
            screen: screen,
            coordinates: coords,
            minibuffer: Arc::new(Mutex::new(None)),
            event_sender: Arc::new(Mutex::new(sender)),
            event_receiver: Arc::new(Mutex::new(Some(receiver))),
            status_bar_content: status_bar_content,
            term_size: (xsize, ysize),
            dirty: DirtyBit::new(),
            config: Arc::new(RwLock::new(config)) };

        let minibuffer = MiniBuffer::new(&core);
        *core.minibuffer.lock().unwrap() = Some(minibuffer);
        Ok(core)
    }

    pub fn get_sender(&self) -> Sender<Events> {
        self.event_sender.lock().unwrap().clone()
    }
}

impl Dirtyable for WidgetCore {
    fn is_dirty(&self) -> bool {
        self.dirty.is_dirty()
    }
    fn set_dirty(&mut self) {
        self.dirty.set_dirty();
    }
    fn set_clean(&mut self) {
        self.dirty.set_clean();
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
        let core = &mut self.get_core_mut()?;
        if &core.coordinates != coordinates {
            core.coordinates = coordinates.clone();
            core.set_dirty();
        }
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
    fn config_loaded(&mut self) -> HResult<()> { Ok(()) }



    fn on_event(&mut self, event: Event) -> HResult<()> {
        self.clear_status().log();
        match event {
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
        self.show_status(&format!("Stop it!! {:?} does nothing!", event))
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
        let result = self.run_widget();
        self.clear().log();
        self.get_core()?.get_sender().send(Events::ExclusiveEvent(None))?;
        result
    }

    fn popup_finnished(&self) -> HResult<()> {
        HError::popup_finnished()
    }

    fn run_widget(&mut self) -> HResult<()> {
        let (tx_event, rx_event) = channel();
        self.get_core()?
            .get_sender()
            .send(Events::ExclusiveEvent(Some(Mutex::new(Some(tx_event)))))?;
        self.get_core()?.get_sender().send(Events::RequestInput)?;

        self.clear()?;
        self.refresh().log();
        self.draw()?;

        for event in rx_event.iter() {
            match event {
                Events::InputEvent(input) => {
                    match self.on_event(input) {
                        err @ Err(HError::PopupFinnished) |
                        err @ Err(HError::Quit) |
                        err @ Err(HError::MiniBufferCancelledInput) => err?,
                        err @ Err(HError::WidgetResizedError) => err?,
                        err @ Err(_) => err.log(),
                        Ok(_) => {}
                    }
                    self.get_core()?.get_sender().send(Events::RequestInput)?;
                }
                Events::WidgetReady => {
                    self.refresh().log();
                    self.draw().log();
                }
                Events::Status(status) => {
                    self.show_status(&status).log();
                }
                Events::TerminalResized => {
                    self.screen()?.clear().log();
                    match self.resize() {
                        err @ Err(HError::TerminalResizedError) => err?,
                        _ => {}
                    }
                }
                Events::ConfigLoaded => {
                    self.get_core_mut()?.config.write()?.take_async().log();
                }
                _ => {}
            }
            self.refresh().log();
            self.draw().log();
            self.after_draw().log();
        }
        Ok(())
    }

    fn clear(&self) -> HResult<()> {
        let clearlist = self.get_clearlist()?;
        self.write_to_screen(&clearlist)
    }

    fn config(&self) -> Config {
        self.get_core()
            .unwrap()
            .config.read().unwrap().get()
            .map(|config| config.clone())
            .unwrap_or(Config::new())
    }

    fn animate_slide_up(&mut self, animator: Option<Stale>) -> HResult<()> {
        if !self.config().animate() { return Ok(()); }

        self.config();

        let coords = self.get_coordinates()?.clone();
        let xpos = coords.position().x();
        let ypos = coords.position().y();
        let xsize = coords.xsize();
        let ysize = coords.ysize();
        let clear = self.get_clearlist()?;
        let pause = std::time::Duration::from_millis(5);

        if let Some(ref animator) = animator {
            if animator.is_stale()? {
                return Ok(())
            }
        }

        self.write_to_screen(&clear).log();

        for i in (0..10).rev() {
            if let Some(ref animator) = animator {
                if animator.is_stale()? {
                    self.set_coordinates(&coords).log();
                    return Ok(())
                }
            }
            let ani_coords = Coordinates { size: Size((xsize,ysize-i)),
                                       position: Position
                                           ((xpos,
                                             ypos+i))
            };
            self.set_coordinates(&ani_coords).log();
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
        self.screen()?.flush().ok();
        Ok(())
    }

    fn handle_input(&mut self) -> HResult<()> {
        let (tx_internal_event, rx_internal_event) = channel();
        let rx_global_event = self.get_core()?.event_receiver.lock()?.take()?;

        dispatch_events(tx_internal_event, rx_global_event, self.screen()?);

        for event in rx_internal_event.iter() {
            match event {
                Events::InputEvent(event) => {
                    match self.on_event(event) {
                        Err(HError::Quit) => { HError::quit()?; },
                        _ => {}
                    }
                    self.get_core()?.get_sender().send(Events::RequestInput)?;
                }
                Events::Status(status) => {
                    self.show_status(&status).log();
                }
                Events::TerminalResized => {
                    self.screen()?.clear().log();
                }
                Events::ConfigLoaded => {
                    self.get_core_mut()?.config.write()?.take_async().log();
                    self.config_loaded().log();
                }
                _ => {}
            }
            self.resize().log();
            if self.screen()?.is_resized()? {
                self.screen()?.take_size().ok();
            }
            self.refresh().ok();
            self.draw().ok();
        }
        Ok(())
    }

    fn draw_status(&self) -> HResult<()> {
        let xsize = term::xsize_u();
        let status = match self.get_core()?.status_bar_content.lock()?.as_ref() {
            Some(status) => status.to_string(),
            None => "".to_string(),
        };
        let sized_status = term::sized_string_u(&status, xsize);

        self.write_to_screen(
            &format!(
                "{}{}{}",
                term::move_bottom(),
                term::status_bg(),
                sized_status
            )).log();

        Ok(())
    }

    fn show_status(&self, status: &str) -> HResult<()> {
        HError::log::<()>(status.to_string()).log();
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
        let mut screen = self.screen()?;
        screen.cursor_hide().log();
        answer
    }

    fn screen(&self) -> HResult<Screen> {
        Ok(self.get_core()?.screen.clone())
    }

    fn write_to_screen(&self, s: &str) -> HResult<()> {
        let mut screen = self.screen()?;
        screen.write_str(s)
    }

    fn resize(&mut self) -> HResult<()> {
        if let Ok(true) = self.screen()?.is_resized() {
            let (xsize, ysize) = self.screen()?.get_size()?;
            let mut coords = self.get_core()?.coordinates.clone();
            coords.set_size_u(xsize, ysize-2);
            self.set_coordinates(&coords)?;
        }
        Ok(())
    }
}

fn dispatch_events(tx_internal: Sender<Events>,
                   rx_global: Receiver<Events>,
                   screen: Screen) {
    let (tx_event, rx_event) = channel();
    let (tx_input_req, rx_input_req) = channel();

    input_thread(tx_event.clone(), rx_input_req);
    event_thread(rx_global, tx_event.clone());
    signal_thread(tx_event.clone());

    std::thread::spawn(move || {
        let mut tx_exclusive_event: Option<Sender<Events>> = None;
        let mut input_enabled = true;

        for event in rx_event.iter() {
            match &event {
                Events::ExclusiveEvent(tx_event) => {
                    tx_exclusive_event = match tx_event {
                        Some(locked_sender) => locked_sender.lock().unwrap().take(),
                        None => None
                    }
                }
                Events::InputEnabled(state) => {
                    input_enabled = *state;
                    continue;
                }
                Events::RequestInput => {
                    if input_enabled {
                        tx_input_req.send(()).unwrap();
                    }
                    continue;
                }
                Events::TerminalResized => {
                    if let Ok(size) = term::size() {
                        screen.set_size(size).log();
                    }
                }
                _ => {}
            }
            if let Some(tx_exclusive) =  &tx_exclusive_event {
                tx_exclusive.send(event).ok();
            } else {
                tx_internal.send(event).ok();
            }
        }
    });
}

fn event_thread(rx_global: Receiver<Events>,
                       tx: Sender<Events>) {
    std::thread::spawn(move || {
        for event in rx_global.iter() {
            tx.send(event).unwrap();
        }
    });
}

fn input_thread(tx: Sender<Events>, rx_input_request: Receiver<()>) {
    std::thread::spawn(move || {
        for input in stdin().events() {
            let input = input.unwrap();
            tx.send(Events::InputEvent(input)).unwrap();
            rx_input_request.recv().unwrap();
        }
    });
}

fn signal_thread(tx: Sender<Events>) {
    std::thread::spawn(move || {
        let rx = notify(&[Signal::WINCH]);
        for _ in rx.iter() {
            tx.send(Events::TerminalResized).unwrap();
        }
    });
}
