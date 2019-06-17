pub trait ExtractResult<T> {
    fn extract(self) -> T;
}

impl<T> ExtractResult<T> for Result<T,T> {
    fn extract(self) -> T {
        match self {
            Ok(val) => val,
            Err(val) => val
        }
    }
}
