use termion::event::Key;
use ini::Ini;
use strum::IntoEnumIterator;

use std::collections::HashMap;
use std::default::Default;
use std::str::FromStr;
use std::fmt::{Display, Debug};

use crate::fail::{HError, HResult, KeyBindError, ErrorLog};
use crate::widget::Widget;


pub type KbResult<T> = Result<T, KeyBindError>;


#[derive(Clone, Debug)]
pub struct Bindings<T>(HashMap<AnyKey, T>);

impl<T> Bindings<T> {
    pub fn get(&self,
               key: impl Into<AnyKey>) -> Option<&T> {
        self.0.get(&key.into())
    }

    pub fn insert(&mut self,
                  key: impl Into<AnyKey>,
                  value: T) -> Option<T> {
        self.0.insert(key.into(), value)
    }

    pub fn new() -> Self {
        Bindings(HashMap::new())
    }
}




pub trait Acting
where
    Self: Widget,
    Self::Action: BindingSection + Debug,
    Bindings<Self::Action>: Default,
{
    type Action;

    fn search_in(&self) -> Bindings<Self::Action>;
    fn do_action(&mut self, action: &Self::Action) -> HResult<()>;

    fn movement(&mut self, _movement: &Movement) -> HResult<()> {
        Err(KeyBindError::MovementUndefined)?
    }

    fn do_key(&mut self, key: Key) -> HResult<()> {
        let gkey = AnyKey::from(key);

        // Moving takes priority
        if let Some(movement) = self.get_core()?
            .config()
            .keybinds
            .movement
            .get(gkey) {
                match self.movement(movement) {
                    Ok(()) => return Ok(()),
                    Err(HError::KeyBind(KeyBindError::MovementUndefined)) => {}
                    Err(e) => Err(e)?
                }
            }

        self.search_in();

        let bindings = self.search_in();

        if let Some(action) = bindings.get(key) {
            return self.do_action(action)
        } else if let Some(any_key) = gkey.any() {
            if let Some(action) = bindings.get(any_key) {
                let action = action.insert_key_param(key);
                return self.do_action(&action);
            }
        }

        HError::undefined_key(key)
    }
}


#[derive(Clone, Debug)]
pub struct KeyBinds {
    pub movement: Bindings<Movement>,
    pub filebrowser: Bindings<FileBrowserAction>,
    pub filelist: Bindings<FileListAction>,
    pub tab: Bindings<TabAction>,
    pub media: Bindings<MediaAction>,
    pub bookmark: Bindings<BookmarkAction>,
    pub process: Bindings<ProcessAction>,
    pub minibuffer: Bindings<MiniBufferAction>,
    pub fold: Bindings<FoldAction>,
    pub log: Bindings<LogAction>,
    pub quickaction: Bindings<QuickActionAction>,
}

impl Default for KeyBinds {
    fn default() -> Self {
        KeyBinds {
            movement: Bindings::default(),
            filebrowser: Bindings::default(),
            filelist: Bindings::default(),
            tab: Bindings::default(),
            media: Bindings::default(),
            bookmark: Bindings::default(),
            process: Bindings::default(),
            minibuffer: Bindings::default(),
            fold: Bindings::default(),
            log: Bindings::default(),
            quickaction: Bindings::default()
        }
    }
}


