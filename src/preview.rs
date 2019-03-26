use std::sync::{Arc, Mutex, RwLock};

use rayon::ThreadPool;

use crate::files::{File, Files, Kind};
use crate::listview::ListView;
use crate::textview::TextView;
use crate::widget::{Widget, WidgetCore};
use crate::coordinates::Coordinates;
use crate::fail::{HResult, HError, ErrorLog};


pub type AsyncValueFn<T> = Box<Fn(Stale) -> HResult<T> + Send>;
pub type AsyncValue<T> = Arc<Mutex<Option<HResult<T>>>>;
pub type AsyncReadyFn = Box<Fn() -> HResult<()> + Send>;
pub type AsyncWidgetFn<W> = Box<Fn(Stale, WidgetCore) -> HResult<W> + Send>;


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

#[derive(Clone, Debug)]
pub struct Stale(Arc<RwLock<bool>>);

impl Stale {
    pub fn new() -> Stale {
        Stale(Arc::new(RwLock::new(false)))
    }
    pub fn is_stale(&self) -> HResult<bool> {
        Ok(*self.0.read()?)
    }
    pub fn set_stale(&self) -> HResult<()> {
        *self.0.write()? = true;
        Ok(())
    }
    pub fn set_fresh(&self) -> HResult<()> {
        *self.0.write()? = false;
        Ok(())
    }
}



pub fn is_stale(stale: &Stale) -> HResult<bool> {
    let stale = stale.is_stale()?;
    Ok(stale)
}

use std::fmt::{Debug, Formatter};

impl<T: Send + Debug> Debug for Async<T> {
    fn fmt(&self, formatter: &mut Formatter) -> Result<(), std::fmt::Error> {
        write!(formatter,
               "{:?}, {:?} {:?}",
               self.value,
               self.async_value,
               self.stale)
    }
}


#[derive(Clone)]
pub struct Async<T: Send> {
    pub value: HResult<T>,
    async_value: AsyncValue<T>,
    async_closure: Arc<Mutex<Option<AsyncValueFn<T>>>>,
    on_ready: Arc<Mutex<Option<AsyncReadyFn>>>,
    started: bool,
    stale: Stale,
}

