#[derive(Debug)]
pub enum Error {
    Clauser(clauser::error::Error),
    IO(std::io::Error),
    Config(serde_json::Error),
    Provider(String),
    Other(String),
}

impl From<clauser::error::Error> for Error {
    fn from(value: clauser::error::Error) -> Self {
        Error::Clauser(value)
    }
}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Error::IO(value)
    }
}

impl From<serde_json::Error> for Error {
    fn from(value: serde_json::Error) -> Self {
        Error::Config(value)
    }
}