impl KeyBinds {
    pub fn load() -> HResult<KeyBinds> {
        let bindings_path = crate::paths::bindings_path()?;
        let ini = Ini::load_from_file_noescape(bindings_path)
            .map_err(KeyBindError::from)?;

        let movement = Movement::load_section(&ini);
        let filebrowser = FileBrowserAction::load_section(&ini);
        let filelist = FileListAction::load_section(&ini);
        let tab = TabAction::load_section(&ini);
        let media = MediaAction::load_section(&ini);
        let bookmark = BookmarkAction::load_section(&ini);
        let process = ProcessAction::load_section(&ini);
        let minibuffer = MiniBufferAction::load_section(&ini);
        let fold = FoldAction::load_section(&ini);
        let log = LogAction::load_section(&ini);
        let quickaction = QuickActionAction::load_section(&ini);

        Ok(KeyBinds {
            movement,
            filebrowser,
            filelist,
            tab,
            media,
            bookmark,
            process,
            minibuffer,
            fold,
            log,
            quickaction
        })
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum AnyKey {
    Key(Key),
    AnyChar,
    AnyF,
    AnyCtrl,
    AnyAlt
}

impl Display for AnyKey {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        use AnyKey::*;
        use termion::event::Key::*;

        match self {
            Key(key) => match key {
                Char(ch) => write!(fmt, "{}", ch),
                Alt(ch) => write!(fmt, "M-{}", ch),
                Ctrl(ch) => write!(fmt, "C-{}", ch),
                F(n) => write!(fmt, "F{}", n),
                k @ _ => write!(fmt, "{:?}", k)
            }
            AnyChar => write!(fmt, "_"),
            AnyF => write!(fmt, "F_"),
            AnyCtrl => write!(fmt, "C-_"),
            AnyAlt => write!(fmt, "M-_")
        }
    }
}

impl AnyKey {
    pub fn any(&self) -> Option<AnyKey> {
        use AnyKey::*;
        use termion::event::Key::*;

        match self {
            Key(F(_)) => Some(AnyF),
            Key(Char(_)) => Some(AnyChar),
            Key(Ctrl(_)) => Some(AnyCtrl),
            Key(Alt(_)) => Some(AnyAlt),
            _ => None
        }
    }
}

impl From<Key> for AnyKey {
    fn from(key: Key) -> Self {
        AnyKey::Key(key)
    }
}

impl FromStr for AnyKey {
    type Err = KeyBindError;

    fn from_str(key: &str) -> Result<AnyKey, Self::Err> {
        use AnyKey::*;
        use termion::event::Key::*;

        let key_err = |key: &str| {
            KeyBindError::ParseKeyError(key.to_string())
        };

        let predefined = |key| {
            match key {
                "Backspace" => Some(Key(Backspace)),
                "Left" => Some(Key(Left)),
                "Right" => Some(Key(Right)),
                "Up" => Some(Key(Up)),
                "Down" => Some(Key(Down)),
                "Home" => Some(Key(Home)),
                "End" => Some(Key(End)),
                "PageUp" => Some(Key(PageUp)),
                "PageDown" => Some(Key(PageDown)),
                "Delete" => Some(Key(Delete)),
                "Insert" => Some(Key(Insert)),
                "Tab" => Some(Key(Char('\t'))),
                "BackTab" => Some(Key(BackTab)),
                "Enter" => Some(Key(Char('\n'))),
                "Space" => Some(Key(Char(' '))),
                "\\_" => Some(Key(Char('_'))),
                "_" => Some(AnyChar),
                "Esc" => Some(Key(Esc)),
                _ => None
            }
        };

        if let Some(key) = predefined(key) {
            return Ok(key);
        }


        if key.starts_with("F") && key.len() == 2 {
            let chr = key.get(1..2);

            if chr == Some("_") {
                Ok(AnyF)
            } else {
                chr.ok_or_else(|| key_err(key))
                    .and_then(|num|
                              num
                              .parse()
                              .map(|n|
                                   Key(F(n)))
                              .map_err(|_| key_err(key)))
                }
        } else if let Ok(key) = key.parse() {
            Ok(Key(Char(key)))
        } else {
            let parts = key.split('-').collect::<Vec<&str>>();

            let (modifier, maybe_key) = if parts.len() > 2 {
                // Something is wrong if there are more parts
                return Err(key_err(key));
            } else {
                (parts.get(0).and_then(|p| p.parse().ok()),
                 parts.get(1).and_then(|p| p.parse().ok()))
            };

            match (modifier, maybe_key) {
                (Some(ch), None) => match ch {
                    '_' => Ok(AnyChar),
                    _ => Ok(Key(Char(ch)))
                }
                (Some('C'), Some(ch)) => match ch {
                    '_' => Ok(AnyCtrl),
                    _ => Ok(Key(Ctrl(ch)))
                }
                (Some('A'), Some(ch)) => match ch {
                    '_' => Ok(AnyAlt),
                    _ => Ok(Key(Alt(ch)))
                }
                (Some('M'), Some(ch)) => match ch {
                    '_' => Ok(AnyAlt),
                    _ => Ok(Key(Alt(ch)))
                }
                _ => Err(key_err(key))
            }
        }
    }
}




#[derive(Copy, Clone, Display, Debug)]
pub enum CharOrNum {
    Char(char),
    Num(usize),
    Any
}

impl CharOrNum {
    fn char_or(self, default: char) -> char {
        match self {
            CharOrNum::Char(ch) => ch,
            CharOrNum::Any => default,
            _ => {
                KeyBindError::CharOrNumWrongType(String::from("char"),
                                                 String::from("number")).log();
                default
            }
        }
    }

