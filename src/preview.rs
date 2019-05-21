use async_value::{Async, Stale};
use termion::event::Key;

use std::sync::{Arc, Mutex};

use crate::files::{File, Files, Kind};
use crate::fscache::FsCache;
use crate::listview::ListView;
use crate::textview::TextView;
use crate::widget::{Widget, WidgetCore};
use crate::coordinates::Coordinates;
use crate::fail::{HResult, HError, ErrorLog};
use crate::dirty::Dirtyable;

#[cfg(feature = "img")]
use crate::imgview::ImgView;
#[cfg(feature = "video")]
use crate::mediaview::MediaView;


pub type AsyncWidgetFn<W> = FnOnce(&Stale, WidgetCore)
                                   -> HResult<W> + Send + Sync;

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
    pub widget: Async<W>,
    core: WidgetCore
}

impl<W: Widget + Send + 'static> AsyncWidget<W> {
    pub fn new(core: &WidgetCore,
               closure: impl FnOnce(&Stale) -> HResult<W> + Send + Sync + 'static)
               -> AsyncWidget<W> {
        let sender = Arc::new(Mutex::new(core.get_sender()));
        let mut widget = Async::new(move |stale|
                                    closure(stale).map_err(|e| e.into()));
        widget.on_ready(move |_, stale| {
            if !stale.is_stale()? {
                sender.lock().map(|s| s.send(crate::widget::Events::WidgetReady)).ok();
            }
            Ok(())
        }).log();
        widget.run().log();

        AsyncWidget {
            widget: widget,
            core: core.clone()
        }
    }
    pub fn change_to(&mut self,
                     closure: impl FnOnce(&Stale,
                                          WidgetCore)
                                          -> HResult<W> + Send + Sync + 'static)
                     -> HResult<()> {
        self.set_stale().log();

        let sender = Mutex::new(self.get_core()?.get_sender());
        let core = self.get_core()?.clone();

        let mut widget = Async::new(move |stale| {
            Ok(closure(stale, core.clone())?)
        });

        widget.on_ready(move |mut w, stale| {
            sender.lock().map(|s| s.send(crate::widget::Events::WidgetReady)).ok();
            Ok(())
        }).log();

        widget.run().log();

        self.widget = widget;
        Ok(())
    }

    pub fn set_stale(&mut self) -> HResult<()> {
        Ok(self.widget.set_stale()?)
    }

    pub fn is_stale(&self) -> HResult<bool> {
        Ok(self.widget.is_stale()?)
    }

    pub fn get_stale(&self) -> Stale {
        self.widget.get_stale()
    }

    pub fn widget(&self) -> HResult<&W> {
        Ok(self.widget.get()?)
    }

    pub fn widget_mut(&mut self) -> HResult<&mut W> {
        Ok(self.widget.get_mut()?)
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
        self.widget.pull_async().ok();

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
    TextView(TextView),
    #[cfg(feature = "img")]
    ImgView(ImgView),
    #[cfg(feature = "video")]
    MediaView(MediaView)
}


pub struct Previewer {
    widget: AsyncWidget<PreviewWidget>,
    core: WidgetCore,
    file: Option<File>,
    pub cache: FsCache,
    animator: Stale
}


impl Previewer {
    pub fn new(core: &WidgetCore, cache: FsCache) -> Previewer {
        let core_ = core.clone();
        let widget = AsyncWidget::new(&core, move |_| {
            let blank = TextView::new_blank(&core_);
            let blank = PreviewWidget::TextView(blank);
            Ok(blank)
        });

        Previewer { widget: widget,
                    core: core.clone(),
                    file: None,
                    cache: cache,
                    animator: Stale::new()}
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

    pub fn cancel_animation(&self) -> HResult<()> {
        Ok(self.animator.set_stale()?)
    }

    pub fn take_files(&mut self) -> HResult<Files> {
        let core = self.core.clone();
        let mut widget = AsyncWidget::new(&core.clone(), move |_| {
            let widget = TextView::new_blank(&core);
            let widget = PreviewWidget::TextView(widget);
            Ok(widget)
        });
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

        self.widget = AsyncWidget::new(&self.core, move |_| {
            let selected_file = cache.get_selection(&files.directory);
            let mut filelist = ListView::new(&core, files);

            selected_file.map(|file| filelist.select_file(&file)).log();

            Ok(PreviewWidget::FileList(filelist))
        });
    }

    pub fn set_file(&mut self,
                    file: &File) -> HResult<()> {
        if Some(file) == self.file.as_ref() && !self.widget.is_stale()? { return Ok(()) }

        let same_dir = self.file
            .as_ref()
            .map(|f| f.path.parent() == file.path.parent())
            .unwrap_or(true);
        self.file = Some(file.clone());

        let coordinates = self.get_coordinates().unwrap().clone();
        let file = file.clone();
        let core = self.core.clone();
        let cache = self.cache.clone();
        let animator = self.animator.clone();

        self.widget.set_stale().ok();

        if same_dir {
            self.animator.set_fresh().ok();
        } else {
            self.animator.set_stale().ok();
        }

        self.become_preview(Ok(AsyncWidget::new(
            &self.core,
            move |stale: &Stale|
            {
                kill_proc().unwrap();

                if file.kind == Kind::Directory  {
                    let preview = Previewer::preview_dir(&file,
                                                         cache,
                                                         &core,
                                                         &stale,
                                                         &animator);
                    return Ok(preview?);
                }

                if file.is_text() {
                    return Ok(Previewer::preview_text(&file,
                                                      &core,
                                                      &stale,
                                                      &animator)?);
                }

                if let Some(mime) = file.get_mime() {
                    let mime_type = mime.type_().as_str();
                    let is_gif = mime.subtype() == "gif";

                    match mime_type {
                       #[cfg(feature = "video")]
                        _ if mime_type == "video" || is_gif => {
                            let media_type = crate::mediaview::MediaType::Video;
                            let mediaview = MediaView::new_from_file(core.clone(),
                                                                     &file.path,
                                                                     media_type);
                            return Ok(PreviewWidget::MediaView(mediaview));
                        }
                        #[cfg(feature = "img")]
                        "image" => {
                            let imgview = ImgView::new_from_file(core.clone(),
                                                                 &file.path())?;
                            return Ok(PreviewWidget::ImgView(imgview));
                        }
                        #[cfg(feature = "video")]
                        "audio" => {
                            let media_type = crate::mediaview::MediaType::Audio;
                            let mediaview = MediaView::new_from_file(core.clone(),
                                                                     &file.path,
                                                                     media_type);
                            return Ok(PreviewWidget::MediaView(mediaview));
                        }
                        _ => {}
                    }
                }


                let preview = Previewer::preview_external(&file,
                                                          &core,
                                                          &stale,
                                                          &animator);
                if preview.is_ok() { return Ok(preview?); }
                else {
                    let mut blank = TextView::new_blank(&core);
                    blank.set_coordinates(&coordinates).log();
                    blank.refresh().log();
                    blank.animate_slide_up(Some(&animator)).log();
                    return Ok(PreviewWidget::TextView(blank))
                }
            })))
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
                   stale: &Stale,
                   animator: &Stale)
                   -> HResult<PreviewWidget> {
        let (selection, cached_files) = cache.get_files(&file, stale.clone())?;

        let files = cached_files.run_sync()?;

        if stale.is_stale()? { return Previewer::preview_failed(&file) }

        let mut file_list = ListView::new(&core, files);
        if let Some(selection) = selection {
            file_list.select_file(&selection);
        }
        file_list.set_coordinates(&core.coordinates)?;
        file_list.refresh()?;
        if stale.is_stale()? { return Previewer::preview_failed(&file) }
        file_list.animate_slide_up(Some(animator))?;
        Ok(PreviewWidget::FileList(file_list))
    }

    fn preview_text(file: &File,
                    core: &WidgetCore,
                    stale: &Stale,
                    animator: &Stale)
                    -> HResult<PreviewWidget> {
        let lines = core.coordinates.ysize() as usize;
        let mut textview
            = TextView::new_from_file_limit_lines(&core,
                                                  &file,
                                                  lines)?;
        if stale.is_stale()? { return Previewer::preview_failed(&file) }

        textview.set_coordinates(&core.coordinates)?;
        textview.refresh()?;

        if stale.is_stale()? { return Previewer::preview_failed(&file) }

        textview.animate_slide_up(Some(animator))?;
        Ok(PreviewWidget::TextView(textview))
    }

    fn preview_external(file: &File,
                        core: &WidgetCore,
                        stale: &Stale,
                        animator: &Stale)
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

        if stale.is_stale()? { return Previewer::preview_failed(&file) }

        let output = process.wait_with_output()?;

        if stale.is_stale()? { return Previewer::preview_failed(&file) }
        {
            let mut pid_ = SUBPROC.lock()?;
            *pid_ = None;
        }

        //let status = output.status.code()?;

        if stale.is_stale()? {
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
            textview.animate_slide_up(Some(animator)).log();
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

    fn config_loaded(&mut self) -> HResult<()> {
        let show_hidden = self.config().show_hidden();
        if let PreviewWidget::FileList(filelist) = self.widget.widget_mut()? {
            filelist.content.show_hidden = show_hidden;
            filelist.content.dirty_meta.set_dirty();
        }
        Ok(())
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

    fn on_key(&mut self, key: Key) -> HResult<()> {
        self.widget.on_key(key)
    }
}

impl Widget for PreviewWidget {
    fn get_core(&self) -> HResult<&WidgetCore> {
        match self {
            PreviewWidget::FileList(widget) => widget.get_core(),
            PreviewWidget::TextView(widget) => widget.get_core(),
            #[cfg(feature = "img")]
            PreviewWidget::ImgView(widget) => widget.get_core(),
            #[cfg(feature = "video")]
            PreviewWidget::MediaView(widget) => widget.get_core()
        }
    }
    fn get_core_mut(&mut self) -> HResult<&mut WidgetCore> {
        match self {
            PreviewWidget::FileList(widget) => widget.get_core_mut(),
            PreviewWidget::TextView(widget) => widget.get_core_mut(),
            #[cfg(feature = "img")]
            PreviewWidget::ImgView(widget) => widget.get_core_mut(),
            #[cfg(feature = "video")]
            PreviewWidget::MediaView(widget) => widget.get_core_mut()
        }
    }
    fn set_coordinates(&mut self, coordinates: &Coordinates) -> HResult<()> {
        match self {
            PreviewWidget::FileList(widget) => widget.set_coordinates(coordinates),
            PreviewWidget::TextView(widget) => widget.set_coordinates(coordinates),
            #[cfg(feature = "img")]
            PreviewWidget::ImgView(widget) => widget.set_coordinates(coordinates),
            #[cfg(feature = "video")]
            PreviewWidget::MediaView(widget) => widget.set_coordinates(coordinates),
        }
    }
    fn refresh(&mut self) -> HResult<()> {
        match self {
            PreviewWidget::FileList(widget) => widget.refresh(),
            PreviewWidget::TextView(widget) => widget.refresh(),
            #[cfg(feature = "img")]
            PreviewWidget::ImgView(widget) => widget.refresh(),
            #[cfg(feature = "video")]
            PreviewWidget::MediaView(widget) => widget.refresh()
        }
    }
    fn get_drawlist(&self) -> HResult<String> {
        match self {
            PreviewWidget::FileList(widget) => widget.get_drawlist(),
            PreviewWidget::TextView(widget) => widget.get_drawlist(),
            #[cfg(feature = "img")]
            PreviewWidget::ImgView(widget) => widget.get_drawlist(),
            #[cfg(feature = "video")]
            PreviewWidget::MediaView(widget) => widget.get_drawlist()
        }
    }

    fn on_key(&mut self, key: Key) -> HResult<()> {
        match self {
            PreviewWidget::FileList(widget) => widget.on_key(key),
            PreviewWidget::TextView(widget) => widget.on_key(key),
            #[cfg(feature = "img")]
            PreviewWidget::ImgView(widget) => widget.on_key(key),
            #[cfg(feature = "video")]
            PreviewWidget::MediaView(widget) => widget.on_key(key)
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
