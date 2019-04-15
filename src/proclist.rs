use std::sync::{Arc, Mutex};
use std::sync::mpsc::Sender;
use std::process::Child;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::io::{BufRead, BufReader};
use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;

use termion::event::Key;
use unicode_width::UnicodeWidthStr;
use osstrtools::OsStrTools;

use crate::listview::{Listable, ListView};
use crate::textview::TextView;
use crate::widget::{Widget, Events, WidgetCore};
use crate::coordinates::Coordinates;
use crate::preview::{AsyncWidget, Stale};
use crate::dirty::Dirtyable;
use crate::hbox::HBox;
use crate::fail::{HResult, HError, ErrorLog};
use crate::term;
use crate::files::File;

#[derive(Debug)]
struct Process {
    cmd: String,
    handle: Arc<Mutex<Child>>,
    output: Arc<Mutex<String>>,
    status: Arc<Mutex<Option<i32>>>,
    success: Arc<Mutex<Option<bool>>>,
    sender: Sender<Events>

}

pub struct Cmd {
    pub cmd: OsString,
    pub args: Option<Vec<OsString>>,
    pub short_cmd: Option<String>,
    pub cwd: File,
    pub cwd_files: Option<Vec<File>>,
    pub tab_files: Option<Vec<Vec<File>>>,
    pub tab_paths: Option<Vec<File>>,
}

impl Cmd {
    fn process(&mut self) -> Vec<OsString> {
        let cmd = self.cmd.clone().split(&OsString::from(" "));
        let cmd = self.substitute_cwd_files(cmd);
        let cmd = self.substitute_tab_files(cmd);
        let cmd = self.substitute_tab_paths(cmd);
        cmd
    }

    fn substitute_cwd_files(&mut self, cmd: Vec<OsString>) -> Vec<OsString> {
        if self.cwd_files.is_none() { return cmd; }

        let cwd_pat = OsString::from("$s");
        let cwd_files =  self.cwd_files
            .take()
            .unwrap()
            .iter()
            .map(|file| file.strip_prefix(&self.cwd).into_os_string())
            .collect::<Vec<OsString>>();

        cmd.iter()
            .map(|part| part.splice_quoted(&cwd_pat,
                                         cwd_files.clone()))
            .flatten().collect()
    }

    fn substitute_tab_files(&mut self, cmd: Vec<OsString>) -> Vec<OsString> {
        if self.tab_files.is_none() { return cmd; }

        let tab_files = self.tab_files.take().unwrap();

        tab_files.into_iter()
            .enumerate()
            .fold(cmd, |cmd, (i, tab_files)| {
                let tab_files_pat = OsString::from(format!("${}s", i));
                let tab_file_paths = tab_files.iter()
                    .map(|file| file.strip_prefix(&self.cwd).into_os_string())
                    .collect::<Vec<OsString>>();

                cmd.iter().map(|part| {
                    part.splice_quoted(&tab_files_pat,
                                       tab_file_paths.clone())
                }).flatten().collect()
            })
    }

    fn substitute_tab_paths(&mut self, cmd: Vec<OsString>) -> Vec<OsString> {
        if self.tab_paths.is_none() { return cmd; }

        let tab_paths = self.tab_paths.take().unwrap();

        tab_paths.into_iter()
            .enumerate()
            .fold(cmd, |cmd, (i, tab_path)| {
                let tab_path_pat = OsString::from(format!("${}", i));
                let tab_path = tab_path.strip_prefix(&self.cwd).into_os_string();

                cmd.iter().map(|part| {
                    part.splice_quoted(&tab_path_pat,
                                     vec![tab_path.clone()])
                }).flatten().collect()
            })
    }
}

impl PartialEq for Process {
    fn eq(&self, other: &Process) -> bool {
        self.cmd == other.cmd
    }
}

impl Process {
    fn read_proc(&mut self) -> HResult<()> {
        let handle = self.handle.clone();
        let output = self.output.clone();
        let status = self.status.clone();
        let success = self.success.clone();
        let sender = self.sender.clone();
        let cmd = self.cmd.clone();
        let pid = self.handle.lock()?.id();

        std::thread::spawn(move || -> HResult<()> {
            let stdout = handle.lock()?.stdout.take()?;
            let mut stdout = BufReader::new(stdout);
            let mut processor = move |cmd, sender: &Sender<Events>| -> HResult<()> {
                loop {
                    let buffer = stdout.fill_buf()?;
                    let len = buffer.len();
                    let buffer = String::from_utf8_lossy(buffer);

                    if len == 0 { return Ok(()) }

                    output.lock()?.push_str(&buffer);

                    let status = format!("{}: read {} chars!", cmd, len);
                    sender.send(Events::Status(status))?;

                    stdout.consume(len);
                }
            };
            processor(&cmd, &sender).log();

            if let Ok(proc_status) = handle.lock()?.wait() {
                let proc_success = proc_status.success();
                let proc_status = match proc_status.code() {
                    Some(status) => status,
                    None => proc_status.signal().unwrap_or(-1)
                };

                *success.lock()? = Some(proc_success);
                *status.lock()? = Some(proc_status);

                let color_success =
                    if proc_success {
                        format!("{}successfully", term::color_green())
                    } else {
                        format!("{}unsuccessfully", term::color_red())
                    };

                let color_status =
                    if proc_success {
                        format!("{}{}", term::color_green(), proc_status)
                    } else {
                        format!("{}{}", term::color_red(), proc_status)
                    };

                let status = format!("Process: {}:{} exited {}{} with status: {}",
                                     cmd,
                                     pid,
                                     color_success,
                                     term::normal_color(),
                                     color_status);
                sender.send(Events::Status(status))?;
            }
            Ok(())
        });

        Ok(())
    }
}