    fn num_or(self, default: usize) -> usize {
        match self {
            CharOrNum::Num(num) => num,
            CharOrNum::Any => default,
            _ => {
                KeyBindError::CharOrNumWrongType(String::from("number"),
                                                 String::from("char")).log();
                default
            }
        }
    }
}

impl FromStr for CharOrNum {
    type Err = KeyBindError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim_start_matches("(")
            .trim_end_matches(")");

        if s == "_" {
            return Ok(Self::Any)
        }

        if let Ok(num) = s.parse() {
            Ok(Self::Num(num))
        } else if let Ok(ch) = s.parse() {
            Ok(Self::Char(ch))
        } else {
            Err(KeyBindError::CharOrNumParseError(s.to_string()))
        }
    }
}




pub trait BindingSection
where
    Self: FromStr + Copy + Display + Debug,
    Bindings<Self>: Default
{
    fn section() -> &'static str;

    fn process_action_str(action_str: &str) -> (&str, Option<CharOrNum>) {
        // Could be something like Up(10) for going up 10 times
        action_str.rfind("(")
            .map(|split_pos| {
                let split = action_str.split_at(split_pos);
                let action = split.0;
                let param = split.1
                    .trim_start_matches("(")
                    .trim_end_matches(")");
                (action, param.parse().log_and().ok())
            }).unwrap_or((action_str, None))

    }

    // statically inserts hardcoded stuff from config like "Up(10)" into action
    fn insert_config_param(self, param: CharOrNum) -> Self {
        let msg = format!("Warning: Unsupported config parameter {:?} for {}",
                          param,
                          self);
        HError::log::<()>(&msg).ok();
        self
    }

    // dynamically inserts stuff like number from "F(8)" key event into action
    fn insert_key_param(self, _key: Key) -> Self {
        self
    }

    // sets default values for actions with parameters
    fn as_default(self) -> Self {
        self
    }

    fn parse_section(ini: &Ini) -> HResult<Bindings<Self>> {
        let section = ini.section(Some(Self::section())).ok_or_else(|| HError::NoneError)?;

        let mut bindings = Bindings::new();

        for (action_str, keys_str) in section.iter() {
            let (action_str, config_param) = Self::process_action_str(action_str);

            let action = Self::from_str(action_str)
                .map_err(|_| KeyBindError::WrongAction(action_str.to_string()))
                .map_err(HError::from)
                .map(|act|
                     if let Some(cp) = config_param {
                         act.insert_config_param(cp)
                     } else {
                         // default() values on e.g. usize are often useless for actions
                         act.as_default()
                     });

            // If action isn't valid log it and try next binding
            if action.is_err() { action.log(); continue; }

            for key_str in keys_str.split(",") {
                let key_str = key_str.trim();

                let key = key_str.parse::<AnyKey>()
                    .map_err(|_| KeyBindError::WrongKey(action_str.to_string(),
                                                        key_str.to_string()));

                // If key isn't valid log it and try next binding
                if key.is_err() { key.log(); continue; }

                bindings.insert(key?, action.clone()?);
            }
        }

        Ok(bindings)
    }

    fn load_section(ini: &Ini) -> Bindings<Self> {
        Self::parse_section(ini)
            .log_and()
            .unwrap_or_else(|_| Bindings::default())
    }
}








