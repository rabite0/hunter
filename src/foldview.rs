use chrono::{DateTime, Local};
use failure::Fail;
use termion::event::Key;

use crate::dirty::Dirtyable;
use crate::fail::{HError, HResult, KeyBindError};
use crate::keybind::{Acting, AnyKey, BindingSection, Bindings, FoldAction, LogAction, Movement};
use crate::listview::{ListView, Listable};
use crate::term;
use crate::widget::Widget;

pub type LogView = ListView<Vec<LogEntry>>;

#[derive(Debug)]
pub struct LogEntry {
    description: String,
    content: Option<String>,
    lines: usize,
    folded: bool,
}

impl Foldable for LogEntry {
    fn description(&self) -> &str {
        &self.description
    }
    fn content(&self) -> Option<&String> {
        self.content.as_ref()
    }
    fn lines(&self) -> usize {
        if self.is_folded() {
            1
        } else {
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

        let logcolor = match from {
            HError::Log(_) => term::normal_color(),
            _ => term::color_red(),
        };

        let description = format!(
            "{}{}{}: {}",
            term::color_green(),
            time.format("%F %R"),
            logcolor,
            from
        )
        .lines()
        .take(1)
        .collect();
        let mut content = format!(
            "{}{}{}: {}\n",
            term::color_green(),
            time.format("%F %R"),
            logcolor,
            from
        );

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
            folded: true,
        }
    }
}

pub trait ActingExt
where
    Self::Action: BindingSection + std::fmt::Debug,
    Bindings<Self::Action>: Default,
    Self: Widget,
{
    type Action;

    fn search_in(&self) -> Bindings<Self::Action>;
    fn movement(&mut self, _movement: &Movement) -> HResult<()> {
        Err(KeyBindError::MovementUndefined)?
    }
    fn do_key_ext(&mut self, key: Key) -> HResult<()> {
        let gkey = AnyKey::from(key);

        // Moving takes priority
        if let Some(movement) = self.get_core()?.config().keybinds.movement.get(gkey) {
            match self.movement(movement) {
                Ok(()) => return Ok(()),
                Err(HError::KeyBind(KeyBindError::MovementUndefined)) => {}
                Err(e) => Err(e)?,
            }
        }

        self.search_in();

        let bindings = self.search_in();

        if let Some(action) = bindings.get(key) {
            return self.do_action(action);
        } else if let Some(any_key) = gkey.any() {
            if let Some(action) = bindings.get(any_key) {
                let action = action.insert_key_param(key);
                return self.do_action(&action);
            }
        }

        HError::undefined_key(key)
    }
    fn do_action(&mut self, _action: &Self::Action) -> HResult<()> {
        Err(KeyBindError::MovementUndefined)?
    }
}

impl ActingExt for ListView<Vec<LogEntry>> {
    type Action = LogAction;

    fn search_in(&self) -> Bindings<Self::Action> {
        self.core.config().keybinds.log
    }

    fn do_action(&mut self, action: &Self::Action) -> HResult<()> {
        match action {
            LogAction::Close => self.popup_finnished(),
        }
    }
}

pub trait FoldableWidgetExt
where
    Self: ActingExt,
    Bindings<<Self as ActingExt>::Action>: Default,
{
    fn on_refresh(&mut self) -> HResult<()> {
        Ok(())
    }
    fn render_header(&self) -> HResult<String> {
        Ok("".to_string())
    }
    fn render_footer(&self) -> HResult<String> {
        Ok("".to_string())
    }
    fn on_key(&mut self, key: Key) -> HResult<()> {
        HError::undefined_key(key)?
    }
    fn render(&self) -> Vec<String> {
        vec![]
    }
}

impl FoldableWidgetExt for ListView<Vec<LogEntry>> {
    fn on_refresh(&mut self) -> HResult<()> {
        if self.content.refresh_logs()? > 0 {
            self.core.set_dirty();
        }
        Ok(())
    }

    fn render_header(&self) -> HResult<String> {
        let (xsize, _) = self.core.coordinates.size_u();
        let current = self.current_fold().map(|n| n + 1).unwrap_or(0);
        let num = self.content.len();
        let hint = format!("{} / {}", current, num);
        let hint_xpos = xsize - hint.len();
        let header = format!(
            "Logged entries: {}{}{}",
            num,
            term::goto_xy_u(hint_xpos, 0),
            hint
        );
        Ok(header)
    }

