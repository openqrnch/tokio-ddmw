use std::fs;
use std::path::PathBuf;

use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;

#[cfg(unix)]
use tokio::net::UnixStream;

use tokio_util::codec::Framed;

use futures::sink::SinkExt;

use bytes::Bytes;

use blather::{Params, Telegram};

use crate::err::Error;


pub enum InputType {
  Params(Params),
  File(PathBuf),
  VecBuf(Vec<u8>),
  Bytes(Bytes)
}

pub enum Endpoint {
  TcpSockAddr(String),

  #[cfg(unix)]
  UdsPath(PathBuf)
}

pub struct ConnTransport {
  pub msgif: Endpoint,
  pub authinfo: Option<crate::auth::AuthInfo>,
  pub ch: u8
}

pub struct Transport {
  pub ch: u8
}

pub struct MsgInfo {
  pub cmd: u32,
  pub meta: Option<InputType>,
  pub payload: Option<InputType>
}


/// Connect, optionally authenticate, send message and disconnect
pub async fn connsend(
  xfer: ConnTransport,
  mi: &MsgInfo
) -> Result<String, Error> {
  match xfer.msgif {
    Endpoint::TcpSockAddr(sa) => {
      let stream = TcpStream::connect(sa).await?;
      let mut framed = Framed::new(stream, blather::Codec::new());
      if let Some(ref authinfo) = xfer.authinfo {
        let _ = crate::auth::authenticate(&mut framed, authinfo).await?;
      }
      send(&mut framed, &Transport { ch: xfer.ch }, mi).await
    }
    #[cfg(unix)]
    Endpoint::UdsPath(sa) => {
      let stream = UnixStream::connect(sa).await?;
      let mut framed = Framed::new(stream, blather::Codec::new());
      if let Some(ref authinfo) = xfer.authinfo {
        let _ = crate::auth::authenticate(&mut framed, authinfo).await?;
      }
      send(&mut framed, &Transport { ch: xfer.ch }, mi).await
    }
  }
}


/// Send a message, including (if applicable) its metadata and payload.
///
/// On successful completion returns the transfer identifier.
pub async fn send<T: AsyncRead + AsyncWrite + Unpin>(
  conn: &mut Framed<T, blather::Codec>,
  xfer: &Transport,
  mi: &MsgInfo
) -> Result<String, Error> {
  let metalen = get_meta_size(&mi)?;
  let payloadlen = get_payload_size(&mi)?;

  let mut tg = Telegram::new_topic("Msg")?;
  tg.add_param("_Ch", xfer.ch)?;
  if mi.cmd != 0 {
    tg.add_param("Cmd", mi.cmd)?;
  }
  if metalen != 0 {
    tg.add_param("MetaLen", metalen)?;
  }
  if payloadlen != 0 {
    tg.add_param("Len", payloadlen)?;
  }
  let params = crate::sendrecv(conn, &tg).await?;

  // Extract the transfer identifier assigned to this message
  let xferid = match params.get_str("XferId") {
    Some(xferid) => xferid.to_string(),
    None => {
      let e = "Missing expected transfer identifier";
      return Err(Error::MissingData(String::from(e)));
    }
  };

  if let Some(meta) = &mi.meta {
    send_content(conn, meta).await?;
    crate::expect_okfail(conn).await?;
  }

  if let Some(payload) = &mi.payload {
    send_content(conn, payload).await?;
    crate::expect_okfail(conn).await?;
  }

  Ok(xferid)
}


fn get_meta_size(mi: &MsgInfo) -> Result<u32, Error> {
  let sz = match &mi.meta {
    Some(meta) => match meta {
      InputType::Params(params) => params.calc_buf_size(),
      InputType::File(f) => {
        let metadata = fs::metadata(&f)?;
        metadata.len() as usize
      }
      InputType::VecBuf(v) => v.len(),
      InputType::Bytes(b) => b.len()
    },
    None => 0
  };

  if sz > u32::MAX as usize {
    // ToDo: Return out of bounds error
  }

  Ok(sz as u32)
}


fn get_payload_size(mi: &MsgInfo) -> Result<u64, Error> {
  let sz = match &mi.payload {
    Some(payload) => match payload {
      InputType::Params(params) => params.calc_buf_size(),
      InputType::File(f) => {
        let metadata = fs::metadata(&f)?;
        metadata.len() as usize
      }
      InputType::VecBuf(v) => v.len(),
      InputType::Bytes(b) => b.len()
    },
    None => 0
  };

  Ok(sz as u64)
}


async fn send_content<T>(
  conn: &mut Framed<T, blather::Codec>,
  data: &InputType
) -> Result<(), Error>
where
  T: AsyncRead + AsyncWrite + Unpin
{
  match data {
    InputType::Params(params) => Ok(conn.send(params).await?),
    InputType::File(fname) => {
      let mut f = tokio::fs::File::open(fname).await?;
      let _ = tokio::io::copy(&mut f, conn.get_mut()).await?;
      Ok(())
    }
    InputType::VecBuf(v) => Ok(conn.send(v.as_slice()).await?),
    InputType::Bytes(b) => Ok(conn.send(b.as_ref()).await?)
  }
}

// vim: set ft=rust et sw=2 ts=2 sts=2 cinoptions=2 tw=79 :