#[derive(EnumString, EnumIter, Copy, Clone, Display, Debug)]
pub enum Movement {
    Up(usize),
    Down(usize),
    Left,
    Right,
    Top,
    Bottom,
    PageUp,
    PageDown,
}



#[derive(EnumString, EnumIter, Copy, Clone, Display, Debug)]
pub enum FileBrowserAction {
    LeftColumnDown,
    LeftColumnUp,
    GotoHome,
    TurboCd,
    SelectExternal,
    EnterDirExternal,
    Quit,
    QuitWithDir,
    RunInBackground,
    GotoPrevCwd,
    ShowBookmarks,
    AddBookmark,
    ShowProcesses,
    ShowLog,
    ShowQuickActions,
    RunSubshell,
    ToggleColumns,
    ZoomPreview,
    ExecCmd
}



#[derive(EnumString, EnumIter, Copy, Clone, Display, Debug)]
pub enum FileListAction {
    Search,
    SearchNext,
    SearchPrev,
    Filter,
    Select,
    InvertSelection,
    ClearSelection,
    FilterSelection,
    ToggleTag,
    ToggleHidden,
    ReverseSort,
    CycleSort,
    ToNextMtime,
    ToPrevMtime,
    ToggleDirsFirst,
}



#[derive(EnumString, EnumIter, Copy, Clone, Display, Debug)]
pub enum TabAction {
    NewTab,
    CloseTab,
    NextTab,
    PrevTab,
    GotoTab(usize),
}



#[derive(EnumString, EnumIter, Copy, Clone, Display, Debug)]
pub enum MediaAction {
    TogglePause,
    ToggleMute,
    SeekForward,
    SeekBackward,
}



#[derive(EnumString, EnumIter, Copy, Clone, Display, Debug)]
pub enum BookmarkAction {
    GotoLastCwd,
    Goto(char),
    Delete(char)
}



#[derive(EnumString, EnumIter, Copy, Clone, Display, Debug)]
pub enum ProcessAction {
    Close,
    Remove,
    Kill,
    FollowOutput,
    ScrollOutputDown,
    ScrollOutputUp,
    ScrollOutputPageDown,
    ScrollOutputPageUp,
    ScrollOutputBottom,
    ScrollOutputTop
}



#[derive(EnumString, EnumIter, Copy, Clone, Display, Debug)]
pub enum MiniBufferAction {
    InsertChar(char),
    InsertTab(usize),
    Cancel,
    Finish,
    Complete,
    DeleteChar,
    BackwardDeleteChar,
    CursorLeft,
    CursorRight,
    HistoryUp,
    HistoryDown,
    ClearLine,
    DeleteWord,
    CursorToStart,
    CursorToEnd
}

#[derive(EnumString, EnumIter, Copy, Clone, Display, Debug)]
pub enum FoldAction {
    ToggleFold
}

#[derive(EnumString, EnumIter, Copy, Clone, Display, Debug)]
pub enum LogAction {
    Close
}

#[derive(EnumString, EnumIter, Copy, Clone, Display, Debug)]
pub enum QuickActionAction {
    Close,
    SelectOrRun(char)
}






