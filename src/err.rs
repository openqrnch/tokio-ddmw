use std::fmt;

use tokio::io;

#[derive(Debug)]
pub enum Error {
  Msg(String),
  IO(String),
  BadFormat(String),
  SerializeError(String)
}

impl std::error::Error for Error { }

impl fmt::Display for Error {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    match &*self {
      Error::Msg(s) => {
        write!(f, "Msg buffer error; {}", s)
      }
      Error::IO(s) => {
        write!(f, "I/O error; {}", s)
      }
      Error::BadFormat(s) => {
        write!(f, "Bad format; {}", s)
      }
      Error::SerializeError(s) => {
        write!(f, "Unable to serialize; {}", s)
      }
    }
  }
}

impl From<io::Error> for Error {
  fn from(err: io::Error) -> Self {
    Error::IO(err.to_string())
  }
}

impl From<ezmsg::Error> for Error {
  fn from(err: ezmsg::Error) -> Self {
    Error::Msg(err.to_string())
  }
}

// vim: set ft=rust et sw=2 ts=2 sts=2 cinoptions=2 tw=79 :
