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

#[path="clntif-util.rs"]
pub mod clntif_util;

pub mod err;

pub mod clntif {
  pub use super::clntif_codec::Codec as Codec;
  pub use super::clntif_codec::Input as Input;
  pub use blather::Telegram as Telegram;
  pub use blather::Params as Params;
  pub use super::clntif_util::expect_okfail as expect_okfail;
}

pub use err::Error;

// vim: set ft=rust et sw=2 ts=2 sts=2 cinoptions=2 tw=79 :