impl BindingSection for Movement {
    fn section() -> &'static str {
        "movement"
    }

    fn insert_config_param(self, param: CharOrNum) -> Self {
        use Movement::*;

        let n = param.num_or(1);

        match self {
            Up(_) => Up(n),
            Down(_) => Down(n),
            _ => self
        }
    }

    fn as_default(self) -> Self {
        use Movement::*;

        match self {
            Up(_) => Up(1),
            Down(_) => Down(1),
            _ => self
        }
    }


}


impl Default for Bindings<Movement> {
    fn default() -> Self {
        use Movement::*;

        let mut movement = Bindings::new();

        for action in Movement::iter() {
            let key = match action {
                Up(_) => Key::Char('k'),
                Down(_) => Key::Char('j'),
                Left => Key::Char('h'),
                Right => Key::Char('l'),
                Top => Key::Char('<'),
                Bottom => Key::Char('>'),
                PageUp => Key::PageUp,
                PageDown => Key::PageDown,
            };

            movement.insert(key, action.as_default());
        }

        movement.insert(Key::Char('K'), Movement::Up(10));
        movement.insert(Key::Char('J'), Movement::Down(10));
        movement.insert(Key::Down, Movement::Down(1));
        movement.insert(Key::Up, Movement::Up(1));
        movement.insert(Key::Home, Movement::Top);
        movement.insert(Key::End, Movement::Bottom);
        movement.insert(Key::Ctrl('v'), Movement::PageDown);
        movement.insert(Key::Ctrl('V'), Movement::PageUp);


        movement
    }
}


impl Default for Bindings<FileBrowserAction> {
    fn default() -> Self {
        use Key::*;
        use FileBrowserAction::*;

        let mut filebrowser = Bindings::new();

        for action in FileBrowserAction::iter() {
            let key = match action {
                LeftColumnDown => Char(']'),
                LeftColumnUp => Char('['),
                GotoHome => Char('~'),
                TurboCd => Char('/'),
                SelectExternal => Alt(' '),
                EnterDirExternal => Char('/'),
                Quit => Char('q'),
                QuitWithDir => Char('Q'),
                RunInBackground => Char('F'),
                GotoPrevCwd => Char('-'),
                ShowBookmarks => Char('`'),
                AddBookmark => Char('b'),
                ShowProcesses => Char('w'),
                ShowLog => Char('l'),
                ShowQuickActions => Char('a'),
                RunSubshell => Char('z'),
                ToggleColumns => Char('c'),
                ZoomPreview => Char('C'),
                ExecCmd => Char('!')
            };

            filebrowser.insert(key, action.as_default());
        }

        filebrowser
    }
}

impl BindingSection for FileBrowserAction {
    fn section() -> &'static str {
        "filebrowser"
    }
}

impl Default for Bindings<FileListAction> {
    fn default() -> Self {
        use Key::*;
        use FileListAction::*;

        let mut filelist = Bindings::new();

        for action in FileListAction::iter() {
            let key = match action {
                Search => Ctrl('s'),
                SearchNext => Alt('s'),
                SearchPrev => Alt('S'),
                Filter => Ctrl('f'),
                Select => Char(' '),
                InvertSelection => Char('v'),
                ClearSelection => Char('V'),
                FilterSelection => Alt('V'),
                ToggleTag => Char('t'),
                ToggleHidden => Char('h'),
                ReverseSort => Char('r'),
                CycleSort => Char('s'),
                ToNextMtime => Char('K'),
                ToPrevMtime => Char('k'),
                ToggleDirsFirst => Char('d')
            };

            filelist.insert(key, action.as_default());
        }

        filelist
    }
}

impl BindingSection for FileListAction {
    fn section() -> &'static str {
        "filelist"
    }
}

impl Default for Bindings<TabAction> {
    fn default() -> Self {
        use Key::*;
        use TabAction::*;

        let mut tab = Bindings::new();

        for action in TabAction::iter() {
            let key = match action {
                NewTab => Ctrl('t').into(),
                NextTab => Char('\t').into(),
                PrevTab => BackTab.into(),
                CloseTab => Ctrl('w').into(),
                GotoTab(_) => AnyKey::AnyF
            };

            tab.insert(key, action.as_default());
        }

        tab
    }
}

