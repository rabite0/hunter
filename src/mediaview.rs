use lazy_static;
use termion::event::Key;
use failure::{self, Fail};

use crate::widget::{Widget, WidgetCore};
use crate::coordinates::Coordinates;
use crate::async_value::Stale;
use crate::fail::{HResult, HError, ErrorLog};
use crate::imgview::ImgView;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock,
                mpsc::{channel, Sender}};

use std::io::{BufRead, BufReader, Write};
use std::process::Child;

#[derive(Fail, Debug, Clone)]
pub enum MediaError {
    #[fail(display = "{}", _0)]
    NoPreviewer(String)
}

impl From<MediaError> for HError {
    fn from(e: MediaError) -> HError {
        HError::Media(e)
    }
}

impl std::cmp::PartialEq for MediaView {
    fn eq(&self, other: &Self) -> bool {
        self.core == other.core
    }
}

lazy_static! {
    static ref MUTE: Arc<RwLock<bool>> = Arc::new(RwLock::new(false));
    static ref AUTOPLAY: Arc<RwLock<bool>> = Arc::new(RwLock::new(true));
}

pub struct MediaView {
    core: WidgetCore,
    imgview: Arc<Mutex<ImgView>>,
    file: PathBuf,
    controller: Sender<String>,
    paused: bool,
    media_type: MediaType,
    height: Arc<Mutex<usize>>,
    position: Arc<Mutex<usize>>,
    duration: Arc<Mutex<usize>>,
    stale: Stale,
    process: Arc<Mutex<Option<Child>>>,
    preview_runner: Option<Box<dyn FnOnce(bool,
                                          bool,
                                          Arc<Mutex<usize>>,
                                          Arc<Mutex<usize>>,
                                          Arc<Mutex<usize>>)
                                          -> HResult<()> + Send + 'static>>
}

#[derive(Clone,Debug)]
pub enum MediaType {
    Video,
    Audio
}

impl MediaType {
    pub fn to_str(&self) -> &str {
        match self {
            MediaType::Video => "video",
            MediaType::Audio => "audio"
        }
    }
}

