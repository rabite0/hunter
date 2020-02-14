use std::sync::{Arc, Mutex};
use std::sync::mpsc::Sender;
use std::process::{Child, Command};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::io::{BufRead, BufReader};
use std::ffi::OsString;
use std::os::unix::ffi::OsStrExt;

use termion::event::Key;
use unicode_width::UnicodeWidthStr;
use osstrtools::{OsStringTools, OsStrTools, OsStrConcat};
use async_value::Stale;

use crate::listview::{Listable, ListView};
use crate::textview::TextView;
use crate::widget::{Widget, Events, WidgetCore};
use crate::coordinates::Coordinates;
use crate::preview::AsyncWidget;
use crate::dirty::Dirtyable;
use crate::hbox::HBox;
use crate::fail::{HResult, HError, ErrorLog};
use crate::term::{self, ScreenExt};
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
    pub vars: Option<Vec<(OsString, OsString)>>,
    pub short_cmd: Option<String>,
    pub cwd: File,
    pub cwd_files: Option<Vec<File>>,
    pub tab_files: Option<Vec<Vec<File>>>,
    pub tab_paths: Option<Vec<File>>,
}

impl Cmd {
    fn process(&mut self) -> Vec<OsString> {
        // Split the string now, so inserted files aren't screwed up by substitutions
        let cmd = self.cmd.split(" ")
            .into_iter()
            .map(|s| s.to_os_string())
            .collect();

        let cmd = self.substitute_cwd_files(cmd);
        let cmd = self.substitute_tab_files(cmd);
        let cmd = self.substitute_tab_paths(cmd);

        cmd
    }

    fn perform_substitution(&self,
                            cmd: Vec<OsString>,
                            pat: &str,
                            files: Vec<File>) -> Vec<OsString> {
        if !self.cmd.contains(pat) { return cmd; }

        let files =  files
            .into_iter()
            .map(|file|
                 // strip out the cwd part to make path shorter
                 file.strip_prefix(&self.cwd)
                 .into_os_string()
                 // escape single quotes so file names with them work
                 .escape_single_quote())
            .collect::<Vec<OsString>>();

        cmd.into_iter()
            .map(|part| {
                // If this part isn't the pattern, just return it as is
                match part != pat {
                    true => part,
                    false => part.splice(pat,
                                         &files)
                        .assemble_with_sep_and_wrap(" ", "'")
                }
            })
            .collect()
    }

    fn substitute_cwd_files(&mut self, cmd: Vec<OsString>) -> Vec<OsString> {
        if self.cwd_files.is_none() { return cmd; }
        let files = self.cwd_files.take().unwrap();
        self.perform_substitution(cmd, "$s", files)
    }

    fn substitute_tab_files(&mut self, cmd: Vec<OsString>) -> Vec<OsString> {
        if self.tab_files.is_none() { return cmd; }
        let tab_files = self.tab_files.take().unwrap();

        tab_files.into_iter()
            .enumerate()
            .fold(cmd, |cmd, (i, tab_files)| {
                let tab_files_pat = String::from(format!("${}s", i));
                self.perform_substitution(cmd, &tab_files_pat, tab_files)
            })
    }