impl Listable for ListView<Vec<Process>> {
    fn len(&self) -> usize { self.content.len() }
    fn render(&self) -> Vec<String> {
        self.content.iter().map(|proc| {
            self.render_proc(proc).unwrap()
        }).collect()
    }
    fn on_refresh(&mut self) -> HResult<()> {
        self.core.set_dirty();
        Ok(())
    }
}

impl ListView<Vec<Process>> {
    fn run_proc_subshell(&mut self, mut cmd: Cmd) -> HResult<()> {
        let shell = std::env::var("SHELL").unwrap_or("sh".into());
        let home = crate::paths::home_path()?.into_os_string();

        let cmd_args = cmd.process();

        let short = OsString::from("~");
        let short_cmd = cmd_args
            .concat()
            .replace(&home, &short)
            .replace(&OsString::from("\""), &OsString::from(""))
            .to_string_lossy()
            .to_string();

        self.show_status(&format!("Running: {}", &short_cmd)).log();

        let shell_args = cmd_args.concat();
        let shell_args = vec![OsString::from("-c"), shell_args.clone()];

        cmd.cmd = OsString::from(shell.clone());
        cmd.args = Some(shell_args.clone());
        cmd.short_cmd = Some(short_cmd);

        self.run_proc_raw(cmd)
    }

