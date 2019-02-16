use failure::Error;
use failure::Fail;

use std::io::Write;
use std::sync::Mutex;
use std::sync::Arc;

use crate::coordinates::{Coordinates};
use crate::files::{File, Files, Kind};
use crate::listview::ListView;
use crate::textview::TextView;
use crate::widget::Widget;
use crate::fail::HError;


type HResult<T> = Result<T, HError>;
type HClosure<T> = Box<Fn() -> Result<T, HError> + Send>;
type WClosure = HClosure<Box<dyn Widget>>;
type WidgetO = Box<dyn Widget + Send>;
type WidgetFn = Box<Fn() -> Box<dyn Widget + Send>>;

lazy_static! {
    static ref SUBPROC: Arc<Mutex<Option<u32>>> = { Arc::new(Mutex::new(None)) };
    static ref CURFILE: Arc<Mutex<Option<File>>> = { Arc::new(Mutex::new(None)) };
}

fn kill_proc() -> HResult<()> {
    let mut pid = SUBPROC.lock()?;
    pid.map(|pid|
        unsafe { libc::kill(pid as i32, 15); }
    );
    *pid = None;
    Ok(())
}

fn is_current(file: &File) -> bool {
    match CURFILE.lock().unwrap().as_ref() {
        Some(curfile) => curfile == file,
        None => true
    }
}

fn set_current(file: &File) -> HResult<()> {
    let mut curfile = CURFILE.lock()?;
    *curfile = Some(file.clone());
    Ok(())
}

enum State<T: Send> {
    Is(T),
    Becoming,
    Taken,
    Fail
}

struct WillBe<T: Send> {
    pub state: State<T>,
    rx: std::sync::mpsc::Receiver<T>,
    cancel: bool
}

impl<T: Send + 'static> WillBe<T> where {
    pub fn new_become(closure: HClosure<T>)
                  -> WillBe<T> {
        let (tx,rx) = std::sync::mpsc::channel();
        let mut willbe = WillBe { state: State::Becoming,
                                  rx: rx,
                                  cancel: false };
        willbe.run(closure, tx);
        willbe
    }

    fn run(&mut self, closure: HClosure<T>, tx: std::sync::mpsc::Sender<T>) {
        std::thread::spawn(move|| {
            let thing = closure();
            match thing {
                Ok(thing) => { tx.send(thing).ok(); },
                Err(err) => { dbg!(err); }
            }
        });
    }

    pub fn check(&mut self) -> Result<(), Error> {
        match self.state {
            State::Is(_) => Ok(()),
            _ => {
                let thing = self.rx.try_recv()?;
                self.state = State::Is(thing);
                Ok(())
            }
        }
    }

    pub fn wait(mut self) -> Result<T, std::sync::mpsc::RecvError> {
        self.rx.recv()
    }

    pub fn take(mut self) -> Option<T> {
        match self.state {
            State::Is(thing) => Some(thing),
            _ => None
        }
    }
}

impl<W: Widget + Send> PartialEq for WillBeWidget<W> {
    fn eq(&self, other: &WillBeWidget<W>) -> bool {
        if self.coordinates == other.coordinates {
            true
        } else {
            false
        }
    }
}

struct WillBeWidget<T: Widget + Send> {
    willbe: WillBe<T>,
    coordinates: Coordinates
}

impl<T: Widget + Send + 'static> WillBeWidget<T> {
    fn new(closure: HClosure<T>) -> WillBeWidget<T> {
        WillBeWidget {
            willbe: WillBe::new_become(Box::new(move || closure())),
            coordinates: Coordinates::new()
        }
    }
}

// impl<T: Widget + Send> WillBeWidget<T> {
//     fn is_widget(&self) -> bool {
//         self.willbe.check().is_ok()
//     }
    // fn take_widget(self) {
    //     if self.is_widget() {
    //         let widget = self.willbe.take();
    //     }
    // }
//}

impl<T: Widget + Send> Widget for WillBeWidget<T> {
    fn get_coordinates(&self) -> &Coordinates {
        &self.coordinates
    }
    fn set_coordinates(&mut self, coordinates: &Coordinates) {
        if self.coordinates == *coordinates {
            return;
        }
        self.coordinates = coordinates.clone();
        match &mut self.willbe.state {
            State::Is(widget) => {
                widget.set_coordinates(&coordinates.clone());
                self.refresh();
            }
            _ => {}
        }
    }
    fn render_header(&self) -> String {
        "".to_string()
    }
    fn refresh(&mut self) {
        match &mut self.willbe.state {
            State::Is(widget) => {
                widget.refresh();
            }
            _ => {}
        }
    }
    fn get_drawlist(&self) -> String {
        match &self.willbe.state {
            State::Is(widget) => {
                widget.get_drawlist()
            },
            _ => { "".to_string() }
        }
    }
}