    fn render_footer(&self) -> HResult<String> {
        let current = self.current_fold().ok_or_else(|| HError::NoneError)?;
        if let Some(logentry) = self.content.get(current) {
            let (xsize, ysize) = self.core.coordinates.size_u();
            let (_, ypos) = self.core.coordinates.position_u();
            let description = logentry.description();
            let lines = logentry.lines();
            let start_pos = self.fold_start_pos(current);
            let selection = self.get_selection();
            let current_line = (selection - start_pos) + 1;
            let line_hint = format!("{} / {}", current_line, lines);
            let hint_xpos = xsize - line_hint.len();
            let hint_ypos = ysize + ypos + 1;

            let sized_description =
                term::sized_string_u(&description, xsize - (line_hint.len() + 2));

            let footer = format!(
                "{}{}{}{}{}",
                sized_description,
                term::reset(),
                term::status_bg(),
                term::goto_xy_u(hint_xpos, hint_ypos),
                line_hint
            );

            Ok(footer)
        } else {
            Ok("No log entries".to_string())
        }
    }
}

trait LogList {
    fn refresh_logs(&mut self) -> HResult<usize>;
}

impl LogList for Vec<LogEntry> {
    fn refresh_logs(&mut self) -> HResult<usize> {
        let logs = crate::fail::get_logs()?;

        let mut logentries = logs
            .into_iter()
            .map(|log| LogEntry::from(log))
            .collect::<Vec<_>>();

        let n = logentries.len();

        self.append(&mut logentries);

        Ok(n)
    }
}

pub trait Foldable {
    fn description(&self) -> &str;
    fn content(&self) -> Option<&String>;
    fn lines(&self) -> usize;
    fn toggle_fold(&mut self);
    fn is_folded(&self) -> bool;

    fn text(&self) -> &str {
        if !self.is_folded() && self.content().is_some() {
            self.content().unwrap()
        } else {
            &self.description()
        }
    }

    fn render_description(&self) -> String {
        self.description().to_string()
    }

    fn render_content(&self) -> Vec<String> {
        if let Some(content) = self.content() {
            content.lines().map(|line| line.to_string()).collect()
        } else {
            vec![self.render_description()]
        }
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
    ListView<Vec<F>>: FoldableWidgetExt,
    Bindings<<ListView<Vec<F>> as ActingExt>::Action>: Default,
{
    pub fn toggle_fold(&mut self) -> HResult<()> {
        let fold = self.current_fold().ok_or_else(|| HError::NoneError)?;
        let fold_pos = self.fold_start_pos(fold);

        self.content[fold].toggle_fold();

        if self.content[fold].is_folded() {
            self.set_selection(fold_pos);
        }

        self.core.set_dirty();
        Ok(())
    }

    pub fn fold_start_pos(&self, fold: usize) -> usize {
        self.content
            .iter()
            .take(fold)
            .fold(0, |pos, foldable| pos + (foldable.lines()))
    }

    pub fn current_fold(&self) -> Option<usize> {
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
                }
            })
            .1
    }
}

impl<F: Foldable> Listable for ListView<Vec<F>>
where
    ListView<Vec<F>>: FoldableWidgetExt,
    Bindings<<ListView<Vec<F>> as ActingExt>::Action>: Default,
{
    type Item = ();

    fn len(&self) -> usize {
        self.content.iter().map(|f| f.lines()).sum()
    }

    fn render(&self) -> Vec<String> {
        let rendering = FoldableWidgetExt::render(self);
        // HACK to check if no custom renderer
        if rendering.len() > 0 {
            return rendering;
        }

        let (xsize, _) = self.core.coordinates.size_u();
        self.content
            .iter()
            .map(|foldable| {
                foldable
                    .render()
                    .iter()
                    .map(|line| term::sized_string_u(line, xsize))
                    .collect::<Vec<_>>()
            })
            .flatten()
            .collect()
    }

    fn render_header(&self) -> HResult<String> {
        FoldableWidgetExt::render_header(self)
    }

    fn render_footer(&self) -> HResult<String> {
        FoldableWidgetExt::render_footer(self)
    }

    fn on_refresh(&mut self) -> HResult<()> {
        FoldableWidgetExt::on_refresh(self)
    }

    fn on_key(&mut self, key: Key) -> HResult<()> {
        match ActingExt::do_key_ext(self, key) {
            Err(HError::PopupFinnished) => Err(HError::PopupFinnished),
            _ => self.do_key(key),
        }
    }
}

impl<F: Foldable> Acting for ListView<Vec<F>>
where
    ListView<Vec<F>>: FoldableWidgetExt,
    Bindings<<ListView<Vec<F>> as ActingExt>::Action>: Default,
{
    type Action = FoldAction;

    fn search_in(&self) -> Bindings<Self::Action> {
        self.core.config().keybinds.fold
    }

    fn movement(&mut self, movement: &Movement) -> HResult<()> {
        use Movement::*;

        match movement {
            Up(n) => {
                for _ in 0..*n {
                    self.move_up()
                }
            }
            Down(n) => {
                for _ in 0..*n {
                    self.move_down()
                }
            }
            _ => Err(KeyBindError::MovementUndefined)?,
        }

        Ok(())
    }

    fn do_action(&mut self, action: &FoldAction) -> HResult<()> {
        use FoldAction::*;

        match action {
            ToggleFold => self.toggle_fold(),
        }
    }
}
