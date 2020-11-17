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


#[derive(Debug)]
pub struct DDLinkInfo {
  engine: String,
  protocol: ddmw_types::node::ddlnk::Protocol,
  protimpl: ddmw_types::node::ddlnk::ProtImpl
}


#[derive(Debug)]
pub struct NodeInfo {
  nodetype: ddmw_types::node::Type,
  ddlnk: DDLinkInfo
}


pub async fn get_nodeinfo<T: AsyncRead + AsyncWrite + Unpin>(
  conn: &mut Framed<T, blather::Codec>
) -> Result<NodeInfo, Error> {
  let mut tg = Telegram::new();
  tg.set_topic("GetNodeInfo")?;
  let params = sendrecv(conn, &tg).await?;

  let nodetype = match params.get_str("ddmw.node") {
    Some(s) => s.parse::<ddmw_types::node::Type>(),
    None => return Err(Error::MissingData("ddmw.node not found".to_string()))
  };
  let nodetype = match nodetype {
    Ok(nt) => nt,
    Err(_) => return Err(Error::UnknownData("Unknown node type".to_string()))
  };

  let engine = match params.get_str("ddmw.ddlink.engine") {
    Some(s) => s.to_string(),
    None => {
      return Err(Error::MissingData(
        "ddmw.ddlink.engine not found".to_string()
      ))
    }
  };
  let protocol = match params.get_str("ddmw.ddlink.protocol") {
    Some(s) => s.parse::<ddmw_types::node::ddlnk::Protocol>(),
    None => {
      return Err(Error::MissingData(
        "ddmw.ddlnk.protocol not found".to_string()
      ))
    }
  };
  let protocol = match protocol {
    Ok(s) => s,
    Err(_) => {
      return Err(Error::UnknownData("Unknown protocol type".to_string()))
    }
  };
  let protimpl = match params.get_str("ddmw.ddlink.protimpl") {
    Some(s) => s.parse::<ddmw_types::node::ddlnk::ProtImpl>(),
    None => {
      return Err(Error::MissingData(
        "ddmw.ddlnk.protimpl not found".to_string()
      ))
    }
  };
  let protimpl = match protimpl {
    Ok(s) => s,
    Err(_) => {
      return Err(Error::UnknownData("Unknown protimpl type".to_string()))
    }
  };

  Ok(NodeInfo {
    nodetype,
    ddlnk: DDLinkInfo {
      engine,
      protocol,
      protimpl
    }
  })
}

// vim: set ft=rust et sw=2 ts=2 sts=2 cinoptions=2 tw=79 :