    fn substitute_tab_paths(&mut self, cmd: Vec<OsString>) -> Vec<OsString> {
        if self.tab_paths.is_none() { return cmd; }
        let tab_paths = self.tab_paths.take().unwrap();

        tab_paths.into_iter()
            .enumerate()
            .fold(cmd, |cmd, (i, tab_path)| {
                let tab_path_pat = String::from(format!("${}", i));
                self.perform_substitution(cmd, &tab_path_pat, vec![tab_path])
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

                    // Wait a bit so hunter doesn't explode
                    std::thread::sleep(std::time::Duration::from_millis(100));
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
    type Item = ();
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
        let fg = cmd.cmd.as_bytes().ends_with(b"!");

        if fg {
            // Remove that last !
            cmd.cmd = cmd.cmd.trim_end("!");
        }

        let cmd_args = cmd.process().concat(" ");

        // Nicer for display
        let short = "~";
        let short_cmd = cmd_args.clone()
            .replace(&home, short)
            .replace("'\''", "'")
            .replace("\"", "")
            .to_string_lossy()
            .to_string();

        let shell_args = cmd_args;
        let shell_args = vec![OsString::from("-c"), shell_args.clone()];

        cmd.cmd = OsString::from(shell.clone());
        cmd.args = Some(shell_args);
        cmd.short_cmd = Some(short_cmd);

        if !fg {
            self.run_proc_raw(cmd)
        } else {
            self.run_proc_raw_fg(cmd).log();

            // Command might fail/return early. do this here
            self.core.screen.reset()?;
            self.core.screen.activate()?;
            self.core.screen.clear()?;
            Ok(())
        }
    }

    fn run_proc_raw(&mut self, cmd: Cmd) -> HResult<()> {
        let real_cmd = cmd.cmd;
        let short_cmd = cmd.short_cmd
            .unwrap_or(real_cmd
                       .to_string_lossy()
                       .to_string());
        let args = cmd.args.unwrap_or(vec![]);
        let vars = cmd.vars.unwrap_or(vec![]);

        self.core.show_status(&format!("Running: {}", &short_cmd)).log();

        // Need pre_exec here to interleave stderr with stdout
        let handle = unsafe {
            Command::new(real_cmd)
                .args(args)
                .envs(vars)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::piped())
                // Without this stderr would be separate which is no good for procview
                .pre_exec(||  { libc::dup2(1, 2); Ok(()) })
                .spawn()
        };

        let handle = match handle {
            Ok(handle) => handle,
            Err(e) => {
                let msg = format!("Error! Failed to start process: {}",
                                  e);
                self.core.show_status(&msg)?;
                return Err(e)?;
            }
        };

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

    fn run_proc_raw_fg(&mut self, cmd: Cmd) -> HResult<()> {
        let real_cmd = cmd.cmd;
        let short_cmd = cmd.short_cmd
                           .unwrap_or(real_cmd
                                      .to_string_lossy()
                                      .to_string());
        let args = cmd.args.unwrap_or(vec![]);

        self.core.show_status(&format!("Running (fg): {}", &short_cmd)).log();

        self.core.screen.goto_xy(0,0)?;
        self.core.screen.reset()?;
        self.core.screen.suspend()?;

        match Command::new(real_cmd)
            .args(args)
            .status() {
                Ok(status) => {
                    let color_success =
                        if status.success() {
                            format!("{}successfully", term::color_green())
                        } else {
                            format!("{}unsuccessfully", term::color_red())
                        };

                    let color_status =
                        if status.success() {
                            format!("{}{}",
                                    term::color_green(),
                                    status.code().unwrap_or(status
                                                            .signal()
                                                            .unwrap_or(-1)))
                        } else {
                            format!("{}{}",
                                    term::color_red(),
                                    status.code().unwrap_or(status
                                                            .signal()
                                                            .unwrap_or(-1)))

                        };


                    let procinfo = format!("{} exited {}{}{} with status: {}",
                                           short_cmd,
                                           color_success,
                                           term::reset(),
                                           term::status_bg(),
                                           color_status);

                    self.core.show_status(&procinfo)?;
                },
                err @ Err(_) => {
                    self.core.show_status(&format!("{}{} ",
                                                  "Couldn't start process:",
                                                   short_cmd))?;
                    err?;
                }
            }
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

#[derive(Debug, PartialEq)]
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

#[derive(Debug)]
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
        let textview = AsyncWidget::new(&core, move |_| {
            let textview = TextView::new_blank(&tcore);
            Ok(textview)
        });
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
        self.get_textview().get_core()?.clear().log();
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

        self.get_textview().change_to(move |_, core| {
            let mut textview = TextView::new_blank(&core);
            textview.set_text(&output).log();
            textview.animate_slide_up(Some(&animator)).log();
            Ok(textview)
        }).log();

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

        if self.get_listview().len() > 0 {
            self.show_output().log();
            self.get_listview_mut().refresh().log();
            self.get_textview().refresh().log();
        }

        Ok(())
    }
    fn get_drawlist(&self) -> HResult<String> {
        self.hbox.get_drawlist()
    }
    fn on_key(&mut self, key: Key) -> HResult<()> {
        self.do_key(key)?;
        self.refresh().log();
        self.draw().log();

        Ok(())
    }
}



use crate::keybind::*;

impl Acting for ProcView {
    type Action = ProcessAction;

    fn search_in(&self) -> Bindings<Self::Action> {
        self.core.config().keybinds.process
    }

    fn movement(&mut self, movement: &Movement) -> HResult<()> {
        self.get_listview_mut().movement(movement)
    }

    fn do_action(&mut self, action: &Self::Action) -> HResult<()> {
        use ProcessAction::*;

        match action {
            Close => { self.animator.set_stale().log();
                       self.core.clear().log();
                       Err(HError::PopupFinnished)? }
            Remove => self.remove_proc()?,
            Kill => self.get_listview_mut().kill_proc()?,
            FollowOutput => self.toggle_follow()?,
            ScrollOutputDown => self.scroll_down()?,
            ScrollOutputUp => self.scroll_up()?,
            ScrollOutputPageDown => self.page_down()?,
            ScrollOutputPageUp => self.page_up()?,
            ScrollOutputBottom => self.scroll_bottom()?,
            ScrollOutputTop => self.scroll_top()?
        }

        Ok(())
    }
}


impl Acting for ListView<Vec<Process>> {
    type Action=ProcessAction;

    fn search_in(&self) -> Bindings<Self::Action> {
        self.core.config().keybinds.process
    }

    fn movement(&mut self, movement: &Movement) -> HResult<()> {
        use Movement::*;

        match movement {
            Up(n) => { for _ in 0..*n { self.move_up(); }; self.refresh()?; }
            Down(n) => { for _ in 0..*n { self.move_down(); }; self.refresh()?; }
            PageUp => self.page_up(),
            PageDown => self.page_down(),
            Top => self.move_top(),
            Bottom => self.move_bottom(),
            Left | Right => {}
        }

        Ok(())
    }

    fn do_action(&mut self, _action: &Self::Action) -> HResult<()> {
        Ok(())
    }
}
