use std::error::Error;

pub type BoxError = Box<dyn Error + Send + Sync + 'static>;
pub type Result<T> = std::result::Result<T, BoxError>;

pub fn err(message: impl Into<String>) -> BoxError {
    message.into().into()
}
