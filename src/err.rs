use std::fmt;

use tokio::io;

use blather::Params;

#[derive(Debug)]
pub enum Error {
  Blather(String),
  IO(String),
  BadFormat(String),
  SerializeError(String),
  ServerError(Params),
  BadState(String),
  InvalidSize(String),
  InvalidCredentials,
  Disconnected
}

impl std::error::Error for Error {}

impl fmt::Display for Error {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    match &*self {
      Error::Blather(s) => write!(f, "Msg buffer error; {}", s),
      Error::IO(s) => write!(f, "I/O error; {}", s),
      Error::BadFormat(s) => write!(f, "Bad format; {}", s),
      Error::SerializeError(s) => write!(f, "Unable to serialize; {}", s),
      Error::ServerError(p) => write!(f, "Server replied: {}", p),
      Error::BadState(s) => {
        write!(f, "Encountred an unexpected/bad state: {}", s)
      }
      Error::InvalidSize(s) => write!(f, "Invalid size; {}", s),
      Error::InvalidCredentials => write!(f, "Invalid credentials"),
      Error::Disconnected => write!(f, "Disconnected")
    }
  }
}

impl From<io::Error> for Error {
  fn from(err: io::Error) -> Self {
    Error::IO(err.to_string())
  }
}

impl From<blather::Error> for Error {
  fn from(err: blather::Error) -> Self {
    Error::Blather(err.to_string())
  }
}

// vim: set ft=rust et sw=2 ts=2 sts=2 cinoptions=2 tw=79 :