impl MediaView {
    pub fn new_from_file(core: WidgetCore,
                         file: &Path,
                         media_type: MediaType) -> HResult<MediaView> {
        // Check if previewer is present, or bail out to show message
        let media_previewer = core.config().media_previewer;
        if crate::minibuffer::find_bins(&media_previewer).is_err() {
            let msg = format!("Couldn't find previewer: {}{}{}!",
                              crate::term::color_red(),
                              media_previewer,
                              crate::term::normal_color());


            core.show_status(&msg).log();

            return Err(MediaError::NoPreviewer(msg))?;
        }



        let imgview = ImgView {
            core: core.clone(),
            buffer: vec![],
            file: None
        };

        let (tx_cmd, rx_cmd) = channel();

        // Stuff that gets moved into the closure
        let imgview = Arc::new(Mutex::new(imgview));
        let thread_imgview = imgview.clone();

        let path = file.to_string_lossy().to_string();
        let sender = core.get_sender();
        let stale = Stale::new();
        let tstale = stale.clone();
        let rx_cmd = Arc::new(Mutex::new(rx_cmd));
        let process = Arc::new(Mutex::new(None));
        let cprocess = process.clone();
        let ctype = media_type.clone();
        let ccore = core.clone();
        let media_previewer = core.config().media_previewer;
        let g_mode = core.config().graphics;

        let run_preview = Box::new(move | auto,
                                   mute,
                                   height: Arc<Mutex<usize>>,
                                   position: Arc<Mutex<usize>>,
                                   duration: Arc<Mutex<usize>>| -> HResult<()> {
            loop {
                if tstale.is_stale()? {
                    return Ok(());
                }

                // Use current size. Widget could have been resized at some point
                let (xsize, ysize, xpix, ypix) =
                {
                    let view = thread_imgview.lock()?;
                    let (xsize, ysize) = view.core.coordinates.size_u();
                    let (xpix, ypix) = view.core.coordinates.size_pixels()?;
                    (xsize, ysize, xpix, ypix)
                };
                let cell_ratio = crate::term::cell_ratio()?;


                let mut previewer = std::process::Command::new(&media_previewer)
                    .arg(format!("{}", (xsize+1)))
                    // Leave space for position/seek bar
                    .arg(format!("{}", (ysize-1)))
                    .arg(format!("{}", xpix))
                    .arg(format!("{}", ypix))
                    .arg(format!("{}", cell_ratio))
                    .arg(format!("{}", ctype.to_str()))
                    .arg(format!("{}", auto))
                    .arg(format!("{}", mute))
                    .arg(format!("{}", g_mode))
                    .arg(&path)
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::inherit())
                    .spawn()
                    .map_err(|e| {
                        let msg = format!("Couldn't run {}{}{}! Error: {:?}",
                                          crate::term::color_red(),
                                          media_previewer,
                                          crate::term::normal_color(),
                                          &e.kind());

                        ccore.show_status(&msg).log();

                        MediaError::NoPreviewer(msg)
                    })?;

                let mut stdout = BufReader::new(previewer.stdout.take()?);
                let mut stdin = previewer.stdin.take()?;

                cprocess.lock().map(|mut p| *p = Some(previewer))?;

                let mut frame = vec![];
                let newline = String::from("\n");
                let mut line_buf = String::new();
                let rx_cmd = rx_cmd.clone();

                std::thread::spawn(move || -> HResult<()> {
                    for cmd in rx_cmd.lock()?.iter() {
                        write!(stdin, "{}", cmd)?;
                        write!(stdin, "\n")?;
                        stdin.flush()?;
                    }
                    Ok(())
                });

                loop {
                    // Check if preview-gen finished and break out of loop to restart
                    if let Ok(Some(code)) = cprocess.lock()?
                        .as_mut()?
                        .try_wait() {
                        if code.success() {
                            break;
                        } else {
                            let msg = String::from("hunter-media failed!");
                            return Err(failure::format_err!("{}", msg))?;
                        }
                    }


                    stdout.read_line(&mut line_buf)?;


                    // Newline means frame is complete
                    if line_buf == newline {
                        let new_height;

                        line_buf.clear();
                        stdout.read_line(&mut line_buf)?;
                        let h = line_buf.trim().parse::<usize>()?;

                        let mut height = height.lock().unwrap();
                        if *height != h {
                            new_height = true;
                        } else {
                            new_height = false;
                        }
                        *height = h;


                        line_buf.clear();
                        stdout.read_line(&mut line_buf)?;
                        let pos = &line_buf.trim();
                        *position.lock().unwrap() = pos
                            .parse::<usize>()?;


                        line_buf.clear();
                        stdout.read_line(&mut line_buf)?;
                        let dur = &line_buf.trim();
                        *duration.lock().unwrap() = dur
                            .parse::<usize>()?;


                        if let Ok(mut imgview) = thread_imgview.lock() {
                            if new_height {
                                imgview.core.clear()?;
                            }
                            imgview.set_image_data(frame);
                            sender.send(crate::widget::Events::WidgetReady)
                                .map_err(|e| HError::from(e))
                                .log();;
                        }

                        line_buf.clear();
                        frame = vec![];
                        continue;
                    } else {
                        frame.push(line_buf);
                        line_buf = String::new();
                    }
                }
            }
        });