impl<T: Send + 'static> Async<T> {
    pub fn new(closure: AsyncValueFn<T>)
                  -> Async<T> {
        let async_value = Async {
            value: HError::async_not_ready(),
            async_value: Arc::new(Mutex::new(None)),
            async_closure: Arc::new(Mutex::new(Some(closure))),
            on_ready: Arc::new(Mutex::new(None)),
            started: false,
            stale: Stale::new() };

        async_value
    }

    pub fn new_with_stale(closure: AsyncValueFn<T>,
                          stale: Stale)
                  -> Async<T> {
        let async_value = Async {
            value: HError::async_not_ready(),
            async_value: Arc::new(Mutex::new(None)),
            async_closure: Arc::new(Mutex::new(Some(closure))),
            on_ready: Arc::new(Mutex::new(None)),
            started: false,
            stale: stale };

        async_value
    }

    pub fn new_with_value(val: T) -> Async<T> {
        Async {
            value: Ok(val),
            async_value: Arc::new(Mutex::new(None)),
            async_closure: Arc::new(Mutex::new(None)),
            on_ready: Arc::new(Mutex::new(None)),
            started: false,
            stale: Stale::new()
        }
    }

    pub fn run(&mut self) -> HResult<()> {
        if self.started {
            HError::async_started()?
        }

        let closure = self.async_closure.clone();
        let async_value = self.async_value.clone();
        let stale = self.stale.clone();
        let on_ready_fn = self.on_ready.clone();
        self.started = true;

        std::thread::spawn(move|| -> HResult<()> {
        let value = closure.lock().map(|closure|
                                           closure.as_ref().map(|closure|
                                                                closure(stale)));

            if let Ok(value) = value {
                if let Some(value) = value {
                    match value {
                        Ok(mut value) => {
                            async_value
                                .lock()
                                .map(|mut async_value|
                                     async_value.replace(Ok(value))).ok();

                            if let Ok(mut on_ready_fn) = on_ready_fn.lock() {
                                if let Some(on_ready_fn) = on_ready_fn.as_ref() {
                                    on_ready_fn().log();
                                }
                                on_ready_fn.take();
                            }

                            closure.lock().map(|mut closure|
                                               closure.take()).ok();
                        },
                        Err(err) => {
                            async_value
                                .lock()
                                .map(|mut async_value|
                                     async_value.replace(Err(err))).ok();
                        }
                    }
                }
            } else {
                async_value
                    .lock()
                    .map(|mut async_value|
                         async_value.replace(Err(HError::MutexError))).ok();
            }
            Ok(())
        });
        Ok(())
    }

    pub fn run_pooled(&mut self, pool: &ThreadPool) -> HResult<()> {
        if self.started {
            HError::async_started()?
        }

        let closure = self.async_closure.clone();
        let async_value = self.async_value.clone();
        let stale = self.stale.clone();
        let on_ready_fn = self.on_ready.clone();
        self.started = true;

        pool.spawn(move || {
            let value = closure.lock().map(|closure|
                                           closure.as_ref().map(|closure|
                                                                closure(stale)));

            if let Ok(value) = value {
                if let Some(value) = value {
                    match value {
                        Ok(mut value) => {
                            async_value
                                .lock()
                                .map(|mut async_value|
                                     async_value.replace(Ok(value))).ok();
                            if let Ok(mut on_ready_fn) = on_ready_fn.lock() {
                                if let Some(on_ready_fn) = on_ready_fn.as_ref() {
                                    on_ready_fn().log();
                                }
                                on_ready_fn.take();
                            }
                            closure.lock().map(|mut closure|
                                               closure.take()).ok();
                        },
                        Err(err) => {
                            async_value
                                .lock()
                                .map(|mut async_value|
                                     async_value.replace(Err(err))).ok();
                        }
                    }
                }
            } else {
                async_value
                    .lock()
                    .map(|mut async_value|
                         async_value.replace(Err(HError::MutexError))).ok();
            }
        });
        Ok(())
    }


    pub fn wait(&mut self) -> HResult<()> {
        let closure = self.async_closure.lock()?.take()?;
        let mut value = closure(self.stale.clone());
        let on_ready_fn = self.on_ready.lock()?.take();


        if let Ok(ref mut value) = value {
            on_ready_fn.map(|on_ready_fn| on_ready_fn());
        }

        self.value = value;
        Ok(())
    }

    pub fn set_stale(&mut self) -> HResult<()> {
        self.stale.set_stale()?;
        Ok(())
    }

    pub fn set_fresh(&self) -> HResult<()> {
        self.stale.set_fresh()?;
        Ok(())
    }

    pub fn is_stale(&self) -> HResult<bool> {
        self.stale.is_stale()
    }

    pub fn get_stale(&self) -> Stale {
        self.stale.clone()
    }

    pub fn is_started(&self) -> bool {
        self.started
    }

    pub fn set_unstarted(&mut self) {
        self.started = false;
    }

    pub fn take_async(&mut self) -> HResult<()> {
        if self.value.is_ok() { HError::async_taken()? }

        let mut async_value = self.async_value.lock()?;
        match async_value.as_ref() {
            Some(Ok(_)) => {
                let value = async_value.take()?;
                self.value = value;
            }
            Some(Err(HError::AsyncAlreadyTakenError)) => HError::async_taken()?,
            Some(Err(_)) => {
                let value = async_value.take()?;
                self.value = value;
            }
            None => HError::async_not_ready()?,
        }
        Ok(())
    }

    pub fn get(&self) -> HResult<&T> {
        match self.value {
            Ok(ref value) => Ok(value),
            Err(ref err) => HError::async_error(err)
        }
    }

    pub fn get_mut(&mut self) -> HResult<&mut T> {
        self.take_async().ok();

        match self.value {
            Ok(ref mut value) => Ok(value),
            Err(ref err) => HError::async_error(err)
        }
    }

    pub fn on_ready(&mut self,
                    fun: AsyncReadyFn) {
        *self.on_ready.lock().unwrap() = Some(fun);
    }
}

impl<W: Widget + Send + 'static> PartialEq for AsyncWidget<W> {
    fn eq(&self, other: &AsyncWidget<W>) -> bool {
        if self.get_coordinates().unwrap() ==
            other.get_coordinates().unwrap() {
            true
        } else {
            false
        }
    }
}


pub struct AsyncWidget<W: Widget + Send + 'static> {
    widget: Async<W>,
    core: WidgetCore
}

