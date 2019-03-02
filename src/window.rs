// use std::io::{stdin, stdout, Stdout, Write};
// use std::sync::{Arc, Mutex};
// use std::sync::mpsc::{Sender, Receiver, channel};

// use termion::event::Event;
// use termion::input::TermRead;
// use termion::screen::AlternateScreen;

// use crate::term;
// use crate::term::ScreenExt;

// use crate::coordinates::{Coordinates, Position, Size};
// use crate::widget::Widget;
// use crate::minibuffer::MiniBuffer;
// use crate::fail::HResult;

// lazy_static! {
//     static ref TX_EVENT: Arc<Mutex<Option<Sender<Events>>>> = { Arc::new(Mutex::new(None)) };
//     static ref MINIBUFFER: Arc<Mutex<MiniBuffer>>
//         = Arc::new(Mutex::new(MiniBuffer::new()));
// }

// #[derive(Debug)]
// pub enum Events {
//     InputEvent(Event),
//     WidgetReady,
//     ExclusiveEvent(Option<Sender<Events>>),
// }

// pub struct Window<T>
// where
//     T: Widget,
// {
//     pub selection: usize,
//     pub widget: T,
//     pub status: Arc<Mutex<Option<String>>>,
//     pub screen: AlternateScreen<Box<Stdout>>,
//     pub coordinates: Coordinates,
// }

// impl<T> Window<T>
// where
//     T: Widget,
// {
//     pub fn new(widget: T) -> Window<T> {
//         let mut screen = AlternateScreen::from(Box::new(stdout()));
//         screen.cursor_hide();
//         let (xsize, ysize) = termion::terminal_size().unwrap();
//         let mut win = Window::<T> {
//             selection: 0,
//             widget: widget,
//             status: STATUS_BAR_CONTENT.clone(),
//             screen: screen,
//             coordinates: Coordinates {
//                 size: Size((xsize, ysize)),
//                 position: Position((1, 1)),
//             },
//         };

//         win.widget.set_coordinates(&Coordinates {
//             size: Size((xsize, ysize - 2)),
//             position: Position((1, 2)),
//         });
//         win.widget.refresh();
//         win
//     }

//     pub fn draw(&mut self) {
//         let output = self.widget.get_drawlist() + &self.widget.get_header_drawlist()
//             + &self.widget.get_footer_drawlist();
//         self.screen.write(output.as_ref()).unwrap();

//         self.screen.flush().unwrap();
//     }

//     // pub fn show_status(status: &str) {
//     //     show_status(status);
//     // }

//     // pub fn draw_status() {
//     //     draw_status();
//     // }

//     // pub fn clear_status() {
//     //     Self::show_status("");
//     // }


//     pub fn handle_input(&mut self) {
//         let (tx_event, rx_event) = channel();
//         let (tx_global_event, rx_global_event) = channel();
//         *TX_EVENT.try_lock().unwrap() = Some(tx_global_event);
//         let (tx_internal_event, rx_internal_event) = channel();

//         input_thread(tx_event.clone());
//         global_event_thread(rx_global_event, tx_event.clone());
//         dispatch_events(rx_event, tx_internal_event);

//         for event in rx_internal_event.iter() {
//             match event {
//                 Events::InputEvent(event) => {
//                     self.widget.on_event(event);
//                     self.screen.cursor_hide();
//                     self.draw();
//                 },
//                 _ => {
//                     self.widget.refresh();
//                     self.draw();
//                 },
//             }
//         }
//     }
// }

// fn dispatch_events(rx: Receiver<Events>, tx: Sender<Events>) {
//     std::thread::spawn(move || {
//         let mut tx_exclusive_event: Option<Sender<Events>> = None;
//         for event in rx.iter() {
//             match &event {
//                 Events::ExclusiveEvent(tx_event) => {
//                     tx_exclusive_event = tx_event.clone();
//                 }
//                 _ => {}
//             }
//             if let Some(tx_event) = &tx_exclusive_event {
//                 tx_event.send(event).unwrap();
//             } else {
//                 tx.send(event).unwrap();
//             }
//         }
//     });
// }

// fn global_event_thread(rx_global: Receiver<Events>,
//                        tx: Sender<Events>) {
//     std::thread::spawn(move || {
//         for event in rx_global.iter() {
//             tx.send(event).unwrap();
//         }
//     });
// }

// fn input_thread(tx: Sender<Events>) {
//     std::thread::spawn(move || {
//         for input in stdin().events() {
//             let input = input.unwrap();
//             tx.send(Events::InputEvent(input)).unwrap();
//         }
//     });
// }

// pub fn send_event(event: Events) -> HResult<()> {
//     let tx = TX_EVENT.lock()?.clone()?.clone();
//     tx.send(event)?;
//     Ok(())
// }

// impl<T> Drop for Window<T>
// where
//     T: Widget,
// {
//     fn drop(&mut self) {
//         // When done, restore the defaults to avoid messing with the terminal.
//         self.screen
//             .write(
//                 format!(
//                     "{}{}{}{}{}",
//                     termion::screen::ToMainScreen,
//                     termion::clear::All,
//                     termion::style::Reset,
//                     termion::cursor::Show,
//                     termion::cursor::Goto(1, 1)
//                 )
//                 .as_ref(),
//             )
//             .unwrap();
//     }
// }

// lazy_static! {
//     static ref STATUS_BAR_CONTENT: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
// }

// pub fn draw_status() {
//     let xsize = term::xsize() as u16;
//     let status = STATUS_BAR_CONTENT.try_lock().unwrap().clone();

//     status.or(Some("".to_string())).and_then(|status| {
//         write!(
//             stdout(),
//             "{}{}{:xsize$}{}{}",
//             term::move_bottom(),
//             term::status_bg(),
//             " ",
//             term::move_bottom(),
//             status,
//             xsize = xsize as usize
//         )
//         .ok()
//     });
//     stdout().flush().unwrap();
// }

// pub fn show_status(status: &str) {
//     {
//         let mut status_content = STATUS_BAR_CONTENT.try_lock().unwrap();
//         *status_content = Some(status.to_string());
//     }
//     draw_status();
// }

// pub fn minibuffer(query: &str) -> HResult<String> {
//     MINIBUFFER.lock()?.query(query)
// }