#[derive(PartialEq)]
pub struct Previewer {
    widget: WillBeWidget<Box<dyn Widget + Send>>,
}


impl Previewer {
    pub fn new() -> Previewer {
        let willbe = WillBeWidget::new(Box::new(move || {
                Ok(Box::new(crate::textview::TextView {
                    lines: vec![],
                    buffer: String::new(),
                    coordinates: Coordinates::new()
                }) as Box<dyn Widget + Send>)
        }));
        Previewer { widget: willbe }
    }

    fn become_preview(&mut self,
                      widget: HResult<WillBeWidget<WidgetO>>) {
        let coordinates = self.get_coordinates().clone();
        self.widget =  widget.unwrap();
        self.set_coordinates(&coordinates);
    }

    pub fn set_file(&mut self, file: &File) {
        let coordinates = self.get_coordinates().clone();
        let file = file.clone();
        set_current(&file).unwrap();

        self.become_preview(Ok(WillBeWidget::new(Box::new(move || {
            kill_proc().unwrap();
            let file = file.clone();

            if file.kind == Kind::Directory  {
                let preview = Previewer::preview_dir(&file, &coordinates);
                return preview;
            }

            if file.get_mime() == Some("text".to_string()) {
                return Previewer::preview_text(&file, &coordinates)
            }

            let preview = Previewer::preview_external(&file, &coordinates);
            if preview.is_ok() { return preview; }
            else {
                let mut blank = Box::new(TextView::new_blank());
                blank.set_coordinates(&coordinates);
                blank.refresh();
                blank.animate_slide_up();
                return Ok(blank)
            }
        }))));
    }

    fn preview_failed(file: &File) -> HResult<WidgetO> {
        Err(HError::PreviewFailed { file: file.name.clone() })
    }

    fn preview_dir(file: &File, coordinates: &Coordinates)
                   -> Result<WidgetO, HError> {
        let files = Files::new_from_path(&file.path)?;
        let len = files.len();

        if len == 0 || !is_current(&file) { return Previewer::preview_failed(&file) }

        let mut file_list = ListView::new(files);
        file_list.set_coordinates(&coordinates);
        file_list.refresh();
        if !is_current(&file) { return Previewer::preview_failed(&file) }
        file_list.animate_slide_up();
        Ok(Box::new(file_list) as Box<dyn Widget + Send>)
    }

    fn preview_text(file: &File, coordinates: &Coordinates)
                    -> HResult<WidgetO> {
        let lines = coordinates.ysize() as usize;
        let mut textview
            = TextView::new_from_file_limit_lines(&file,
                                                  lines);
        if !is_current(&file) { return Previewer::preview_failed(&file) }

        textview.set_coordinates(&coordinates);
        textview.refresh();

        if !is_current(&file) { return Previewer::preview_failed(&file) }

        textview.animate_slide_up();
        Ok(Box::new(textview))
    }

    fn preview_external(file: &File, coordinates: &Coordinates)
                        -> Result<Box<dyn Widget + Send>, HError> {
        let process =
            std::process::Command::new("scope.sh")
            .arg(&file.name)
            .arg("10".to_string())
            .arg("10".to_string())
            .arg("".to_string())
            .arg("false".to_string())
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()?;

        let pid = process.id();
        {
            let mut pid_ = SUBPROC.lock()?;
            *pid_ = Some(pid);
        }

        if !is_current(&file) { return Previewer::preview_failed(&file) }

        let output = process.wait_with_output()?;

        if !is_current(&file) { return Previewer::preview_failed(&file) }

        {
            let mut pid_ = SUBPROC.lock()?;
            *pid_ = None;
        }

        let status = output.status.code()
            .ok_or(HError::PreviewFailed{file: file.name.clone()})?;

        if status == 0 || status == 5 && is_current(&file) {
            let output = std::str::from_utf8(&output.stdout)
                .unwrap()
                .to_string();
            let mut textview = TextView {
                lines: output.lines().map(|s| s.to_string()).collect(),
                buffer: String::new(),
                coordinates: Coordinates::new() };
            textview.set_coordinates(&coordinates);
            textview.refresh();
            textview.animate_slide_up();
            return Ok(Box::new(textview))
        }
        Err(HError::PreviewFailed{file: file.name.clone()})
    }

}



impl Widget for Previewer {
    fn get_coordinates(&self) -> &Coordinates {
        &self.widget.coordinates
    }
    fn set_coordinates(&mut self, coordinates: &Coordinates) {
        if self.widget.coordinates == *coordinates {
            return;
        }
        self.widget.set_coordinates(coordinates);
    }
    fn render_header(&self) -> String {
        "".to_string()
    }
    fn refresh(&mut self) {
        self.widget.refresh();
    }
    fn get_drawlist(&self) -> String {
        self.widget.get_drawlist()
    }
}
























