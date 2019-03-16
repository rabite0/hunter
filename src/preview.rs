use std::sync::{Arc, Mutex};

use failure::Backtrace;

use crate::files::{File, Files, Kind};
use crate::listview::ListView;
use crate::textview::TextView;
use crate::widget::{Widget, WidgetCore};
use crate::fail::{HResult, HError, ErrorLog};



type HClosure<T> = Box<Fn(Arc<Mutex<bool>>) -> Result<T, HError> + Send>;
type HCClosure<T> = Box<Fn(Arc<Mutex<bool>>, WidgetCore) -> Result<T, HError> + Send>;
type WidgetO = Box<dyn Widget + Send>;

lazy_static! {
    static ref SUBPROC: Arc<Mutex<Option<u32>>> = { Arc::new(Mutex::new(None)) };
}

fn kill_proc() -> HResult<()> {
    let mut pid = SUBPROC.lock()?;
    pid.map(|pid|
        unsafe { libc::kill(pid as i32, 15); }
    );
    *pid = None;
    Ok(())
}

pub fn is_stale(stale: &Arc<Mutex<bool>>) -> HResult<bool> {
    let stale = *(stale.try_lock().unwrap());
    Ok(stale)
}

enum State {
    Is,
    Becoming,
    Fail(HError)
}

struct WillBe<T: Send> {
    pub state: Arc<Mutex<State>>,
    pub thing: Arc<Mutex<Option<T>>>,
    on_ready: Arc<Mutex<Option<Box<Fn(Arc<Mutex<Option<T>>>) -> HResult<()> + Send>>>>,
    stale: Arc<Mutex<bool>>
}

impl<T: Send + 'static> WillBe<T> where {
    pub fn new_become(closure: HClosure<T>)
                  -> WillBe<T> {
        let mut willbe = WillBe { state: Arc::new(Mutex::new(State::Becoming)),
                                  thing: Arc::new(Mutex::new(None)),
                                  on_ready: Arc::new(Mutex::new(None)),
                                  stale: Arc::new(Mutex::new(false)) };
        willbe.run(closure);
        willbe
    }

    fn run(&mut self, closure: HClosure<T>) {
        let state = self.state.clone();
        let stale = self.stale.clone();
        let thing = self.thing.clone();
        let on_ready_fn = self.on_ready.clone();
        std::thread::spawn(move|| {
            let got_thing = closure(stale);
            match got_thing {
                Ok(got_thing) => {
                    *thing.lock().unwrap() = Some(got_thing);
                    *state.lock().unwrap() = State::Is;
                    match *on_ready_fn.lock().unwrap() {
                        Some(ref on_ready) => { on_ready(thing.clone()).ok(); },
                        None => {}
                    }
                },
                Err(err) => *state.lock().unwrap() = State::Fail(err)
            }
        });
    }

    pub fn set_stale(&mut self) -> HResult<()> {
        *self.stale.lock()? = true;
        Ok(())
    }

    pub fn take(&mut self) -> HResult<T> {
        self.check()?;
        Ok(self.thing.lock()?.take()?)
    }

    pub fn check(&self) -> HResult<()> {
        match *self.state.lock()? {
            State::Is => Ok(()),
            _ => Err(HError::WillBeNotReady(Backtrace::new()))
        }
    }

    pub fn on_ready(&mut self,
                    fun: Box<Fn(Arc<Mutex<Option<T>>>) -> HResult<()> + Send>)
                    -> HResult<()> {
        if self.check().is_ok() {
            fun(self.thing.clone())?;
        } else {
            *self.on_ready.lock()? = Some(fun);
        }
        Ok(())
    }
}

impl<W: Widget + Send + 'static> PartialEq for WillBeWidget<W> {
    fn eq(&self, other: &WillBeWidget<W>) -> bool {
        if self.get_coordinates().unwrap() ==
            other.get_coordinates().unwrap() {
            true
        } else {
            false
        }
    }
}

pub struct WillBeWidget<T: Widget + Send + 'static> {
    willbe: WillBe<T>,
    core: WidgetCore
}