impl BindingSection for TabAction {
    fn section() -> &'static str {
        "tabs"
    }

    fn insert_config_param(self, param: CharOrNum) -> Self {
        use TabAction::*;

        let n = param.num_or(1);

        match self {
            GotoTab(_) => GotoTab(n),
            _ => self
        }
    }

    fn insert_key_param(self, key: Key) -> Self {
        match (self, key) {
            (TabAction::GotoTab(_), Key::F(n)) => TabAction::GotoTab(n as usize - 1),
            _ => self
        }
    }
}

impl Default for Bindings<MediaAction> {
    fn default() -> Self {
        use Key::*;
        use MediaAction::*;

        let mut media = Bindings::new();

        for action in MediaAction::iter() {
            let key = match action {
                TogglePause => Alt('m'),
                ToggleMute => Alt('M'),
                SeekForward => Alt('>'),
                SeekBackward => Alt('<')
            };

            media.insert(key, action.as_default());
        }

        media
    }
}

impl BindingSection for MediaAction {
    fn section() -> &'static str {
        "media"
    }
}

impl Default for Bindings<BookmarkAction> {
    fn default() -> Self {
        use Key::*;
        use BookmarkAction::*;

        let mut bookmark = Bindings::new();

        for action in BookmarkAction::iter() {
            let key = match action {
                GotoLastCwd => Char('`').into(),
                Goto(_) => AnyKey::AnyChar,
                BookmarkAction::Delete(_) => AnyKey::AnyAlt
            };

            bookmark.insert(key, action.as_default());
        }



        bookmark
    }
}

impl BindingSection for BookmarkAction {
    fn section() -> &'static str {
        "bookmarks"
    }


    fn insert_config_param(self, param: CharOrNum) -> Self {
        use BookmarkAction::*;

        let ch = param.char_or('E');

        match self {
            Goto(_) => Goto(ch),
            _ => self
        }
    }

    fn insert_key_param(self, key: Key) -> Self {
        use BookmarkAction::*;

        match (self, key) {
            (Goto(_), Key::Char(ch)) => Goto(ch),
            (Delete(_), Key::Char(ch)) => Delete(ch),
            _ => self
        }
    }
}


impl Default for Bindings<ProcessAction> {
    fn default() -> Self {
        use Key::*;
        use ProcessAction::*;

        let mut process = Bindings::new();

        for action in ProcessAction::iter() {
            let key = match action {
                Close => Char('w'),
                Remove => Char('d'),
                Kill => Char('k'),
                FollowOutput => Char('f'),
                ScrollOutputDown => Ctrl('n'),
                ScrollOutputUp => Ctrl('p'),
                ScrollOutputPageDown => Ctrl('v'),
                ScrollOutputPageUp => Ctrl('V'),
                ScrollOutputBottom => Char('>'),
                ScrollOutputTop => Ctrl('<')
            };

            process.insert(key, action.as_default());
        }

        process
    }
}

impl BindingSection for ProcessAction {
    fn section() -> &'static str {
        "processes"
    }
}


