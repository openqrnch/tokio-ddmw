use tokio::net::TcpStream;
use tokio_util::codec::Framed;
use tokio::stream::StreamExt;

use crate::clntif;
use crate::err::Error;

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
    return Err(Error::BadState(
      "Unexpected reply from server.".to_string(),
    ));
  }

  // ToDo: Is this an "connection lost" error?

  panic!("Now we know what happened?");
  //Err(Error::UnexpectedState)
}

// vim: set ft=rust et sw=2 ts=2 sts=2 cinoptions=2 tw=79 :
