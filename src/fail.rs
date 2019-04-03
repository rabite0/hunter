use failure;
use failure::Fail;
use failure::Backtrace;

use termion::event::Key;

use std::path::PathBuf;
use std::sync::Mutex;

use crate::foldview::LogEntry;

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
    #[fail(display = "Not ready yet!")]
    WillBeNotReady,
    #[fail(display = "Not ready yet!")]
    AsyncNotReadyError,
    #[fail(display = "Async is stale!")]
    AsyncStaleError,
    #[fail(display = "Value has already been taken!")]
    AsyncAlreadyTakenError,
    #[fail(display = "Async has already been started!")]
    AsyncAlreadyStartedError,
    #[fail(display = "Async Error: {}", _0)]
    AsyncError(String),
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
}

impl HError {
    pub fn log<T>(log: String) -> HResult<T> {
        Err(HError::Log(log))
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

    pub fn async_not_ready<T>() -> HResult<T> {
        Err(HError::AsyncNotReadyError)
    }

    pub fn async_taken<T>() -> HResult<T> {
        Err(HError::AsyncAlreadyTakenError)
    }

    pub fn async_error<T>(error: &HError) -> HResult<T> {
        Err(HError::AsyncError(format!("{}", error)))
    }

    pub fn async_started<T>() -> HResult<T> {
        Err(HError::AsyncAlreadyStartedError)
    }

    pub fn metadata_processed<T>() -> HResult<T> {
        Err(HError::MetadataProcessedError)
    }

    pub fn no_files<T>() -> HResult<T> {
        Err(HError::WidgetNoFilesError)
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

impl<T> ErrorLog for HResult<T> {
    fn log(self) {
        if let Err(err) = self {
            // eprintln!("{:?}", err);
            put_log(&err).ok();
        }
    }

    fn log_and(self) -> Self {
        if let Err(err) = &self {
            put_log(err).ok();
        }
        self
    }
}




// impl From<&HError> for HError {
//     fn from(error: &HError) -> Self {
//         dbg!(&error);
//         (error.clone())
//     }
// }

impl From<std::io::Error> for HError {
    fn from(error: std::io::Error) -> Self {
        // dbg!(&error);
        let err = HError::IoError(format!("{}", error));
        put_log(&err).ok();
        err
    }
}

impl From<failure::Error> for HError {
    fn from(error: failure::Error) -> Self {
        // dbg!(&error);
        let err = HError::Error(format!("{}", error));
        put_log(&err).ok();
        err
    }
}

impl From<std::sync::mpsc::TryRecvError> for HError {
    fn from(error: std::sync::mpsc::TryRecvError) -> Self {
        // dbg!(&error);
        let err = HError::ChannelTryRecvError { error: error };
        put_log(&err).ok();
        err
    }
}

impl From<std::sync::mpsc::RecvError> for HError {
    fn from(error: std::sync::mpsc::RecvError) -> Self {
        // dbg!(&error);
        let err = HError::ChannelRecvError { error: error };
        put_log(&err).ok();
        err
    }
}

impl<T> From<std::sync::mpsc::SendError<T>> for HError {
    fn from(error: std::sync::mpsc::SendError<T>) -> Self {
        dbg!(&error);
        let err = HError::ChannelSendError;
        put_log(&err).ok();
        err
    }
}

impl<T> From<std::sync::PoisonError<T>> for HError {
    fn from(_: std::sync::PoisonError<T>) -> Self {
        // dbg!("Poisoned Mutex");
        let err = HError::MutexError;
        put_log(&err).ok();
        err
    }
}

impl<T> From<std::sync::TryLockError<T>> for HError {
    fn from(error: std::sync::TryLockError<T>) -> Self {
        // dbg!(&error);
        let err = HError::TryLockError;
        put_log(&err).ok();
        err
    }
}

impl From<std::option::NoneError> for HError {
    fn from(error: std::option::NoneError) -> Self {
        //dbg!(&error);
        let err = HError::NoneError;
        //put_log(&err).ok();
        err
    }
}

impl From<std::path::StripPrefixError> for HError {
    fn from(error: std::path::StripPrefixError) -> Self {
        // dbg!(&error);
        let err = HError::StripPrefixError{error: error };
        put_log(&err).ok();
        err
    }
}

impl From<notify::Error> for HError {
    fn from(error: notify::Error) -> Self {
        // dbg!(&error);
        let err = HError::INotifyError(format!("{}", error));
        put_log(&err).ok();
        err
    }
}
