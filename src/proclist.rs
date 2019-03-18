use std::sync::{Arc, Mutex};
use std::sync::mpsc::Sender;
use std::process::Child;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::io::{BufRead, BufReader};

use termion::event::Key;
use unicode_width::UnicodeWidthStr;

use crate::listview::{Listable, ListView};
use crate::textview::TextView;
use crate::widget::{Widget, Events, WidgetCore};
use crate::hbox::HBox;
use crate::preview::WillBeWidget;
use crate::fail::{HResult, HError, ErrorLog};
use crate::term;

#[derive(Debug)]
struct Process {
    cmd: String,
    handle: Arc<Mutex<Child>>,
    output: Arc<Mutex<String>>,
    status: Arc<Mutex<Option<i32>>>,
    success: Arc<Mutex<Option<bool>>>,
    sender: Sender<Events>

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

                let status = format!("Process: {}:{} exited {}{}{} with status: {}",
                                     cmd,
                                     pid,
                                     color_success,
                                     term::reset(),
                                     term::status_bg(),
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
}

impl ListView<Vec<Process>> {
    fn run_proc(&mut self, cmd: &str) -> HResult<()> {
        let shell = std::env::var("SHELL").unwrap_or("sh".into());

        let handle = std::process::Command::new(shell)
            .arg("-c")
            .arg(cmd)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .before_exec(|| unsafe { libc::dup2(1, 2); Ok(()) })
            .spawn()?;
        let mut proc = Process {
            cmd: cmd.to_string(),
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
    TextView(WillBeWidget<TextView>),
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
    viewing: Option<usize>
}

impl HBox<ProcViewWidgets> {
    fn get_listview(&mut self) -> &mut ListView<Vec<Process>> {
        match &mut self.widgets[0] {
            ProcViewWidgets::List(listview) => listview,
            _ => unreachable!()
        }
    }
    fn get_textview(&mut self) -> &mut WillBeWidget<TextView> {
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
        let textview = Box::new(move |_| Ok(TextView::new_blank(&tcore)));
        let textview = WillBeWidget::new(&core, textview);
        let mut hbox = HBox::new(&core);
        hbox.push_widget(ProcViewWidgets::List(listview));
        hbox.push_widget(ProcViewWidgets::TextView(textview));
        hbox.set_ratios(vec![33, 66]);
        hbox.refresh().log();
        ProcView {
            core: core.clone(),
            hbox: hbox,
            viewing: None
        }
    }

    fn get_listview(&mut self) -> &mut ListView<Vec<Process>> {
        self.hbox.get_listview()
    }

    fn get_textview(&mut self) -> &mut WillBeWidget<TextView> {
        self.hbox.get_textview()
    }

    pub fn run_proc(&mut self, cmd: &str) -> HResult<()> {
        self.get_listview().run_proc(cmd)?;
        Ok(())
    }

    pub fn remove_proc(&mut self) -> HResult<()> {
        if self.get_listview().content.len() == 0 { return Ok(()) }
        self.get_listview().remove_proc()?;
        self.get_textview().change_to(Box::new(move |_, core| {
            let mut textview = TextView::new_blank(&core);
            textview.refresh().log();
            textview.animate_slide_up().log();
            Ok(textview)
        })).log();
        Ok(())
    }

    fn show_output(&mut self) -> HResult<()> {
        if Some(self.get_listview().get_selection()) == self.viewing {
            return Ok(());
        }
        let output = self.get_listview().selected_proc()?.output.lock()?.clone();

        self.get_textview().change_to(Box::new(move |_, core| {
            let mut textview = TextView::new_blank(&core);
            textview.set_text(&output).log();
            textview.animate_slide_up().log();
            Ok(textview)
        })).log();
        self.viewing = Some(self.get_listview().get_selection());
        Ok(())
    }

    pub fn toggle_follow(&mut self) -> HResult<()> {
        self.get_textview().widget()?.lock()?.as_mut()?.toggle_follow();
        Ok(())
    }

    pub fn scroll_up(&mut self) -> HResult<()> {
        self.get_textview().widget()?.lock()?.as_mut()?.scroll_up();
        Ok(())
    }

    pub fn scroll_down(&mut self) -> HResult<()> {
        self.get_textview().widget()?.lock()?.as_mut()?.scroll_down();
        Ok(())
    }

    pub fn page_up(&mut self) -> HResult<()> {
        self.get_textview().widget()?.lock()?.as_mut()?.page_up();
        Ok(())
    }

    pub fn page_down(&mut self) -> HResult<()> {
        self.get_textview().widget()?.lock()?.as_mut()?.page_down();
        Ok(())
    }

    pub fn scroll_top(&mut self) -> HResult<()> {
        self.get_textview().widget()?.lock()?.as_mut()?.scroll_top();
        Ok(())
    }

    pub fn scroll_bottom(&mut self) -> HResult<()> {
        self.get_textview().widget()?.lock()?.as_mut()?.scroll_bottom();
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
    fn refresh(&mut self) -> HResult<()> {
        self.hbox.refresh().log();

        self.show_output().log();
        self.get_listview().refresh().log();
        self.get_textview().refresh().log();

        Ok(())
    }
    fn get_drawlist(&self) -> HResult<String> {
        self.hbox.get_drawlist()
    }
    fn on_key(&mut self, key: Key) -> HResult<()> {
        match key {
            Key::Char('w') => { return Err(HError::PopupFinnished) }
            Key::Char('d') => { self.remove_proc()? }
            Key::Char('k') => { self.get_listview().kill_proc()? }
            Key::Up | Key::Char('p') => {
                self.get_listview().move_up();
            }
            Key::Down | Key::Char('n') => {
                self.get_listview().move_down();
            }
            Key::Char('f') => { self.toggle_follow().log(); }
            Key::Ctrl('n') => { self.scroll_down().log(); },
            Key::Ctrl('p') => { self.scroll_up().log(); },
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
