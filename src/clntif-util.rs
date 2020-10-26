use tokio::net::TcpStream;
use tokio::stream::StreamExt;

use tokio_util::codec::Framed;

use futures::sink::SinkExt;

use blather::Telegram;

use crate::clntif;
use crate::err::Error;


pub enum Auth {
  AccPass(String, String),
  Token(String)
}

/// Simple helper function for authenticating a connection.
pub async fn authenticate(
  conn: &mut Framed<TcpStream, clntif::Codec>,
  auth: &Auth
) -> Result<blather::Params, Error> {
  let mut tg = Telegram::new_topic("Auth")?;

  match auth {
    Auth::AccPass(acc, pass) => {
      tg.add_param("AccName", acc)?;
      tg.add_param("Pass", pass)?;
    }
    Auth::Token(tkn) => {
      tg.add_param("Tkn", tkn)?;
    }
  }

  conn.send(&tg).await?;

  let params = clntif::expect_okfail(conn).await?;

  Ok(params)
}

/// Waits for a message and ensures that it's Ok or Fail.
/// Converts Fail state to an Error::ServerError.
/// Returns a Params buffer containig the Ok parameters on success.
pub async fn expect_okfail(
  conn: &mut Framed<TcpStream, clntif::Codec>
) -> Result<blather::Params, Error> {
  if let Some(o) = conn.next().await {
    let o = o?;
    match o {
      clntif::Input::Telegram(tg) => {
        if let Some(topic) = tg.get_topic() {
          if topic == "Ok" {
            return Ok(tg.into_params());
          } else if topic == "Fail" {
            return Err(Error::ServerError(tg.into_params()));
          }
        }
      }
      _ => {
        println!("unexpected reply");
      }
    }
    return Err(Error::BadState("Unexpected reply from server.".to_string()));
  }

  // ToDo: Is this an "connection lost" error?

  panic!("Now we know what happened?");
  //Err(Error::UnexpectedState)
}

// vim: set ft=rust et sw=2 ts=2 sts=2 cinoptions=2 tw=79 :
