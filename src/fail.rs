use failure;
use failure::Fail;
//use failure::Backtrace;
//use async_value::AError;


use termion::event::Key;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::foldview::LogEntry;
use crate::mediaview::MediaError;

pub type HResult<T> = Result<T, HError>;



#[derive(Fail, Debug, Clone)]
pub enum HError {
    #[fail(display = "IO error: {} ", _0)]
    IoError(String),
    #[fail(display = "Mutex failed")]
    MutexError,
    #[fail(display = "Can't lock!")]
    TryLockError,
    #[fail(display = "Channel failed: {}", error)]
    ChannelTryRecvError{#[cause] error: std::sync::mpsc::TryRecvError},
    #[fail(display = "Channel failed: {}", error)]
    ChannelRecvError{#[cause] error: std::sync::mpsc::RecvError},
    #[fail(display = "Channel failed")]
    ChannelSendError,
    #[fail(display = "Timer ran out while waiting for message on channel!")]
    ChannelRecvTimeout(#[cause] std::sync::mpsc::RecvTimeoutError),
    #[fail(display = "Previewer failed on file: {}", file)]
    PreviewFailed{file: String},
    #[fail(display = "StalePreviewer for file: {}", file)]
    StalePreviewError{file: String},
    #[fail(display = "Accessed stale value")]
    StaleError,
    #[fail(display = "Failed: {}", _0)]
    Error(String),
    #[fail(display = "Was None!")]
    NoneError,
    #[fail(display = "Async Error: {}", _0)]
    AError(async_value::AError),
    #[fail(display = "No widget found")]
    NoWidgetError,
    #[fail(display = "Path: {:?} not in this directory: {:?}", path, dir)]
    WrongDirectoryError{ path: PathBuf, dir: PathBuf},
    #[fail(display = "Widget finnished")]
    PopupFinnished,
    #[fail(display = "No completions found")]
    NoCompletionsError,
    #[fail(display = "No more history")]
    NoHistoryError,
    #[fail(display = "No core for widget")]
    NoWidgetCoreError,
    #[fail(display = "No header for widget")]
    NoHeaderError,
    #[fail(display = "You wanted this!")]
    Quit,
    #[fail(display = "HBox ratio mismatch: {} widgets, ratio is {:?}", wnum, ratio)]
    HBoxWrongRatioError{ wnum: usize, ratio: Vec<usize> },
    #[fail(display = "Got wrong widget: {}! Wanted: {}", got, wanted)]
    WrongWidgetError{got: String, wanted: String},
    #[fail(display = "Strip Prefix Error: {}", error)]
    StripPrefixError{#[cause] error: std::path::StripPrefixError},
    #[fail(display = "INofify failed: {}", _0)]
    INotifyError(String),
    #[fail(display = "Tags not loaded yet")]
    TagsNotLoadedYetError,
    #[fail(display = "Input cancelled!")]
    MiniBufferCancelledInput,
    #[fail(display = "Empty input!")]
    MiniBufferEmptyInput,
    #[fail(display = "Undefined key: {:?}", key)]
    WidgetUndefinedKeyError{key: Key},
    #[fail(display = "Terminal has been resized!")]
    TerminalResizedError,
    #[fail(display = "Widget has been resized!")]
    WidgetResizedError,
    #[fail(display = "{}", _0)]
    Log(String),
    #[fail(display = "Metadata already processed")]
    MetadataProcessedError,
    #[fail(display = "No files to take from widget")]
    WidgetNoFilesError,
    #[fail(display = "Invalid line in settings file: {}", _0)]
    ConfigLineError(String),
    #[fail(display = "New input in Minibuffer")]
    MiniBufferInputUpdated(String),
    #[fail(display = "Failed to parse into UTF8")]
    UTF8ParseError(std::str::Utf8Error),
    #[fail(display = "Failed to parse integer!")]
    ParseIntError(std::num::ParseIntError),
    #[fail(display = "Failed to parse char!")]
    ParseCharError(std::char::ParseCharError),
    #[fail(display = "{}", _0)]
    Media(MediaError),
    #[fail(display = "{}", _0)]
    Mime(MimeError),
    #[fail(display = "{}", _0)]
    KeyBind(KeyBindError),
    #[fail(display = "FileBrowser needs to know about all tab's files to run exec!")]
    FileBrowserNeedTabFiles,
    #[fail(display = "{}", _0)]
    FileError(crate::files::FileError),
    #[fail(display = "{}", _0)]
    Nix(#[cause] nix::Error)
}

impl HError {
    pub fn log<T>(log: &str) -> HResult<T> {
        Err(HError::Log(String::from(log))).log_and()
    }
    pub fn quit() -> HResult<()> {
        Err(HError::Quit)
    }
    pub fn wrong_ratio<T>(wnum: usize, ratio: Vec<usize>) -> HResult<T> {
        Err(HError::HBoxWrongRatioError{ wnum: wnum, ratio: ratio })
    }
    pub fn no_widget<T>() -> HResult<T> {
        Err(HError::NoWidgetError)
    }
    pub fn wrong_widget<T>(got: &str, wanted: &str) -> HResult<T> {
        Err(HError::WrongWidgetError{ got: got.to_string(),
                                      wanted: wanted.to_string() })

    }
    pub fn popup_finnished<T>() -> HResult<T> {
        Err(HError::PopupFinnished)
    }
    pub fn tags_not_loaded<T>() -> HResult<T> {
        Err(HError::TagsNotLoadedYetError)
    }
    pub fn minibuffer_cancel<T>() -> HResult<T> {
        Err(HError::MiniBufferCancelledInput)
    }
    pub fn minibuffer_empty<T>() -> HResult<T> {
        Err(HError::MiniBufferEmptyInput)
    }
    pub fn undefined_key<T>(key: Key) -> HResult<T> {
        Err(HError::WidgetUndefinedKeyError { key: key })
    }
    pub fn wrong_directory<T>(path: PathBuf, dir: PathBuf) -> HResult<T> {
        Err(HError::WrongDirectoryError{ path: path,
                                         dir: dir })

    }
    pub fn preview_failed<T>(file: &crate::files::File) -> HResult<T> {
        let name = file.name.clone();
        Err(HError::PreviewFailed{ file: name })

    }

    pub fn terminal_resized<T>() -> HResult<T> {
        Err(HError::TerminalResizedError)
    }

    pub fn widget_resized<T>() -> HResult<T> {
        Err(HError::WidgetResizedError)
    }

    pub fn stale<T>() -> HResult<T> {
        Err(HError::StaleError)
    }

    pub fn config_error<T>(line: String) -> HResult<T> {
        Err(HError::ConfigLineError(line))
    }

    pub fn metadata_processed<T>() -> HResult<T> {
        Err(HError::MetadataProcessedError)
    }

    pub fn no_files<T>() -> HResult<T> {
        Err(HError::WidgetNoFilesError)
    }

    pub fn input_updated<T>(input: String) -> HResult<T> {
        Err(HError::MiniBufferInputUpdated(input))
    }


}


lazy_static! {
    static ref LOG: Mutex<Vec<LogEntry>> = Mutex::new(vec![]);
}

pub fn get_logs() -> HResult<Vec<LogEntry>> {
    let logs = LOG.lock()?.drain(..).collect();
    Ok(logs)
}

pub fn put_log<L: Into<LogEntry>>(log: L) -> HResult<()> {
    LOG.lock()?.push(log.into());
    Ok(())
}

pub trait ErrorLog where Self: Sized {
    fn log(self);
    fn log_and(self) -> Self;
}

// impl<T> ErrorLog for HResult<T> {
//     fn log(self) {
//         if let Err(err) = self {
//             put_log(&err).ok();
//         }
//     }

//     fn log_and(self) -> Self {
//         if let Err(err) = &self {
//             put_log(err).ok();
//         }
//         self
//     }
// }


// impl<T> ErrorLog for Result<T, AError> {
//     fn log(self) {
//         if let Err(err) = self {
//             put_log(&err.into()).ok();
//         }
//     }

//     fn log_and(self) -> Self {
//         if let Err(err) = &self {
//             put_log(&err.clone().into()).ok();
//         }
//         self
//     }
// }

impl<T, E> ErrorLog for Result<T, E>
where E: Into<HError> + Clone {
    fn log(self) {
        if let Err(err) = self {
            let err: HError = err.into();
            put_log(&err).ok();
        }
    }
    fn log_and(self) -> Self {
        if let Err(ref err) = self {
            let err: HError = err.clone().into();
            put_log(&err).ok();
        }
        self
    }
}

impl<E> ErrorLog for E
where E: Into<HError> + Clone {
    fn log(self) {
        let err: HError = self.into();
        put_log(&err).ok();

    }
    fn log_and(self) -> Self {
        let err: HError = self.clone().into();
        put_log(&err).ok();
        self
    }
}




impl From<std::io::Error> for HError {
    fn from(error: std::io::Error) -> Self {
        let err = HError::IoError(format!("{}", error));
        err
    }
}

impl From<failure::Error> for HError {
    fn from(error: failure::Error) -> Self {
        let err = HError::Error(format!("{}", error));
        err
    }
}

impl From<std::sync::mpsc::TryRecvError> for HError {
    fn from(error: std::sync::mpsc::TryRecvError) -> Self {
        let err = HError::ChannelTryRecvError { error: error };
        err
    }
}

impl From<std::sync::mpsc::RecvError> for HError {
    fn from(error: std::sync::mpsc::RecvError) -> Self {
        let err = HError::ChannelRecvError { error: error };
        err
    }
}

impl From<std::sync::mpsc::RecvTimeoutError> for HError {
    fn from(error: std::sync::mpsc::RecvTimeoutError) -> Self {
        let err = HError::ChannelRecvTimeout(error);
        err
    }
}

impl<T> From<std::sync::mpsc::SendError<T>> for HError {
    fn from(_error: std::sync::mpsc::SendError<T>) -> Self {
        let err = HError::ChannelSendError;
        err
    }
}

impl<T> From<std::sync::PoisonError<T>> for HError {
    fn from(_: std::sync::PoisonError<T>) -> Self {
        let err = HError::MutexError;
        err
    }
}

impl<T> From<std::sync::TryLockError<T>> for HError {
    fn from(_error: std::sync::TryLockError<T>) -> Self {
        let err = HError::TryLockError;
        err
    }
}

impl From<std::option::NoneError> for HError {
    fn from(_error: std::option::NoneError) -> Self {
        let err = HError::NoneError;
        err
    }
}

impl From<std::path::StripPrefixError> for HError {
    fn from(error: std::path::StripPrefixError) -> Self {
        let err = HError::StripPrefixError{error: error };
        err
    }
}

impl From<notify::Error> for HError {
    fn from(error: notify::Error) -> Self {
        let err = HError::INotifyError(format!("{}", error));
        err
    }
}

impl From<async_value::AError> for HError {
    fn from(error: async_value::AError) -> Self {
        let err = HError::AError(error);
        err
    }
}

impl From<std::str::Utf8Error> for HError {
    fn from(error: std::str::Utf8Error) -> Self {
        let err = HError::UTF8ParseError(error);
        err
    }
}


impl From<std::num::ParseIntError> for HError {
    fn from(error: std::num::ParseIntError) -> Self {
        let err = HError::ParseIntError(error);
        err
    }
}

impl From<nix::Error> for HError {
    fn from(error: nix::Error) -> Self {
        let err = HError::Nix(error);
        err
    }
}

impl From<std::char::ParseCharError> for HError {
    fn from(error: std::char::ParseCharError) -> Self {
        let err = HError::ParseCharError(error);
        err
    }
}


// MIME Errors

#[derive(Fail, Debug, Clone)]
pub enum MimeError {
    #[fail(display = "Need a file to determine MIME type")]
    NoFileProvided,
    #[fail(display = "File access failed! Error: {}", _0)]
    AccessFailed(Box<HError>),
    #[fail(display = "No MIME type found for this file",)]
    NoMimeFound,
    #[fail(display = "Paniced while trying to find MIME type for: {}!", _0)]
    Panic(String),
}

impl From<MimeError> for HError {
    fn from(e: MimeError) -> Self {
        HError::Mime(e)
    }
}


impl From<KeyBindError> for HError {
    fn from(e: KeyBindError) -> Self {
        HError::KeyBind(e)
    }
}


#[derive(Fail, Debug, Clone)]
pub enum KeyBindError {
    #[fail(display = "Movement has not been defined for this widget")]
    MovementUndefined,
    #[fail(display = "Keybind defined with wrong key: {} -> {}", _0, _1)]
    WrongKey(String, String),
    #[fail(display = "Defined keybind for non-existing action: {}", _0)]
    WrongAction(String),
    #[fail(display = "Failed to parse keybind: {}", _0)]
    ParseKeyError(String),
    #[fail(display = "Trouble with ini file! Error: {}", _0)]
    IniError(Arc<ini::ini::Error>),
    #[fail(display = "Couldn't parse as either char or u8: {}", _0)]
    CharOrNumParseError(String),
    #[fail(display = "Wanted {}, but got {}!", _0, _1)]
    CharOrNumWrongType(String, String)

}

impl From<ini::ini::Error> for KeyBindError {
    fn from(err: ini::ini::Error) -> Self {
        KeyBindError::IniError(Arc::new(err))
    }
}

impl From<crate::files::FileError> for HError {
    fn from(err: crate::files::FileError) -> Self {
        HError::FileError(err)
    }
}