// #[derive(PartialEq)]
// pub struct AsyncPreviewer {
//     pub file: Option<File>,
//     pub buffer: String,
//     pub coordinates: Coordinates,
//     pub async_plug: AsyncPlug2<Box<dyn Widget + Send + 'static>>
// }

// impl AsyncPreviewer {
//     pub fn new() -> AsyncPreviewer {
//         let closure = Box::new(|| {
//             Box::new(crate::textview::TextView {
//                     lines: vec![],
//                     buffer: "".to_string(),
//                     coordinates: Coordinates::new()
//             }) as Box<dyn Widget + Send + 'static>
//         });

//         AsyncPreviewer {
//             file: None,
//             buffer: String::new(),
//             coordinates: Coordinates::new(),
//             async_plug: AsyncPlug2::new_from_closure(closure),
//         }
//     }
//     pub fn set_file(&mut self, file: &File) {
//         let coordinates = self.coordinates.clone();
//         let file = file.clone();
//         let redraw = crate::term::reset() + &self.get_redraw_empty_list(0);
//         //let pids = PIDS.clone();
//         //kill_procs();

//         self.async_plug.replace_widget(Box::new(move || {
//             kill_procs();
//             let mut bufout = std::io::BufWriter::new(std::io::stdout());
//             match &file.kind {
//                 Kind::Directory => match Files::new_from_path(&file.path) {
//                     Ok(files) => {
//                         //if !is_current(&file) { return }
//                         let len = files.len();
//                         //if len == 0 { return };
//                         let mut file_list = ListView::new(files);
//                         file_list.set_coordinates(&coordinates);
//                         file_list.refresh();
//                         //if !is_current(&file) { return }
//                         file_list.animate_slide_up();
//                         return Box::new(file_list)

//                     }
//                     Err(err) => {
//                         write!(bufout, "{}", redraw).unwrap();
//                         let textview = crate::textview::TextView {
//                             lines: vec![],
//                             buffer: "".to_string(),
//                             coordinates: Coordinates::new(),
//                         };
//                         return Box::new(textview)
//                     },
//                 }
//                 _ => {
//                     if file.get_mime() == Some("text".to_string()) {
//                         let lines = coordinates.ysize() as usize;
//                         let mut textview
//                             = TextView::new_from_file_limit_lines(&file,
//                                                                   lines);
//                         //if !is_current(&file) { return }
//                         textview.set_coordinates(&coordinates);
//                         textview.refresh();
//                         //if !is_current(&file) { return }
//                         textview.animate_slide_up();
//                         return Box::new(textview)
//                     } else {
//                         let process =
//                             std::process::Command::new("scope.sh")
//                             .arg(&file.name)
//                             .arg("10".to_string())
//                             .arg("10".to_string())
//                             .arg("".to_string())
//                             .arg("false".to_string())
//                             .stdin(std::process::Stdio::null())
//                             .stdout(std::process::Stdio::piped())
//                             .stderr(std::process::Stdio::null())
//                             .spawn().unwrap();

//                         let pid = process.id();
//                         PIDS.lock().unwrap().push(pid);

//                         //if !is_current(&file) { return }

//                         let output = process.wait_with_output();
//                         match output {
//                             Ok(output) => {
//                                 let status = output.status.code();
//                                 match status {
//                                     Some(status) => {
//                                         if status == 0 || status == 5 && is_current(&file) {
//                                             let output = std::str::from_utf8(&output.stdout)
//                                                 .unwrap()
//                                                 .to_string();
//                                             let mut textview = TextView {
//                                                 lines: output.lines().map(|s| s.to_string()).collect(),
//                                                 buffer: String::new(),
//                                                 coordinates: Coordinates::new() };
//                                             textview.set_coordinates(&coordinates);
//                                             textview.refresh();
//                                             textview.animate_slide_up();
//                                             return Box::new(textview)
//                                         }
//                                     }, None => {}
//                                 }
//                             }, Err(_) => {}
//                         }

//                         write!(bufout, "{}", redraw).unwrap();
//                         //std::io::stdout().flush().unwrap();
//                         let textview = crate::textview::TextView {
//                             lines: vec![],
//                             buffer: "".to_string(),
//                             coordinates: Coordinates::new(),
//                         };
//                         return Box::new(textview)
//                     }
//                 }
//             }}))
//     }
// }





impl<T> Widget for Box<T> where T: Widget + ?Sized {
    fn get_coordinates(&self) -> &Coordinates {
        (**self).get_coordinates()
    }
    fn set_coordinates(&mut self, coordinates: &Coordinates) {
        if (**self).get_coordinates() == coordinates {
            return;
        }
        (**self).set_coordinates(&coordinates);
        (**self).refresh();
    }
    fn render_header(&self) -> String {
        (**self).render_header()
    }
    fn refresh(&mut self) {
        (**self).refresh()
    }
    fn get_drawlist(&self) -> String {
        (**self).get_drawlist()
    }
}
