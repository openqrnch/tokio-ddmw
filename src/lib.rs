//! Utility library for creating integrations against the Data Diode
//! Middleware.
//!
//! ```compile_fail
//! use tokio_ddmw::ClntIfCodec;
//!
//! async fn conn_handler(
//!   socket: TcpStream
//! ) -> Result<(), Error> {
//!   let mut conn = Framed::new(socket, ClntIfCodec::new());
//!   while let Some(o) = conn.next().await {
//!     let o = o?;
//!     match o {
//!       Input::Telegram(tg) => {
//!         match tg.get_topic() {
//!           Some(t) if t == "Ok" => {
//!             if let Some(name) = tg.get_param("Id") {
//!               // ...
//!             }
//!             Some(t) if t == "Fail" => {
//!               // ...
//!             }
//!           }
//!           _ => {
//!             // ...
//!           }
//!         }
//!         Input::Bin(d, remain) => {
//!           // ...
//!         }
//!       }
//!     }
//!   }
//! }
//! ```



#[path="clntif-codec.rs"]
pub mod clntif_codec;
pub mod err;

pub mod clntif {
  pub use super::clntif_codec::Codec as Codec;
  pub use super::clntif_codec::Input as Input;
  pub use blather::Telegram as Telegram;
  pub use blather::Params as Params;
}

pub use err::Error;


use tokio::net::TcpStream;
use tokio_util::codec::Framed;
use tokio::stream::StreamExt;

/// Waits for a message and ensures that it's Ok or Fail.
/// Converts Fail state to an Error::ServerError.
/// Returns a Params buffer containig the Ok parameters on success.
pub async fn expect_result(
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
