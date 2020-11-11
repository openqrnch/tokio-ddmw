//! Utility library for creating integrations against the Data Diode
//! Middleware.

pub mod auth;
pub mod err;

use futures::sink::SinkExt;

use tokio::io::{AsyncRead, AsyncWrite};
use tokio::stream::StreamExt;

use tokio_util::codec::Framed;

use blather::{codec, Telegram};

pub use err::Error;


/// Send a telegram and wait for a reply.
pub async fn sendrecv<T: AsyncRead + AsyncWrite + Unpin>(
  conn: &mut Framed<T, blather::Codec>,
  tg: &Telegram
) -> Result<blather::Params, Error> {
  conn.send(tg).await?;
  crate::expect_okfail(conn).await
}


/// Waits for a message and ensures that it's Ok or Fail.
/// Converts Fail state to an Error::ServerError.
/// Returns a Params buffer containig the Ok parameters on success.
pub async fn expect_okfail<T: AsyncRead + AsyncWrite + Unpin>(
  conn: &mut Framed<T, blather::Codec>
) -> Result<blather::Params, Error> {
  if let Some(o) = conn.next().await {
    let o = o?;
    match o {
      codec::Input::Telegram(tg) => {
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

  Err(Error::Disconnected)
}


// vim: set ft=rust et sw=2 ts=2 sts=2 cinoptions=2 tw=79 :
