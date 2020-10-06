use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::io::{BufReader, BufRead};
use std::sync::atomic::{AtomicU32, Ordering};

use crate::widget::{Widget, WidgetCore};
use crate::coordinates::Coordinates;
use crate::fail::{HResult, ErrorCause, HError};
use crate::mediaview::MediaError;



lazy_static! {
    static ref PID: AtomicU32 = AtomicU32::new(0);
}

#[derive(Derivative)]
#[derivative(PartialEq)]
pub struct ImgView {
    pub core: WidgetCore,
    pub buffer: Vec<String>,
    pub file: Option<PathBuf>,
}

impl ImgView {
    pub fn new_from_file(core: WidgetCore, file: &Path) -> HResult<ImgView> {
        let mut view = ImgView {
            core: core,
            buffer: vec![],
            file: Some(file.to_path_buf()),
        };

        view.encode_file()?;
        Ok(view)
    }

    pub fn encode_file(&mut self) -> HResult<()> {
        let (xsize, ysize) = self.core.coordinates.size_u();
        let (xpix, ypix) = self.core.coordinates.size_pixels()?;
        let cell_ratio = crate::term::cell_ratio()?;

        let file = &self.file.as_ref().ok_or_else(|| HError::NoneError)?;
        let media_previewer = self.core.config().media_previewer;
        let g_mode = self.core.config().graphics;

        let mut previewer = Command::new(&media_previewer)
            .arg(format!("{}", (xsize+1)))
            .arg(format!("{}", (ysize+1)))
            .arg(format!("{}", xpix))
            .arg(format!("{}", ypix))
            .arg(format!("{}", cell_ratio))
            .arg("image")
            .arg(format!("true"))
            .arg(format!("true"))
            .arg(format!("{}", g_mode))
            .arg(file.to_string_lossy().to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                let msg = format!("Couldn't run {}{}{}! Error: {:?}",
                                  crate::term::color_red(),
                                  media_previewer,
                                  crate::term::normal_color(),
                                  &e.kind());

                self.core.show_status(&msg).ok();

                MediaError::NoPreviewer(msg)
            })?;

        PID.store(previewer.id(), Ordering::Relaxed);

        let stdout = previewer.stdout
                              .take()
                              .unwrap();

        let output = BufReader::new(stdout)
            .lines()
            .collect::<Result<Vec<String>, _>>()?;

        let stderr = previewer.stderr
                              .take()
                              .unwrap();

        let stderr = BufReader::new(stderr)
            .lines()
            .collect::<Result<String, _>>()?;

        let status = previewer.wait()?;

        PID.store(0, Ordering::Relaxed);

        if !status.success() {
            match status.code() {
                Some(code) => Err(MediaError::MediaViewerFailed(code,
                                                                ErrorCause::Str(stderr)))?,
                None => Err(MediaError::MediaViewerKilled)?
            }
        }

        self.buffer = output;

        Ok(())
    }

    pub fn set_image_data(&mut self, img_data: Vec<String>) {
        self.buffer = img_data;
    }

    pub fn lines(&self) -> usize {
        self.buffer.len()
    }

    pub fn kill_running() {
        use nix::{unistd::Pid,
                  sys::signal::{kill, Signal}};

        let pid = PID.load(Ordering::Relaxed);

        if pid == 0 { return; }

        let pid = Pid::from_raw(pid as i32);
        kill(pid, Signal::SIGTERM).ok();

        PID.store(0, Ordering::Relaxed);
    }
}


impl Widget for ImgView {
    fn get_core(&self) -> HResult<&WidgetCore> {
        Ok(&self.core)
    }

    fn get_core_mut(&mut self) -> HResult<&mut WidgetCore> {
        Ok(&mut self.core)
    }

    fn set_coordinates(&mut self, coordinates: &Coordinates) -> HResult<()> {
        if &self.core.coordinates == coordinates { return Ok(()) }

        self.core.coordinates = coordinates.clone();
        if self.file.is_some() {
            self.encode_file()?;
        }

        Ok(())
    }

    fn refresh(&mut self) -> HResult<()> {

        Ok(())
    }

    fn get_drawlist(&self) -> HResult<String> {
        let (xpos, ypos) = self.core.coordinates.position_u();

        let mut draw = self.buffer
            .iter()
            .enumerate()
            .fold(String::new(), |mut draw, (pos, line)| {
                draw += &format!("{}", crate::term::goto_xy_u(xpos,
                                                              ypos + pos));
                draw += line;
                draw
            });

        draw += &format!("{}", termion::style::Reset);

        Ok(draw)
    }
}

impl Drop for ImgView {
    fn drop(&mut self) {
        let g_mode = self.core.config().graphics;
        if g_mode == "kitty" || g_mode == "auto" {
            print!("\x1b_Ga=d\x1b\\");
        }
    }
}
