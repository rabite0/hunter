use std::sync::{Arc, Mutex, RwLock};
use std::boxed::FnBox;

use rayon::ThreadPool;

use crate::files::{File, Files, Kind};
use crate::fscache::FsCache;
use crate::listview::ListView;
use crate::textview::TextView;
use crate::widget::{Widget, WidgetCore};
use crate::coordinates::Coordinates;
use crate::fail::{HResult, HError, ErrorLog};


pub type AsyncValueFn<T> = Box<dyn FnBox(Stale) -> HResult<T> + Send + Sync>;
pub type AsyncValue<T> = Arc<Mutex<Option<HResult<T>>>>;
pub type AsyncReadyFn = Box<dyn FnBox() -> HResult<()> + Send + Sync>;
pub type AsyncWidgetFn<W> = Box<dyn FnBox(Stale, WidgetCore)
                                          -> HResult<W> + Send + Sync>;


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

    pub fn run_async(async_fn: Arc<Mutex<Option<AsyncValueFn<T>>>>,
                     async_value: AsyncValue<T>,
                     on_ready_fn: Arc<Mutex<Option<AsyncReadyFn>>>,
                     stale: Stale) -> HResult<()> {
        let value_fn = async_fn.lock()?.take()?;
        let value = value_fn.call_box((stale.clone(),));
        async_value.lock()?.replace(value);
        on_ready_fn.lock()?
            .take()
            .map(|on_ready| on_ready.call_box(()).log());
        Ok(())
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

        std::thread::spawn(move || {
            Async::run_async(closure,
                             async_value,
                             on_ready_fn,
                             stale).log();
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
            Async::run_async(closure,
                             async_value,
                             on_ready_fn,
                             stale).log();
        });

        Ok(())
    }


    pub fn wait(self) -> HResult<T> {
        Async::run_async(self.async_closure,
                         self.async_value.clone(),
                         self.on_ready,
                         self.stale).log();
        let value = self.async_value.lock()?.take()?;
        value
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

    pub fn put_stale(&mut self, stale: Stale) {
        self.stale = stale;
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
        let sender = Mutex::new(core.get_sender());
        let mut widget = Async::new(Box::new(move |stale|
                                             closure.call_box((stale,))));
        widget.on_ready(Box::new(move || {
            sender.lock()?.send(crate::widget::Events::WidgetReady)?;
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

        let sender = Mutex::new(self.get_core()?.get_sender());
        let core = self.get_core()?.clone();

        let mut widget = Async::new(Box::new(move |stale| {
            closure.call_box((stale, core.clone(),))
        }));

        widget.on_ready(Box::new(move || {
            sender.lock()?.send(crate::widget::Events::WidgetReady)?;
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

    pub fn take_widget(self) -> HResult<W> {
        Ok(self.widget.value?)
    }

    pub fn ready(&self) -> bool {
        self.widget().is_ok()
    }
}



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

#[derive(PartialEq)]
enum PreviewWidget {
    FileList(ListView<Files>),
    TextView(TextView)
}



pub struct Previewer {
    widget: AsyncWidget<PreviewWidget>,
    core: WidgetCore,
    file: Option<File>,
    pub cache: FsCache,
}


impl Previewer {
    pub fn new(core: &WidgetCore, cache: FsCache) -> Previewer {
        let core_ = core.clone();
        let widget = AsyncWidget::new(&core, Box::new(move |_| {
            let blank = TextView::new_blank(&core_);
            let blank = PreviewWidget::TextView(blank);
            Ok(blank)
        }));
        Previewer { widget: widget,
                    core: core.clone(),
                    file: None,
                    cache: cache }
    }

    fn become_preview(&mut self,
                      widget: HResult<AsyncWidget<PreviewWidget>>) -> HResult<()> {
        let coordinates = self.get_coordinates()?.clone();
        self.widget =  widget?;
        self.widget.set_coordinates(&coordinates)?;
        Ok(())
    }

    pub fn set_stale(&mut self) -> HResult<()> {
        self.widget.set_stale()
    }

    pub fn get_file(&self) -> Option<&File> {
        self.file.as_ref()
    }

    pub fn take_files(&mut self) -> HResult<Files> {
        let core = self.core.clone();
        let mut widget = AsyncWidget::new(&core.clone(), Box::new(move |_| {
            let widget = TextView::new_blank(&core);
            let widget = PreviewWidget::TextView(widget);
            Ok(widget)
        }));
        std::mem::swap(&mut self.widget, &mut widget);

        match widget.take_widget() {
            Ok(PreviewWidget::FileList(file_list)) => {
                let files = file_list.content;
                Ok(files)
            }
            _ => HError::no_files()?
        }
    }

    pub fn replace_file(&mut self, dir: &File,
                        old: Option<&File>,
                        new: Option<&File>) -> HResult<()> {
        if self.file.as_ref() != Some(dir) { return Ok(()) }
        self.widget.widget_mut().map(|widget| {
            match widget {
                PreviewWidget::FileList(filelist) => {
                    filelist.content.replace_file(old, new.cloned()).map(|_| {
                        filelist.refresh().ok();
                    }).ok();

                }
                _ => {}
            }
        })
    }

    pub fn put_preview_files(&mut self, files: Files) {
        let core = self.core.clone();
        let dir = files.directory.clone();
        let cache = self.cache.clone();
        self.file = Some(dir);

        self.widget = AsyncWidget::new(&self.core, Box::new(move |_| {
            let selected_file = cache.get_selection(&files.directory);
            let mut filelist = ListView::new(&core, files);

            selected_file.map(|file| filelist.select_file(&file));

            Ok(PreviewWidget::FileList(filelist))
        }));
    }

    pub fn set_file(&mut self,
                    file: &File) -> HResult<()> {
        if Some(file) == self.file.as_ref() && !self.widget.is_stale()? { return Ok(()) }
        self.file = Some(file.clone());

        let coordinates = self.get_coordinates().unwrap().clone();
        let file = file.clone();
        let core = self.core.clone();
        let cache = self.cache.clone();

        self.widget.set_stale().ok();

        self.become_preview(Ok(AsyncWidget::new(&self.core,
                                                Box::new(move |stale: Stale| {
            kill_proc().unwrap();

            if file.kind == Kind::Directory  {
                let preview = Previewer::preview_dir(&file,
                                                     cache,
                                                     &core,
                                                     stale);
                return preview;
            }

            if file.is_text() {
                return Previewer::preview_text(&file, &core, stale)
            }

            let preview = Previewer::preview_external(&file, &core, stale);
            if preview.is_ok() { return preview; }
            else {
                let mut blank = TextView::new_blank(&core);
                blank.set_coordinates(&coordinates).log();
                blank.refresh().log();
                blank.animate_slide_up().log();
                return Ok(PreviewWidget::TextView(blank))
            }
        }))))
    }

    pub fn reload(&mut self) {
        if let Some(file) = self.file.clone() {
            self.file = None;
            self.set_file(&file).log();
        }
    }



    fn preview_failed(file: &File) -> HResult<PreviewWidget> {
        HError::preview_failed(file)
    }

    fn preview_dir(file: &File,
                   cache: FsCache,
                   core: &WidgetCore,
                   stale: Stale)
                   -> HResult<PreviewWidget> {
        let (selection, cached_files) = cache.get_files(&file, stale.clone())?;

        let files = cached_files.wait()?;

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
        Ok(PreviewWidget::FileList(file_list))
    }

    fn preview_text(file: &File, core: &WidgetCore, stale: Stale)
                    -> HResult<PreviewWidget> {
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
        Ok(PreviewWidget::TextView(textview))
    }

    fn preview_external(file: &File,
                        core: &WidgetCore,
                        stale: Stale)
                        -> HResult<PreviewWidget> {
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

        let output = dbg!(process.wait_with_output())?;

        if is_stale(&stale)? { return Previewer::preview_failed(&file) }
        {
            let mut pid_ = SUBPROC.lock()?;
            *pid_ = None;
        }

        let status = output.status.code()?;

        if !is_stale(&stale)? {
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
            return Ok(PreviewWidget::TextView(textview))
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

impl Widget for PreviewWidget {
    fn get_core(&self) -> HResult<&WidgetCore> {
        match self {
            PreviewWidget::FileList(widget) => widget.get_core(),
            PreviewWidget::TextView(widget) => widget.get_core()
        }
    }
    fn get_core_mut(&mut self) -> HResult<&mut WidgetCore> {
        match self {
            PreviewWidget::FileList(widget) => widget.get_core_mut(),
            PreviewWidget::TextView(widget) => widget.get_core_mut()
        }
    }
    fn set_coordinates(&mut self, coordinates: &Coordinates) -> HResult<()> {
        match self {
            PreviewWidget::FileList(widget) => widget.set_coordinates(coordinates),
            PreviewWidget::TextView(widget) => widget.set_coordinates(coordinates),
        }
    }
    fn refresh(&mut self) -> HResult<()> {
        match self {
            PreviewWidget::FileList(widget) => widget.refresh(),
            PreviewWidget::TextView(widget) => widget.refresh()
        }
    }
    fn get_drawlist(&self) -> HResult<String> {
        match self {
            PreviewWidget::FileList(widget) => widget.get_drawlist(),
            PreviewWidget::TextView(widget) => widget.get_drawlist()
        }
    }
}


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
