use std::collections::HashSet;

use tokio::io::{AsyncRead, AsyncWrite};

use tokio_util::codec::Framed;

use crate::Error;

pub struct Account {
  pub id: i64,
  pub name: String,
  pub lock: bool,
  pub perms: HashSet<String>
}

pub enum GetAccount {
  Current,
  Id(i64),
  Name(String)
}

/// Get information about an account.
/// The `ga` parameter can be used to query the account by id, name or get
/// information about the connection's current owner.
pub async fn rdacc<T: AsyncRead + AsyncWrite + Unpin>(
  conn: &mut Framed<T, blather::Codec>,
  ga: &GetAccount
) -> Result<Account, Error> {
  let mut tg = blather::Telegram::new_topic("RdAcc")?;

  match ga {
    GetAccount::Id(id) => {
      tg.add_param("Id", id)?;
    }
    GetAccount::Name(nm) => {
      tg.add_str("Name", nm)?;
    }
    _ => {}
  }

  let params = crate::sendrecv(conn, &tg).await?;

  let id = params.get_int::<i64>("Id")?;
  let name = params.get_param::<String>("Name")?;
  let lock = params.get_bool("Lock")?;
  let perms = params.get_hashset("Perms")?;

  let acc = Account {
    id,
    name,
    lock,
    perms
  };

  Ok(acc)
}

// vim: set ft=rust et sw=2 ts=2 sts=2 cinoptions=2 tw=79 :
