use std::fs;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

use tokio::io::{AsyncRead, AsyncWrite};

use tokio_util::codec::Framed;

use blather::Telegram;

use crate::utils;
use crate::Error;

/// Used to choose where an authentication token is fetched from.
#[derive(Clone)]
pub enum Token {
  /// Token is stored in a string.
  Buf(String),

  /// Token is stored in a file.
  File(PathBuf)
}

#[derive(Clone)]
pub struct AuthInfo {
  pub accpass: Option<(String, String)>,
  pub itkn: Option<Token>,
  pub otkn: Option<PathBuf>
}

impl AuthInfo {
  pub fn from_accpass(accname: String, pass: String) -> Self {
    AuthInfo {
      accpass: Some((accname, pass)),
      itkn: None,
      otkn: None
    }
  }
}


impl From<&AuthInfo> for AuthInfo {
  fn from(ai: &AuthInfo) -> AuthInfo {
    ai.clone()
  }
}


impl From<ddmw_util::app::Config> for AuthInfo {
  fn from(cfg: ddmw_util::app::Config) -> AuthInfo {
    AuthInfo::from(&cfg)
  }
}


impl From<&ddmw_util::app::Config> for AuthInfo {
  fn from(cfg: &ddmw_util::app::Config) -> AuthInfo {
    if let Some(ref auth) = cfg.auth {
      AuthInfo::from(auth)
    } else {
      AuthInfo {
        accpass: None,
        itkn: None,
        otkn: None
      }
    }
  }
}


impl From<&ddmw_util::app::Auth> for AuthInfo {
  fn from(auth: &ddmw_util::app::Auth) -> AuthInfo {
    // Attempt to get account name and passphrase (from file, if not set
    // explictly).
    let accpass = match &auth.name {
      Some(name) => {
        // Got an account name, see if there's a passphrase.
        if let Some(ref pass) = auth.pass {
          // Got a raw passphrase -- use it
          Some((name.clone(), pass.clone()))
        } else if let Some(ref fname) = auth.pass_file {
          // No raw passphrase, attempt to load from file
          if let Some(pass) = utils::read_single_line(fname) {
            Some((name.to_string(), pass.to_string()))
          } else {
            None
          }
        } else {
          None
        }
      }
      None => None
    };

    // Attempt to get token (if not raw, then a filename to one)
    let itkn = if let Some(ref tkn) = auth.token {
      Some(Token::Buf(tkn.to_string()))
    } else if let Some(ref tknfile) = auth.token_file {
      Some(Token::File(PathBuf::from(tknfile)))
    } else {
      None
    };

    // If a token filename was specified, then use it as the output token
    // filename as well.  This will cause the authenticate() to, if
    // authenticating using username and passphrase, to request an authtoken
    // and save it to this file.
    let otkn = if let Some(Token::File(ref fname)) = itkn {
      Some(fname.to_path_buf())
    } else {
      None
    };

    AuthInfo {
      accpass,
      itkn,
      otkn
    }
  }
}


/// Attempt to authenticate using an authentication token.
/// The token is either loaded from a file or stored in memory as a string.
/// If the caller requested to load a token from a file, but that file can not
/// be read, an error will be returned.
pub async fn token<T: AsyncRead + AsyncWrite + Unpin>(
  conn: &mut Framed<T, blather::Codec>,
  tkn: &Token
) -> Result<(), Error> {
  let buf = match tkn {
    Token::Buf(s) => s.clone(),
    Token::File(fname) => {
      let mut buf = fs::read_to_string(&fname)?;
      buf.truncate(32);
      buf
    }
  };
  let mut tg = Telegram::new_topic("Auth")?;
  tg.add_param("Tkn", buf)?;
  crate::sendrecv(conn, &tg).await?;
  Ok(())
}


/// Attempt to authenticate using an account name and a passphrase.
/// Optionally request an authentication token if the authentication was
/// successful.
pub async fn accpass<T: AsyncRead + AsyncWrite + Unpin>(
  conn: &mut Framed<T, blather::Codec>,
  accname: &String,
  pass: &String,
  reqtkn: bool
) -> Result<Option<String>, Error> {
  let mut tg = Telegram::new_topic("Auth")?;
  tg.add_param("AccName", accname)?;
  tg.add_param("Pass", pass)?;
  if reqtkn {
    tg.add_param("ReqTkn", "True")?;
  }
  let params = crate::sendrecv(conn, &tg).await?;

  if reqtkn {
    let s = params.get_str("Tkn");
    if let Some(s) = s {
      Ok(Some(s.to_string()))
    } else {
      Ok(None)
    }
  } else {
    Ok(None)
  }
}


/// Helper function for authenticating a connection.
///
/// 1. Attempt to authenticate using token, if one was supplied (either by
///    buffer or filename).
/// 2. If token authentication failed, and account name and passphrase was
///    supplied, then attempt to authenticate with the account name and
///    passphrase.
/// 3. If an output token file name was supplied, then save the returned
///    authentication to that file.
pub async fn authenticate<T: AsyncRead + AsyncWrite + Unpin>(
  conn: &mut Framed<T, blather::Codec>,
  ai: &AuthInfo
) -> Result<Option<String>, Error> {
  //
  // If an input token was specified, then try to authenticate with it.
  //
  if let Some(tkn) = &ai.itkn {
    let do_tknauth = match tkn {
      Token::File(fname) => {
        // If the caller has requested to load an authentication token from a
        // file, then check if the file exists.
        // If it doesn't, then continue regardless (but don't actually try to
        // authenticate using a token), because it may be possible to fall
        // back to password authentication.
        if fname.exists() {
          // File exists -- go ahead and try to use it
          true
        } else {
          // File doesn't exist -- don't even attempt to use it
          false
        }
      }
      Token::Buf(_) => {
        // It's a plain token buffer.
        // Don't validate here; let the call to the server do it
        true
      }
    };

    if do_tknauth {
      match token(conn, &tkn).await {
        Ok(_) => {
          // Everything went ok, and since it was a token authentication
          // there's no token to return.
          return Ok(None);
        }
        Err(e) => {
          match e {
            Error::ServerError(_) => {
              // Ignore server errors, because it may just mean that the token
              // is outdated.
              // Could be more granular about the errors here.
            }
            _ => {
              // Return any error that isn't a server error.
              return Err(e);
            }
          }
        }
      }
    }
  }


  //
  // Either no token authentication was attempted, or it failed in a manner
  // which suggests that a password authentication is an acceptable fallback.
  //
  if let Some((acc, pass)) = &ai.accpass {
    let reqtkn = if let Some(_otkn) = &ai.otkn {
      true
    } else {
      false
    };

    let tkn = accpass(conn, &acc, &pass, reqtkn).await;
    if let Ok(tkn) = &tkn {
      if let Some(tkn) = tkn {
        if let Some(fname) = &ai.otkn {
          let mut f = File::create(fname)?;
          f.write(tkn.as_bytes())?;
        }
      }
    }
    return tkn;
  }


  // Token authetication failed and no account name/password was passed, so
  // error out.
  return Err(Error::InvalidCredentials);
}


/// Return ownership of a connection to the built-in _unauthenticated_ account.
pub async fn unauthenticate<T: AsyncRead + AsyncWrite + Unpin>(
  conn: &mut Framed<T, blather::Codec>
) -> Result<(), Error> {
  let tg = Telegram::new_topic("Unauth")?;

  crate::sendrecv(conn, &tg).await?;

  Ok(())
}


// vim: set ft=rust et sw=2 ts=2 sts=2 cinoptions=2 tw=79 :