        Ok(MediaView {
            core: core.clone(),
            imgview: imgview,
            file: file.to_path_buf(),
            media_type: media_type,
            controller: tx_cmd,
            paused: false,
            height: Arc::new(Mutex::new(0)),
            position: Arc::new(Mutex::new(0)),
            duration: Arc::new(Mutex::new(0)),
            stale: stale,
            process: process,
            preview_runner: Some(run_preview)
        })
    }

    pub fn start_video(&mut self) -> HResult<()> {
        let runner = self.preview_runner.take();

        if runner.is_some() {
            let stale = self.stale.clone();
            let autoplay = self.autoplay();
            let mute = self.mute();
            let height = self.height.clone();
            let position = self.position.clone();
            let duration = self.duration.clone();
            let clear = self.get_core()?.get_clearlist()?;

            std::thread::spawn(move || -> HResult<()> {
                // Sleep a bit to avoid overloading the system when scrolling
                let sleeptime = std::time::Duration::from_millis(50);
                std::thread::sleep(sleeptime);

                if !stale.is_stale()? {
                    print!("{}", clear);

                    runner.map(|runner| runner(autoplay,
                                               mute,
                                               height,
                                               position,
                                               duration).log());
                }
                Ok(())
            });
        }
        Ok(())
    }

    pub fn play(&self) -> HResult<()> {
        Ok(self.controller.send(String::from("p"))?)
    }

    pub fn pause(&self) -> HResult<()> {
        Ok(self.controller.send(String::from ("a"))?)
    }

    pub fn progress_bar(&self) -> HResult<String> {
        let xsize = self.core.coordinates.xsize_u();

        let position = self.position.lock()?.clone();
        let duration = self.duration.lock()?.clone();

        if duration == 0 || position == 0 {
            Ok(format!("{:elements$}", "|", elements=xsize))
        } else {
            let element_percent = 100 as f32 / xsize as f32;
            let progress_percent = position as f32 / duration as f32 * 100 as f32;
            let element_count = progress_percent as f32 / element_percent as f32;

            Ok(format!("{:|>elements$}|{: >empty$}",
                       "",
                       "",
                       empty=xsize - (element_count as usize),
                       elements=element_count as usize))
        }
    }

    pub fn progress_string(&self) -> HResult<String> {
        let position = self.position.lock()?.clone();
        let duration = self.duration.lock()?.clone();

        let fposition = self.format_secs(position);
        let fduration = self.format_secs(duration);

        Ok(format!("{} / {}", fposition, fduration))
    }

    pub fn get_icons(&self, lines: usize) -> HResult<String> {
        let (xpos, ypos) = self.core.coordinates.position_u();
        let (xsize, _) = self.core.coordinates.size_u();

        let mute_char = "🔇";
        let pause_char = "⏸";
        let play_char = "▶";

        let mut icons = String::new();

        if *MUTE.read()? == true {
            icons += &crate::term::goto_xy_u(xpos+xsize-2, ypos+lines);
            icons += mute_char;
        } else {
            // Clear the mute symbol, or it doesn't go away
            icons += &crate::term::goto_xy_u(xpos+xsize-2, ypos+lines);
            icons += "  ";
        }

        if *AUTOPLAY.read()? == true {
            icons += &crate::term::goto_xy_u(xpos+xsize-4, ypos+lines);
            icons += play_char;
        } else {
            icons += &crate::term::goto_xy_u(xpos+xsize-4, ypos+lines);
            icons += pause_char;
        }

        Ok(icons)
    }

    pub fn format_secs(&self, secs: usize) -> String {
        let hours = if secs >= 60*60 { (secs / 60) / 60 } else { 0 };
        let mins = if secs >= 60 { (secs / 60) %60 } else { 0 };


        format!("{:02}:{:02}:{:02}", hours, mins, secs % 60)
    }

    pub fn toggle_pause(&mut self) -> HResult<()> {
        let auto = AUTOPLAY.read()?.clone();
        let pos = self.position.lock()?.clone();

        // This combination means only first frame was shown, since
        // self.paused will be false, even with autoplay off
        if pos == 0 && auto == false && self.paused == false {
            self.toggle_autoplay();

            self.start_video()?;
            self.paused = false;
            self.play()?;

            return Ok(())
        }
        if self.paused {
            self.toggle_autoplay();
            self.play()?;
            self.paused = false;
        } else {
            self.pause()?;
            self.toggle_autoplay();
            self.paused = true;
        }
        Ok(())
    }

    pub fn quit(&self) -> HResult<()> {
        Ok(self.controller.send(String::from("q"))?)
    }

    pub fn seek_forward(&self) -> HResult<()> {
        Ok(self.controller.send(String::from(">"))?)
    }

    pub fn seek_backward(&self) -> HResult<()> {
        Ok(self.controller.send(String::from("<"))?)
    }

    pub fn autoplay(&self) -> bool {
        if let Ok(autoplay) = AUTOPLAY.read() {
            return *autoplay;
        }
        return true;
    }

    pub fn mute(&self) -> bool {
        if let Ok(mute) = MUTE.read() {
            return *mute;
        }
        return false;
    }

    pub fn toggle_autoplay(&self) {
        if let Ok(mut autoplay) = AUTOPLAY.write() {
            *autoplay = !*autoplay;
        }
    }

    pub fn toggle_mute(&self) {
        if let Ok(mut mute) = MUTE.write() {
            *mute = !*mute;
            if *mute {
                self.controller.send(String::from("m")).ok();
            } else {
                self.controller.send(String::from("u")).ok();
            }
        }
    }

    pub fn kill(&mut self) -> HResult<()> {
        let proc = self.process.clone();
        std::thread::spawn(move || -> HResult<()> {
            proc.lock()?
                .as_mut()
                .map(|p| {
                    p.kill().map_err(|e| HError::from(e)).log();
                    p.wait().map_err(|e| HError::from(e)).log();
                });
            Ok(())
        });
        Ok(())
    }
}