impl<T: Widget + Send + 'static> WillBeWidget<T> {
    pub fn new(core: &WidgetCore, closure: HClosure<T>) -> WillBeWidget<T> {
        let sender = core.get_sender();
        let mut willbe = WillBe::new_become(Box::new(move |stale| closure(stale)));
        willbe.on_ready(Box::new(move |_| {
            sender.send(crate::widget::Events::WidgetReady)?;
            Ok(()) })).ok();

        WillBeWidget {
            willbe: willbe,
            core: core.clone()
        }
    }
    pub fn change_to(&mut self, closure: HCClosure<T>) -> HResult<()> {
        self.set_stale().log();

        let sender = self.get_core()?.get_sender();
        let core = self.get_core()?.clone();

        let mut willbe = WillBe::new_become(Box::new(move |stale| {
            let core = core.clone();
            closure(stale, core)
        }));
        willbe.on_ready(Box::new(move |_| {
            sender.send(crate::widget::Events::WidgetReady)?;
            Ok(())
        }))?;

        self.willbe = willbe;
        Ok(())
    }
    pub fn set_stale(&mut self) -> HResult<()> {
        self.willbe.set_stale()
    }
    pub fn widget(&self) -> HResult<Arc<Mutex<Option<T>>>> {
        self.willbe.check()?;
        Ok(self.willbe.thing.clone())
    }
    pub fn ready(&self) -> bool {
        match self.willbe.check() {
            Ok(_) => true,
            _ => false
        }
    }
    pub fn take(&mut self) -> HResult<T> {
        self.willbe.take()
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

impl<T: Widget + Send + 'static> Widget for WillBeWidget<T> {
    fn get_core(&self) -> HResult<&WidgetCore> {
        Ok(&self.core)
    }
    fn get_core_mut(&mut self) -> HResult<&mut WidgetCore> {
        Ok(&mut self.core)
    }
    fn refresh(&mut self) -> HResult<()> {
        if let Ok(widget) = self.widget() {
            let mut widget = widget.lock()?;
            let widget = widget.as_mut()?;
            widget.set_coordinates(self.get_coordinates()?).log();
            widget.refresh().log();
        }
        Ok(())
    }
    fn get_drawlist(&self) -> HResult<String> {
        if self.willbe.check().is_err() {
            let clear = self.get_clearlist()?;
            let (xpos, ypos) = self.get_coordinates()?.u16position();
            let pos = crate::term::goto_xy(xpos, ypos);
            return Ok(clear + &pos + "...")
        }
        if is_stale(&self.willbe.stale)? {
            return self.get_clearlist()
        }
        let widget = self.widget()?;
        let widget = widget.lock()?;
        let widget = widget.as_ref()?;
        widget.get_drawlist()
    }
    fn on_key(&mut self, key: termion::event::Key) -> HResult<()> {
        if self.willbe.check().is_err() { return Ok(()) }
        let widget = self.widget()?;
        let mut widget = widget.lock()?;
        let widget = widget.as_mut()?;
        widget.on_key(key)
    }
}


impl PartialEq for Previewer {
    fn eq(&self, other: &Previewer) -> bool {
        if self.widget.get_coordinates().unwrap() ==
            other.widget.get_coordinates().unwrap() {
            true
        } else {
            false
        }
    }
}

pub struct Previewer {
    widget: WillBeWidget<Box<dyn Widget + Send>>,
    core: WidgetCore,
    file: Option<File>,
    selection: Option<File>,
    cached_files: Option<Files>
}


impl Previewer {
    pub fn new(core: &WidgetCore) -> Previewer {
        let core_ = core.clone();
        let willbe = WillBeWidget::new(&core, Box::new(move |_| {
            Ok(Box::new(crate::textview::TextView::new_blank(&core_))
               as Box<dyn Widget + Send>)
        }));
        Previewer { widget: willbe,
                    core: core.clone(),
                    file: None,
                    selection: None,
                    cached_files: None }
    }

    fn become_preview(&mut self,
                      widget: HResult<WillBeWidget<WidgetO>>) {
        let coordinates = self.get_coordinates().unwrap().clone();
        self.widget =  widget.unwrap();
        self.widget.set_coordinates(&coordinates).ok();
    }

    pub fn set_file(&mut self,
                    file: &File,
                    selection: Option<File>,
                    cached_files: Option<Files>) {
        if Some(file) == self.file.as_ref() { return }
        self.file = Some(file.clone());
        self.selection = selection.clone();
        self.cached_files = cached_files.clone();

        let coordinates = self.get_coordinates().unwrap().clone();
        let file = file.clone();
        let core = self.core.clone();

        self.widget.set_stale().ok();

        self.become_preview(Ok(WillBeWidget::new(&self.core, Box::new(move |stale| {
            kill_proc().unwrap();

            let file = file.clone();
            let selection = selection.clone();
            let cached_files = cached_files.clone();

            if file.kind == Kind::Directory  {
                let preview = Previewer::preview_dir(&file,
                                                     selection,
                                                     cached_files,
                                                     &core,
                                                     stale.clone());
                return preview;
            }

            if file.get_mime() == Some("text".to_string()) {
                return Previewer::preview_text(&file, &core, stale.clone())
            }

            let preview = Previewer::preview_external(&file, &core, stale.clone());
            if preview.is_ok() { return preview; }
            else {
                let mut blank = Box::new(TextView::new_blank(&core));
                blank.set_coordinates(&coordinates).log();
                blank.refresh().log();
                blank.animate_slide_up().log();
                return Ok(blank)
            }
        }))));
    }