impl<W: Widget + Send + 'static> AsyncWidget<W> {
    pub fn new(core: &WidgetCore, closure: AsyncValueFn<W>) -> AsyncWidget<W> {
        let sender = core.get_sender();
        let mut widget = Async::new(Box::new(move |stale| closure(stale)));
        widget.on_ready(Box::new(move || {
            sender.send(crate::widget::Events::WidgetReady)?;
            Ok(())
        }));
        widget.run().log();

        AsyncWidget {
            widget: widget,
            core: core.clone()
        }
    }
    pub fn change_to(&mut self, closure: AsyncWidgetFn<W>) -> HResult<()> {
        self.set_stale().log();

        let sender = self.get_core()?.get_sender();
        let core = self.get_core()?.clone();

        let mut widget = Async::new(Box::new(move |stale| {
            closure(stale, core.clone())
        }));

        widget.on_ready(Box::new(move || {
            sender.send(crate::widget::Events::WidgetReady)?;
            Ok(())
        }));

        widget.run().log();

        self.widget = widget;
        Ok(())
    }

    pub fn set_stale(&mut self) -> HResult<()> {
        self.widget.set_stale()
    }

    pub fn is_stale(&self) -> HResult<bool> {
        self.widget.is_stale()
    }

    pub fn get_stale(&self) -> Stale {
        self.widget.get_stale()
    }

    pub fn widget(&self) -> HResult<&W> {
        self.widget.get()
    }

    pub fn widget_mut(&mut self) -> HResult<&mut W> {
        self.widget.get_mut()
    }

    pub fn ready(&self) -> bool {
        self.widget().is_ok()
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

impl<T: Widget + Send + 'static> Widget for AsyncWidget<T> {
    fn get_core(&self) -> HResult<&WidgetCore> {
        Ok(&self.core)
    }
    fn get_core_mut(&mut self) -> HResult<&mut WidgetCore> {
        Ok(&mut self.core)
    }

    fn set_coordinates(&mut self, coordinates: &Coordinates) -> HResult<()> {
        self.core.coordinates = coordinates.clone();
        if let Ok(widget) = self.widget_mut() {
            widget.set_coordinates(&coordinates)?;
        }
        Ok(())
    }

    fn refresh(&mut self) -> HResult<()> {
        self.widget.take_async().ok();

        let coords = self.get_coordinates()?.clone();
        if let Ok(widget) = self.widget_mut() {
            if widget.get_coordinates()? != &coords {
                widget.set_coordinates(&coords)?;
                widget.refresh()?;
            } else {
                widget.refresh()?;
            }
        }
        Ok(())
    }
    fn get_drawlist(&self) -> HResult<String> {
        if self.widget().is_err() {
            let clear = self.get_clearlist()?;
            let (xpos, ypos) = self.get_coordinates()?.u16position();
            let pos = crate::term::goto_xy(xpos, ypos);
            return Ok(clear + &pos + "...")
        }

        if self.is_stale()? {
            return self.get_clearlist()
        }

        self.widget()?.get_drawlist()
    }
    fn on_key(&mut self, key: termion::event::Key) -> HResult<()> {
        if self.widget().is_err() { return Ok(()) }
        self.widget_mut()?.on_key(key)
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
    widget: AsyncWidget<Box<dyn Widget + Send>>,
    core: WidgetCore,
    file: Option<File>,
    selection: Option<File>,
    cached_files: Option<Files>,
}


impl Previewer {
    pub fn new(core: &WidgetCore) -> Previewer {
        let core_ = core.clone();
        let widget = AsyncWidget::new(&core, Box::new(move |_| {
            Ok(Box::new(TextView::new_blank(&core_)) as Box<dyn Widget + Send>)
        }));
        Previewer { widget: widget,
                    core: core.clone(),
                    file: None,
                    selection: None,
                    cached_files: None }
    }

    fn become_preview(&mut self,
                      widget: HResult<AsyncWidget<WidgetO>>) -> HResult<()> {
        let coordinates = self.get_coordinates()?.clone();
        self.widget =  widget?;
        self.widget.set_coordinates(&coordinates)?;
        Ok(())
    }

    pub fn set_stale(&mut self) -> HResult<()> {
        self.widget.set_stale()
    }

    pub fn set_file(&mut self,
                    file: &File,
                    selection: Option<File>,
                    cached_files: Option<Files>) -> HResult<()> {
        if Some(file) == self.file.as_ref() && !self.widget.is_stale()? { return Ok(()) }
        self.file = Some(file.clone());
        self.selection = selection.clone();
        self.cached_files = cached_files.clone();

        let coordinates = self.get_coordinates().unwrap().clone();
        let file = file.clone();
        let core = self.core.clone();

        self.widget.set_stale().ok();

        self.become_preview(Ok(AsyncWidget::new(&self.core,
                                                Box::new(move |stale| {
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
        }))))
    }

    pub fn reload(&mut self) {
        if let Some(file) = self.file.clone() {
            self.file = None;
            let cache = self.cached_files.take();
            self.set_file(&file, self.selection.clone(), cache).log();
        }
    }

    fn preview_failed(file: &File) -> HResult<WidgetO> {
        HError::preview_failed(file)
    }

    fn preview_dir(file: &File,
                   selection: Option<File>,
                   cached_files: Option<Files>,
                   core: &WidgetCore,
                   stale: Stale)
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

    fn preview_text(file: &File, core: &WidgetCore, stale: Stale)
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

    fn preview_external(file: &File,
                        core: &WidgetCore,
                        stale: Stale)
                        -> Result<Box<dyn Widget + Send>, HError> {
        let process =
            std::process::Command::new("scope.sh")
            .arg(&file.path)
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

    fn set_coordinates(&mut self, coordinates: &Coordinates) -> HResult<()> {
        self.core.coordinates = coordinates.clone();
        self.widget.set_coordinates(&coordinates)
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