    fn run_proc_raw(&mut self, cmd: Cmd) -> HResult<()> {
        let real_cmd = cmd.cmd;
        let short_cmd = cmd.short_cmd
            .unwrap_or(real_cmd
                       .to_string_lossy()
                       .to_string());
        let args = cmd.args.unwrap_or(vec![]);

        self.show_status(&format!("Running: {}", &short_cmd)).log();

        let handle = std::process::Command::new(real_cmd)
            .args(args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .before_exec(|| unsafe { libc::dup2(1, 2); Ok(()) })
            .spawn()?;
        let mut proc = Process {
            cmd: short_cmd,
            handle: Arc::new(Mutex::new(handle)),
            output: Arc::new(Mutex::new(String::new())),
            status: Arc::new(Mutex::new(None)),
            success: Arc::new(Mutex::new(None)),
            sender: self.get_core()?.get_sender()
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

    pub fn render_proc(&self, proc: &Process) -> HResult<String> {
        let pid = proc.handle.lock()?.id();
        let status = match *proc.status.lock()? {
            Some(status) => format!("{}", status),
            None => format!("<{}>", pid),
        };

        let xsize = self.get_coordinates()?.xsize();
        let sized_string = term::sized_string(&proc.cmd, xsize);
        let status_pos = xsize - status.len() as u16;
        let padding = sized_string.len() - sized_string.width_cjk();
        let padding = xsize - padding as u16;

        let color_status = match *proc.success.lock().unwrap() {
            Some(false) => { format!("{}{}", term::color_red(), status) }
            _ => { status }
        };

        Ok(format!(
            "{}{}{}{}{}{}",
            termion::cursor::Save,
            format!("{}{:padding$}{}",
                    term::normal_color(),
                    &sized_string,
                    term::normal_color(),
                    padding = padding as usize),
            termion::cursor::Restore,
            termion::cursor::Right(status_pos),
            term::highlight_color(),
            color_status))
    }
}

#[derive(PartialEq)]
enum ProcViewWidgets {
    List(ListView<Vec<Process>>),
    TextView(AsyncWidget<TextView>),
}

impl Widget for ProcViewWidgets {
    fn get_core(&self) -> HResult<&WidgetCore> {
        match self {
            ProcViewWidgets::List(widget) => widget.get_core(),
            ProcViewWidgets::TextView(widget) => widget.get_core()
        }
    }
    fn get_core_mut(&mut self) -> HResult<&mut WidgetCore> {
        match self {
            ProcViewWidgets::List(widget) => widget.get_core_mut(),
            ProcViewWidgets::TextView(widget) => widget.get_core_mut()
        }
    }
    fn refresh(&mut self) -> HResult<()> {
        match self {
            ProcViewWidgets::List(widget) => widget.refresh(),
            ProcViewWidgets::TextView(widget) => widget.refresh()
        }
    }
    fn get_drawlist(&self) -> HResult<String> {
        match self {
            ProcViewWidgets::List(widget) => widget.get_drawlist(),
            ProcViewWidgets::TextView(widget) => widget.get_drawlist()
        }
    }
}

pub struct ProcView {
    core: WidgetCore,
    hbox: HBox<ProcViewWidgets>,
    viewing: Option<usize>,
    animator: Stale
}

impl HBox<ProcViewWidgets> {
    fn get_listview(&self) -> &ListView<Vec<Process>> {
        match &self.widgets[0] {
            ProcViewWidgets::List(listview) => listview,
            _ => unreachable!()
        }
    }
    fn get_listview_mut(&mut self) -> &mut ListView<Vec<Process>> {
        match &mut self.widgets[0] {
            ProcViewWidgets::List(listview) => listview,
            _ => unreachable!()
        }
    }
    fn get_textview(&mut self) -> &mut AsyncWidget<TextView> {
        match &mut self.widgets[1] {
            ProcViewWidgets::TextView(textview) => textview,
            _ => unreachable!()
        }
    }
}

impl ProcView {
    pub fn new(core: &WidgetCore) -> ProcView {
        let tcore = core.clone();
        let listview = ListView::new(&core, vec![]);
        let textview = AsyncWidget::new(&core, Box::new(move |_| {
            let textview = TextView::new_blank(&tcore);
            Ok(textview)
        }));
        let mut hbox = HBox::new(&core);
        hbox.push_widget(ProcViewWidgets::List(listview));
        hbox.push_widget(ProcViewWidgets::TextView(textview));
        hbox.set_ratios(vec![33, 66]);
        hbox.refresh().log();
        ProcView {
            core: core.clone(),
            hbox: hbox,
            viewing: None,
            animator: Stale::new()
        }
    }

    fn get_listview(& self) -> & ListView<Vec<Process>> {
        self.hbox.get_listview()
    }

    fn get_listview_mut(&mut self) -> &mut ListView<Vec<Process>> {
        self.hbox.get_listview_mut()
    }

    fn get_textview(&mut self) -> &mut AsyncWidget<TextView> {
        self.hbox.get_textview()
    }

    pub fn run_proc_subshell(&mut self, cmd: Cmd) -> HResult<()> {
        self.get_listview_mut().run_proc_subshell(cmd)?;
        Ok(())
    }

    pub fn run_proc_raw(&mut self, cmd: Cmd) -> HResult<()> {
        self.get_listview_mut().run_proc_raw(cmd)?;
        Ok(())
    }

    pub fn remove_proc(&mut self) -> HResult<()> {
        if self.get_listview_mut().content.len() == 0 { return Ok(()) }
        self.get_listview_mut().remove_proc()?;
        self.get_textview().clear().log();
        self.get_textview().widget_mut()?.set_text("").log();
        self.viewing = None;
        Ok(())
    }

    fn show_output(&mut self) -> HResult<()> {
        if Some(self.get_listview_mut().get_selection()) == self.viewing {
            return Ok(());
        }
        let output = self.get_listview_mut().selected_proc()?.output.lock()?.clone();

        let animator = self.animator.clone();
        animator.set_fresh().log();

        self.get_textview().change_to(Box::new(move |_, core| {
            let mut textview = TextView::new_blank(&core);
            textview.set_text(&output).log();
            textview.animate_slide_up(Some(animator)).log();
            Ok(textview)
        })).log();

        self.viewing = Some(self.get_listview_mut().get_selection());
        Ok(())
    }

    pub fn toggle_follow(&mut self) -> HResult<()> {
        self.get_textview().widget_mut()?.toggle_follow();
        Ok(())
    }

    pub fn scroll_up(&mut self) -> HResult<()> {
        self.get_textview().widget_mut()?.scroll_up();
        Ok(())
    }

    pub fn scroll_down(&mut self) -> HResult<()> {
        self.get_textview().widget_mut()?.scroll_down();
        Ok(())
    }

    pub fn page_up(&mut self) -> HResult<()> {
        self.get_textview().widget_mut()?.page_up();
        Ok(())
    }

    pub fn page_down(&mut self) -> HResult<()> {
        self.get_textview().widget_mut()?.page_down();
        Ok(())
    }

    pub fn scroll_top(&mut self) -> HResult<()> {
        self.get_textview().widget_mut()?.scroll_top();
        Ok(())
    }

    pub fn scroll_bottom(&mut self) -> HResult<()> {
        self.get_textview().widget_mut()?.scroll_bottom();
        Ok(())
    }
}

impl Widget for ProcView {
    fn get_core(&self) -> HResult<&WidgetCore> {
        Ok(&self.core)
    }
    fn get_core_mut(&mut self) -> HResult<&mut WidgetCore> {
        Ok(&mut self.core)
    }
    fn set_coordinates(&mut self, coordinates: &Coordinates) -> HResult<()> {
        self.core.coordinates = coordinates.clone();
        self.hbox.core.coordinates = coordinates.clone();
        self.hbox.set_coordinates(&coordinates)
    }

    fn render_header(&self) -> HResult<String> {
        let listview = self.get_listview();
        let procs_num = listview.len();
        let procs_running = listview
            .content
            .iter()
            .filter(|proc| proc.status.lock().unwrap().is_none())
            .count();

        let header = format!("Running processes: {} / {}",
                             procs_running,
                             procs_num);
        Ok(header)
    }

    fn render_footer(&self) -> HResult<String> {
        let listview = self.get_listview();
        let selection = listview.get_selection();
        let xsize = self.core.coordinates.xsize_u();

        if let Some(proc) = listview.content.get(selection) {
            let cmd = &proc.cmd;
            let pid = proc.handle.lock()?.id();
            let proc_status = proc.status.lock()?;
            let proc_success = proc.success.lock()?;

            let procinfo = if proc_status.is_some() {
                let color_success =
                    if let Some(_) = *proc_success {
                        format!("{}successfully", term::color_green())
                    } else {
                        format!("{}unsuccessfully", term::color_red())
                    };

                let color_status =
                    if let Some(success) = *proc_success {
                        if success {
                            format!("{}{}", term::color_green(), proc_status.unwrap())
                        } else {
                            format!("{}{}", term::color_red(), proc_status.unwrap())
                        }
                    } else { "wtf".to_string() };

                let procinfo = format!("{}:{} exited {}{}{} with status: {}",
                                     cmd,
                                     pid,
                                     color_success,
                                     term::reset(),
                                     term::status_bg(),
                                     color_status);
                procinfo
            } else { "still running".to_string() };

            let footer = term::sized_string_u(&procinfo, xsize);

            Ok(footer)
        } else { Ok("No proccesses".to_string()) }
    }

    fn refresh(&mut self) -> HResult<()> {
        self.hbox.refresh().log();

        self.show_output().log();
        self.get_listview_mut().refresh().log();
        self.get_textview().refresh().log();

        Ok(())
    }
    fn get_drawlist(&self) -> HResult<String> {
        self.hbox.get_drawlist()
    }
    fn on_key(&mut self, key: Key) -> HResult<()> {
        match key {
            Key::Char('w') => {
                self.animator.set_stale().log();
                self.clear().log();
                return Err(HError::PopupFinnished) }
            Key::Char('d') => { self.remove_proc()? }
            Key::Char('K') => { self.get_listview_mut().kill_proc()? }
            Key::Up | Key::Char('k') => {
                self.get_listview_mut().move_up();
            }
            Key::Down | Key::Char('j') => {
                self.get_listview_mut().move_down();
            }
            Key::Char('f') => { self.toggle_follow().log(); }
            Key::Ctrl('j') => { self.scroll_down().log(); },
            Key::Ctrl('k') => { self.scroll_up().log(); },
            Key::Ctrl('v') => { self.page_down().log(); },
            Key::Alt('v') => { self.page_up().log(); },
            Key::Char('>') => { self.scroll_bottom().log(); },
            Key::Char('<') => { self.scroll_top().log(); }
            _ => {}
        }
        self.refresh().log();
        self.draw().log();
        Ok(())
    }
}


trait ConcatOsString {
    fn concat(&self) -> OsString;
    fn concat_quoted(&self) -> OsString;
}

impl ConcatOsString for Vec<OsString> {
    fn concat(&self) -> OsString {
        let len = self.len();
        self.iter().enumerate().fold(OsString::new(), |string, (i, part)| {
            let mut string = string.into_vec();
            let mut space = " ".as_bytes().to_vec();
            let mut part = part.clone().into_vec();

            string.append(&mut part);

            if i != len {
                string.append(&mut space);
            }

            OsString::from_vec(string)
        })
    }

    fn concat_quoted(&self) -> OsString {
        let len = self.len();
        self.iter().enumerate().fold(OsString::new(), |string, (i, part)| {
            let mut string = string.into_vec();
            let mut space = " ".as_bytes().to_vec();
            let mut quote = "\"".as_bytes().to_vec();
            let mut part = part.clone().into_vec();


            string.append(&mut quote.clone());
            string.append(&mut part);
            string.append(&mut quote);


            if i+1 != len {
                string.append(&mut space);
            }

            OsString::from_vec(string)
        })
    }
}