    pub fn reload(&mut self) {
        if let Some(file) = self.file.clone() {
            self.file = None;
            let cache = self.cached_files.take();
            self.set_file(&file, self.selection.clone(), cache);
        }
    }

    fn preview_failed(file: &File) -> HResult<WidgetO> {
        HError::preview_failed(file)
    }

    fn preview_dir(file: &File,
                   selection: Option<File>,
                   cached_files: Option<Files>,
                   core: &WidgetCore,
                   stale: Arc<Mutex<bool>>)
                   -> Result<WidgetO, HError> {
        let files = cached_files.or_else(|| {
            Files::new_from_path_cancellable(&file.path,
                                             stale.clone()).ok()
        })?;
        let len = files.len();

        if len == 0 || is_stale(&stale)? { return Previewer::preview_failed(&file) }

        let mut file_list = ListView::new(&core, files);
        if let Some(selection) = selection {
            file_list.select_file(&selection);
        }
        file_list.set_coordinates(&core.coordinates)?;
        file_list.refresh()?;
        if is_stale(&stale)? { return Previewer::preview_failed(&file) }
        file_list.animate_slide_up()?;
        Ok(Box::new(file_list) as Box<dyn Widget + Send>)
    }

    fn preview_text(file: &File, core: &WidgetCore, stale: Arc<Mutex<bool>>)
                    -> HResult<WidgetO> {
        let lines = core.coordinates.ysize() as usize;
        let mut textview
            = TextView::new_from_file_limit_lines(&core,
                                                  &file,
                                                  lines)?;
        if is_stale(&stale)? { return Previewer::preview_failed(&file) }

        textview.set_coordinates(&core.coordinates)?;
        textview.refresh()?;

        if is_stale(&stale)? { return Previewer::preview_failed(&file) }

        textview.animate_slide_up()?;
        Ok(Box::new(textview))
    }

    fn preview_external(file: &File, core: &WidgetCore, stale: Arc<Mutex<bool>>)
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

        if is_stale(&stale)? { return Previewer::preview_failed(&file) }

        let output = process.wait_with_output()?;

        if is_stale(&stale)? { return Previewer::preview_failed(&file) }
        {
            let mut pid_ = SUBPROC.lock()?;
            *pid_ = None;
        }

        let status = output.status.code()
            .ok_or(HError::preview_failed(file)?);

        if status == Ok(0) || status == Ok(5) && !is_stale(&stale)? {
            let output = std::str::from_utf8(&output.stdout)
                .unwrap()
                .to_string();
            let mut textview = TextView {
                lines: output.lines().map(|s| s.to_string()).collect(),
                buffer: String::new(),
                core: core.clone(),
                follow: false,
                offset: 0};
            textview.set_coordinates(&core.coordinates).log();
            textview.refresh().log();
            textview.animate_slide_up().log();
            return Ok(Box::new(textview))
        }
        HError::preview_failed(file)
    }

}



impl Widget for Previewer {
    fn get_core(&self) -> HResult<&WidgetCore> {
        Ok(&self.core)
    }
    fn get_core_mut(&mut self) -> HResult<&mut WidgetCore> {
        Ok(&mut self.core)
    }
    fn refresh(&mut self) -> HResult<()> {
        self.widget.refresh()
    }
    fn get_drawlist(&self) -> HResult<String> {
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
    fn get_core(&self) -> HResult<&WidgetCore> {
        Ok((**self).get_core()?)
    }
    fn get_core_mut(&mut self) -> HResult<&mut WidgetCore> {
        Ok((**self).get_core_mut()?)
    }
    fn render_header(&self) -> HResult<String> {
        (**self).render_header()
    }
    fn refresh(&mut self) -> HResult<()> {
        (**self).refresh()
    }
    fn get_drawlist(&self) -> HResult<String> {
        (**self).get_drawlist()
    }
}
