use failure;

pub type HResult<T> = Result<T, HError>;

#[derive(Fail, Debug)]
pub enum HError {
    #[fail(display = "IO error: {}", error)]
    IoError{#[cause] error: std::io::Error},
    #[fail(display = "Mutex failed")]
    MutexError,
    #[fail(display = "Channel failed: {}", error)]
    ChannelTryRecvError{#[cause] error: std::sync::mpsc::TryRecvError},
    #[fail(display = "Previewer failed on file: {}", file)]
    PreviewFailed{file: String},
    #[fail(display = "StalePreviewer for file: {}", file)]
    StalePreviewError{file: String},
    #[fail(display = "Failed: {}", error)]
    Error{#[cause] error: failure::Error }
}

impl From<std::io::Error> for HError {
    fn from(error: std::io::Error) -> Self {
        HError::IoError { error: error }
    }
}

impl From<failure::Error> for HError {
    fn from(error: failure::Error) -> Self {
        HError::Error { error: error }
    }
}

impl From<std::sync::mpsc::TryRecvError> for HError {
    fn from(error: std::sync::mpsc::TryRecvError) -> Self {
        HError::ChannelTryRecvError { error: error }
    }
}

impl<T> From<std::sync::PoisonError<T>> for HError {
    fn from(_: std::sync::PoisonError<T>) -> Self {
        HError::MutexError
    }
}
