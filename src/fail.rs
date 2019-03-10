use failure;
use failure::Fail;
use failure::Backtrace;

use std::path::PathBuf;

pub type HResult<T> = Result<T, HError>;

#[derive(Fail, Debug)]
pub enum HError {
    #[fail(display = "IO error: {}", error)]
    IoError{#[cause] error: std::io::Error},
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
    #[fail(display = "Failed: {}", error)]
    Error{#[cause] error: failure::Error },
    #[fail(display = "Was None!")]
    NoneError,
    #[fail(display = "Not ready yet!")]
    WillBeNotReady(Backtrace),
    #[fail(display = "No widget found")]
    NoWidgetError(Backtrace),
    #[fail(display = "Path: {:?} not in this directory: {:?}", path, dir)]
    WrongDirectoryError{ path: PathBuf, dir: PathBuf },
    #[fail(display = "Widget finnished")]
    PopupFinnished,
    #[fail(display = "No completions found")]
    NoCompletionsError,
    #[fail(display = "No more history")]
    NoHistoryError,
    #[fail(display = "No core for widget")]
    NoWidgetCoreError(Backtrace),
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
    #[fail(display = "INofify failed: {}", error)]
    INotifyError{#[cause] error: notify::Error},
}

impl HError {
    pub fn quit() -> HResult<()> {
        Err(HError::Quit)
    }
    pub fn wrong_ratio<T>(wnum: usize, ratio: Vec<usize>) -> HResult<T> {
        Err(HError::HBoxWrongRatioError{ wnum: wnum, ratio: ratio })
    }
    pub fn no_widget<T>() -> HResult<T> {
        Err(HError::NoWidgetError(Backtrace::new()))
    }
    pub fn wrong_widget<T>(got: &str, wanted: &str) -> HResult<T> {
        Err(HError::WrongWidgetError{ got: got.to_string(),
                                      wanted: wanted.to_string()})
    }
}

pub trait ErrorLog where Self: Sized {
    fn log(self) {}
}

impl<T> ErrorLog for HResult<T> {
    fn log(self) {
        if let Err(err) = self {
            eprintln!("{:?}", err);
        }
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
        dbg!(&error);
        HError::IoError { error: error }
    }
}

impl From<failure::Error> for HError {
    fn from(error: failure::Error) -> Self {
        dbg!(&error);
        HError::Error { error: error }
    }
}

impl From<std::sync::mpsc::TryRecvError> for HError {
    fn from(error: std::sync::mpsc::TryRecvError) -> Self {
        dbg!(&error);
        HError::ChannelTryRecvError { error: error }
    }
}

impl From<std::sync::mpsc::RecvError> for HError {
    fn from(error: std::sync::mpsc::RecvError) -> Self {
        dbg!(&error);
        HError::ChannelRecvError { error: error }
    }
}

impl<T> From<std::sync::mpsc::SendError<T>> for HError {
    fn from(error: std::sync::mpsc::SendError<T>) -> Self {
        dbg!(&error);
        HError::ChannelSendError
    }
}

impl<T> From<std::sync::PoisonError<T>> for HError {
    fn from(_: std::sync::PoisonError<T>) -> Self {
        dbg!("Poisoned Mutex");
        HError::MutexError
    }
}

impl<T> From<std::sync::TryLockError<T>> for HError {
    fn from(error: std::sync::TryLockError<T>) -> Self {
        dbg!(&error);
        HError::TryLockError
    }
}

impl From<std::option::NoneError> for HError {
    fn from(error: std::option::NoneError) -> Self {
        dbg!(&error);
        HError::NoneError
    }
}

impl From<std::path::StripPrefixError> for HError {
    fn from(error: std::path::StripPrefixError) -> Self {
        dbg!(&error);
        HError::StripPrefixError{error: error}
    }
}

impl From<notify::Error> for HError {
    fn from(error: notify::Error) -> Self {
        dbg!(&error);
        HError::INotifyError{error: error}
    }
}