impl Default for Bindings<MiniBufferAction> {
    fn default() -> Self {
        use termion::event::Key::*;
        use AnyKey::*;
        use MiniBufferAction::*;

        let mut minibuffer = Bindings::new();

        for action in MiniBufferAction::iter() {
            let key = match action {
                InsertChar(_) => AnyChar,
                InsertTab(_) => AnyF,
                Cancel => Ctrl('c').into(),
                Finish => Char('\n').into(),
                Complete => Char('\t').into(),
                DeleteChar => Delete.into(),
                BackwardDeleteChar => Backspace.into(),
                CursorLeft => Ctrl('b').into(),
                CursorRight => Ctrl('f').into(),
                HistoryUp => Ctrl('p').into(),
                HistoryDown => Ctrl('n').into(),
                ClearLine => Ctrl('u').into(),
                DeleteWord => Ctrl('h').into(),
                CursorToStart => Ctrl('a').into(),
                CursorToEnd => Ctrl('e').into()
        };

            minibuffer.insert(key, action.as_default());
        }

        minibuffer.insert(Esc, Cancel);
        minibuffer.insert(AnyKey::AnyChar, InsertChar('E'));
        minibuffer.insert(Ctrl('d'), DeleteChar);
        minibuffer.insert(Left, CursorLeft);
        minibuffer.insert(Right, CursorRight);
        minibuffer.insert(Alt('p'), HistoryUp);
        minibuffer.insert(Alt('n'), HistoryDown);
        minibuffer.insert(Down, HistoryDown);

        minibuffer
    }
}

impl BindingSection for MiniBufferAction {
    fn section() -> &'static str {
        "minibuffer"
    }

    fn insert_config_param(self, param: CharOrNum) -> Self {
        use MiniBufferAction::*;

        let ch = param.char_or('E');

        match self {
            InsertChar(_) => InsertChar(ch),
            _ => self
        }
    }

    fn insert_key_param(self, key: Key) -> Self {
        use MiniBufferAction::*;
        use Key::*;

        match (self, key) {
            (InsertChar(_), Char(ch)) => InsertChar(ch),
            (InsertTab(_), F(n)) => InsertTab(n as usize),
            _ => self
        }
    }
}




impl Default for Bindings<FoldAction> {
    fn default() -> Self {
        use Key::*;
        use FoldAction::*;

        let mut fold = Bindings::new();

        for action in FoldAction::iter() {
            let key = match action {
                ToggleFold => Char('t')
        };

            fold.insert(key, action.as_default());
        }

        fold
    }
}

impl BindingSection for FoldAction {
    fn section() -> &'static str {
        "fold"
    }
}

impl Default for Bindings<LogAction> {
    fn default() -> Self {
        use Key::*;
        use LogAction::*;

        let mut log = Bindings::new();

        for action in LogAction::iter() {
            let key = match action {
                Close => Char('l')
            };

            log.insert(key, action.as_default());
        }

        log
    }
}

impl BindingSection for LogAction {
    fn section() -> &'static str {
        "log"
    }
}

impl Default for Bindings<QuickActionAction> {
    fn default() -> Self {
        use AnyKey::*;
        use termion::event::Key::*;
        use QuickActionAction::*;

        let mut quickaction = Bindings::new();

        for action in QuickActionAction::iter() {
            let key = match action {
                Close => Key(Char('a')),
                SelectOrRun(_) => AnyChar
            };

            quickaction.insert(key, action.as_default());
        }

        quickaction.insert(Ctrl('c'), Close);
        quickaction.insert(Esc, Close);

        quickaction
    }
}

impl BindingSection for QuickActionAction {
    fn section() -> &'static str {
        "quickaction"
    }

    // statically inserts hardcoded stuff from config like "Up(10)" into action
    fn insert_config_param(self, param: CharOrNum) -> Self {
        use QuickActionAction::*;

        let ch = param.char_or('E');

        match self {
            SelectOrRun(_) => SelectOrRun(ch),
            _ => self
        }
    }

    fn insert_key_param(self, key: Key) -> Self {
        use QuickActionAction::*;
        use Key::*;

        match (self, key) {
            (SelectOrRun(_), Char(ch)) => SelectOrRun(ch),
            (SelectOrRun(_), Ctrl(ch)) => SelectOrRun(ch),
            (SelectOrRun(_), Alt(ch)) => SelectOrRun(ch),
            _ => self
        }
    }
}


#[test]
fn test_keyparse() {
    let keys = ["C-a", "A-_", "Delete", "a", "F9", "C-_"];

    for key in keys.iter() {
        let parsed = key.parse::<AnyKey>();

        dbg!(parsed).ok();
    }
}
