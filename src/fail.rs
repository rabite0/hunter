use failure;
use failure::Fail;

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
    WillBeNotReady,
    #[fail(display = "No widget found")]
    NoWidgetError,
    #[fail(display = "Path: {:?} not in this directory: {:?}", path, dir)]
    WrongDirectoryError{ path: PathBuf, dir: PathBuf },
    #[fail(display = "Widget finnished")]
    PopupFinnished,
    #[fail(display = "Input finnished")]
    InputFinnished,
    #[fail(display = "No completions found")]
    NoCompletionsError,
    #[fail(display = "No more history")]
    NoHistoryError
}

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