impl Widget for MediaView {
    fn get_core(&self) -> HResult<&WidgetCore> {
        Ok(&self.core)
    }

    fn get_core_mut(&mut self) -> HResult<&mut WidgetCore> {
        Ok(&mut self.core)
    }

    fn set_coordinates(&mut self, coordinates: &Coordinates) -> HResult<()> {
        if &self.core.coordinates == coordinates { return Ok(()); }


        self.core.coordinates = coordinates.clone();

        let mut imgview = self.imgview.lock()?;
        imgview.set_image_data(vec![]);
        imgview.set_coordinates(&coordinates)?;

        let (xsize, ysize) = self.core.coordinates.size_u();
        let (xpix, ypix) = self.core.coordinates.size_pixels()?;
        let cell_ratio = crate::term::cell_ratio()?;

        let xystring = format!("xy\n{}\n{}\n{}\n{}\n{}\n",
                               xsize+1,
                               ysize-1,
                               xpix,
                               ypix,
                               cell_ratio);

        self.controller.send(xystring)?;

        Ok(())
    }

    fn refresh(&mut self) -> HResult<()> {
        self.start_video().log();
        Ok(())
    }

    fn get_drawlist(&self) -> HResult<String> {
        let (xpos, ypos) = self.core.coordinates.position_u();
        let height = *self.height.lock()?;
        let progress_str = self.progress_string()?;
        let progress_bar = self.progress_bar()?;

        let frame= self.imgview
            .lock()
            .map(|img| img.get_drawlist())?;

        let mut frame = frame?;

        frame += &crate::term::goto_xy_u(xpos, ypos+height);
        frame += &progress_str;
        frame += &self.get_icons(height)?;
        frame += &crate::term::goto_xy_u(xpos, ypos+height+1);
        frame += &progress_bar;

        Ok(frame)
    }

    fn on_key(&mut self, key: Key) -> HResult<()> {
        match key {
            Key::Alt('>') => self.seek_forward(),
            Key::Alt('<') => self.seek_backward(),
            Key::Alt('m') => self.toggle_pause(),
            Key::Alt('M') => Ok(self.toggle_mute()),
            _ => HError::undefined_key(key)
        }
    }
}

impl Drop for MediaView {
    fn drop(&mut self) {
        self.stale.set_stale().ok();
        self.kill().log();

        self.core.clear().log();
    }
}
