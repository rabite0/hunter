use std::sync::{Arc, Mutex};
use std::process::Child;
use std::process::Stdio;
use std::os::unix::io::FromRawFd;
use std::io::{BufRead, BufReader};

use termion::event::Key;
use unicode_width::UnicodeWidthStr;

use crate::coordinates::{Coordinates, Size, Position};
use crate::listview::{Listable, ListView};
use crate::textview::TextView;
use crate::widget::Widget;
use crate::window::{send_event, Events};
use crate::fail::{HResult, HError};
use crate::term;

#[derive(Debug)]
struct Process {
    cmd: String,
    handle: Arc<Mutex<Child>>,
    output: Arc<Mutex<String>>,
    status: Arc<Mutex<Option<i32>>>,
    success: Arc<Mutex<Option<bool>>>
}

impl Process {
    fn read_proc(&mut self) -> HResult<()> {
        let handle = self.handle.clone();
        let output = self.output.clone();
        let status = self.status.clone();
        let success = self.success.clone();

        std::thread::spawn(move || {
            let stdout = handle.lock().unwrap().stdout.take().unwrap();
            let mut stdout = BufReader::new(stdout);
            loop {
                let mut line = String::new();
                match stdout.read_line(&mut line) {
                    Ok(0) => break,
                    Ok(_) => {
                        output.lock().unwrap().push_str(&line);
                        send_event(Events::WidgetReady).unwrap();
                    }
                    Err(err) => {
                        dbg!(err);
                        break;
                    }
                }
            }
            if let Ok(proc_status) = handle.lock().unwrap().wait() {
                *success.lock().unwrap() = Some(proc_status.success());
                *status.lock().unwrap() = proc_status.code();
            }
        });

        Ok(())
    }
}

impl Listable for ListView<Vec<Process>> {
    fn len(&self) -> usize { self.content.len() }
    fn render(&self) -> Vec<String> {
        self.content.iter().map(|proc| {
            self.render_proc(proc)
        }).collect()
    }
}

impl ListView<Vec<Process>> {
    fn run_proc(&mut self, cmd: &str) -> HResult<()> {
        let handle = std::process::Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(unsafe { Stdio::from_raw_fd(2) })
            .spawn()?;
        let mut proc = Process {
            cmd: cmd.to_string(),
            handle: Arc::new(Mutex::new(handle)),
            output: Arc::new(Mutex::new(String::new())),
            status: Arc::new(Mutex::new(None)),
            success: Arc::new(Mutex::new(None))
        };
        proc.read_proc()?;
        self.content.push(proc);
        Ok(())
    }

    fn kill_proc(&mut self) -> HResult<()> {
        let proc = self.selected_proc()?;
        proc.handle.lock()?.kill()?;
        Ok(())
    }

    fn remove_proc(&mut self) -> HResult<()> {
        self.kill_proc().ok();
        let selection = self.get_selection();
        self.content.remove(selection);
        Ok(())
    }

    fn selected_proc(&mut self) -> Option<&mut Process> {
        let selection = self.get_selection();
        self.content.get_mut(selection)
    }

    pub fn render_proc(&self, proc: &Process) -> String {
        let status = match *proc.status.lock().unwrap() {
            Some(status) => format!("{}", status),
            None => "<R>".to_string()
        };

        let xsize = self.get_coordinates().xsize();
        let sized_string = term::sized_string(&proc.cmd, xsize);
        let status_pos = xsize - status.len() as u16;
        let padding = sized_string.len() - sized_string.width_cjk();
        let padding = xsize - padding as u16;

        let color_status = match *proc.success.lock().unwrap() {
            Some(false) => { format!("{}{}", term::color_red(), status) }
            _ => { status }
        };

        format!(
            "{}{}{}{}{}{}",
            termion::cursor::Save,
            format!("{}{}{:padding$}{}",
                    term::normal_color(),
                    &sized_string,
                    " ",
                    term::normal_color(),
                    padding = padding as usize),
            termion::cursor::Restore,
            termion::cursor::Right(status_pos),
            term::highlight_color(),
            color_status
        )
    }
}

pub struct ProcView {
    coordinates: Coordinates,
    proc_list: ListView<Vec<Process>>,
    textview: TextView,
}

impl ProcView {
    pub fn new() -> ProcView {
        ProcView {
            coordinates: Coordinates::new(),
            proc_list: ListView::new(vec![]),
            textview: TextView::new_blank(),
        }
    }

    pub fn run_proc(&mut self, cmd: &str) -> HResult<()> {
        self.proc_list.run_proc(cmd)?;
        Ok(())
    }

    pub fn remove_proc(&mut self) -> HResult<()> {
        self.proc_list.remove_proc()?;
        self.textview.set_text("");
        Ok(())
    }

    fn show_output(&mut self) -> HResult<()> {
        let output = self.proc_list.selected_proc()?.output.lock()?;
        self.textview.set_text(&*output);
        Ok(())
    }

    pub fn calculate_coordinates(&self) -> (Coordinates, Coordinates) {
        let xsize = self.coordinates.xsize();
        let ysize = self.coordinates.ysize();
        let top = self.coordinates.top().y();
        let ratio = (33, 66);

        let left_xsize = xsize * ratio.0 / 100;
        let left_size = Size((left_xsize, ysize));
        let left_pos = self.coordinates.top();

        let main_xsize = xsize * ratio.1 / 100;
        let main_size = Size((main_xsize, ysize));
        let main_pos = Position((left_xsize + 2, top));



        let left_coords = Coordinates {
            size: left_size,
            position: left_pos,
        };

        let main_coords = Coordinates {
            size: main_size,
            position: main_pos,
        };

        (left_coords, main_coords)
    }


}

impl Widget for ProcView {
    fn get_coordinates(&self) -> &Coordinates {
        &self.coordinates
    }
    fn set_coordinates(&mut self, coordinates: &Coordinates) {
        self.coordinates = coordinates.clone();

        let (lcoord, rcoord) = self.calculate_coordinates();
        self.proc_list.set_coordinates(&lcoord);
        self.textview.set_coordinates(&rcoord);

        self.refresh();
    }
    fn render_header(&self) -> String {
        "".to_string()
    }
    fn refresh(&mut self) {
        self.show_output().ok();
        self.proc_list.refresh();
        self.textview.refresh();
    }
    fn get_drawlist(&self) -> String {
        self.proc_list.get_drawlist() + &self.textview.get_drawlist()
    }
    fn on_key(&mut self, key: Key) -> HResult<()> {
        match key {
            Key::Char('w') => { return Err(HError::PopupFinnished) }
            Key::Char('d') => { self.remove_proc()? }
            Key::Char('k') => { self.proc_list.kill_proc()? }
            Key::Up | Key::Char('p') => {
                self.proc_list.move_up();
                self.proc_list.refresh();
            }
            Key::Down | Key::Char('n') => {
                self.proc_list.move_down();
                self.proc_list.refresh();
            }
            _ => {}
        }
        self.refresh();
        self.draw()?;
        Ok(())
    }
}
