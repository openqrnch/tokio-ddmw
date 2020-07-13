#[path="clntif-codec.rs"]
mod clntif_codec;
mod err;

pub use clntif_codec::Codec as ClntIfCodec;
pub use err::Error;

// vim: set ft=rust et sw=2 ts=2 sts=2 cinoptions=2 tw=79 :
