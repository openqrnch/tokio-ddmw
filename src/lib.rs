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
//!       Input::Msg(msg) => {
//!         match msg.get_topic() {
//!           Some(t) if t == "Ok" => {
//!             if let Some(name) = msg.get_param("Id") {
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

pub use clntif_codec::Codec as ClntIfCodec;
pub use err::Error;

pub use ezmsg::Msg as ClntIfMsg;

// vim: set ft=rust et sw=2 ts=2 sts=2 cinoptions=2 tw=79 :
