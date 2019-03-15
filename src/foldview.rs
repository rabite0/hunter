use termion::event::Key;
use failure::Fail;
use chrono::{DateTime, TimeZone, Local};

use crate::term;
use crate::widget::Widget;
use crate::listview::{ListView, Listable};
use crate::fail::{HResult, HError};

pub type LogView = ListView<Vec<LogEntry>>;


#[derive(Debug)]
pub struct LogEntry {
    description: String,
    content: Option<String>,
    lines: usize,
    folded: bool
}


impl Foldable for LogEntry {
    fn description(&self) -> &String {
        &self.description
    }
    fn content(&self) -> Option<&String> {
        self.content.as_ref()
    }
    fn lines(&self) -> usize {
        if self.is_folded() { 1 } else {
            self.lines
        }
    }
    fn toggle_fold(&mut self) {
        self.folded = !self.folded;
    }
    fn is_folded(&self) -> bool {
        self.folded
    }
}


impl From<&HError> for LogEntry {
    fn from(from: &HError) -> LogEntry {
        let time: DateTime<Local> = Local::now();
        let description = format!("{}{}{}: {}",
                                  term::color_green(),
                                  time.format("%F %R"),
                                  term::color_red(),
                                  from).lines().take(1).collect();
        let mut content = format!("{}{}{}: {}\n",
                                  term::color_green(),
                                  time.format("%F %R"),
                                  term::color_red(),
                                  from);


        if let Some(cause) = from.cause() {
            content += &format!("{}\n", cause);
        }

        if let Some(backtrace) = from.backtrace() {
            content += &format!("{}\n", backtrace);
        }

        let lines = content.lines().count();

        LogEntry {
            description: description,
            content: Some(content),
            lines: lines,
            folded: true
        }
    }
}



pub trait FoldableWidgetExt {
    fn on_refresh(&mut self) -> HResult<()> { Ok(()) }
}

impl FoldableWidgetExt for  ListView<Vec<LogEntry>> {
    fn on_refresh(&mut self) -> HResult<()> {
        self.content.refresh_logs()
    }
}

trait LogList {
    fn refresh_logs(&mut self) -> HResult<()>;
}

impl LogList for Vec<LogEntry> {
    fn refresh_logs(&mut self) -> HResult<()> {
        let logs = crate::fail::get_logs()?;

        let mut logentries = logs.into_iter().map(|log| {
            LogEntry::from(log)
        }).collect();

        self.append(&mut logentries);
        //self.truncate(10);
        Ok(())
    }
}


pub trait Foldable {
    fn description(&self) -> &String;
    fn content(&self) -> Option<&String>;
    fn lines(&self) -> usize;
    fn toggle_fold(&mut self);
    fn is_folded(&self) -> bool;

    fn text(&self) -> &String {
        if !self.is_folded() && self.content().is_some() {
            self.content().unwrap()
        } else { self.description() }
    }

    fn render_description(&self) -> String {
        self.description().to_string()
    }

    fn render_content(&self) -> Vec<String> {
        if let Some(content) = self.content() {
            content
                .lines()
                .map(|line| line.to_string())
                .collect()
        } else { vec![self.render_description()] }
    }

    fn render(&self) -> Vec<String> {
        if self.is_folded() {
            vec![self.render_description()]
        } else {
            self.render_content()
        }
    }
}

impl<F: Foldable> ListView<Vec<F>>
where
    ListView<Vec<F>>: FoldableWidgetExt {

    fn toggle_fold(&mut self) -> HResult<()> {
        let fold = self.current_fold();
        let fold_pos = self.fold_start_pos(fold);

        self.content[fold].toggle_fold();

        if self.content[fold].is_folded() {
            self.set_selection(fold_pos);
        }

        self.refresh()
    }

    fn fold_start_pos(&self, fold: usize) -> usize {
        self.content
            .iter()
            .take(fold)
            .fold(0, |pos, foldable| {
                pos + (foldable.lines())
            })
    }

    fn current_fold(&self) -> usize {
        let pos = self.get_selection();

        let fold_lines = self
            .content
            .iter()
            .map(|f| f.lines())
            .collect::<Vec<usize>>();

        fold_lines
            .iter()
            .enumerate()
            .fold((0, None), |(lines, fold_pos), (i, current_fold_lines)| {
                if fold_pos.is_some() {
                    (lines, fold_pos)
                } else {
                    if lines + current_fold_lines > pos {
                        (lines, Some(i))
                    } else {
                        (lines + current_fold_lines, None)
                    }
                }}).1.unwrap()
    }
}


impl<F: Foldable> Listable for ListView<Vec<F>>
where
    ListView<Vec<F>>: FoldableWidgetExt {

    fn len(&self) -> usize {
        self.content.iter().map(|f| f.lines()).sum()
    }

    fn render(&self) -> Vec<String> {
        let (xsize, ysize) = self.core.coordinates.size_u();
        let offset = self.offset;
        self.content
            .iter()
            //.skip(offset)
            .map(|foldable|
                 foldable
                 .render()
                 .iter()
                 .map(|line| term::sized_string_u(line, xsize))
                 .collect::<Vec<_>>())
            .flatten()
            .skip(offset)
            .take(ysize+1)
            .collect()
    }
    fn on_refresh(&mut self) -> HResult<()> {
        FoldableWidgetExt::on_refresh(self)
    }

    fn on_key(&mut self, key: Key) -> HResult<()> {
        match key {
            Key::Up | Key::Char('p') => {
                self.move_up();
                self.refresh()?;
            }
            Key::Char('P') => { for _ in 0..10 { self.move_up() } self.refresh()?; }
            Key::Char('N') => { for _ in 0..10 { self.move_down() } self.refresh()?; }
            Key::Down | Key::Char('n') => {
                self.move_down();
                self.refresh()?;
            },
            Key::Char('t') => { self.toggle_fold()?; }
            Key::Char('l') => { self.popup_finnished()?; }
            _ => {}
        }
        Ok(())
    }
}
