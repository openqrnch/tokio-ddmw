use std::collections::HashSet;

use tokio::io::{AsyncRead, AsyncWrite};

use tokio_util::codec::Framed;

use crate::Error;

/// Reference an account; with the option to implicitly reference self.
pub enum OptAccRef {
  Current,
  Id(i64),
  Name(String)
}


/// Explicitly reference an account.
pub enum AccRef {
  Id(i64),
  Name(String)
}


#[derive(Debug)]
pub struct Account {
  pub id: i64,
  pub name: String,
  pub lock: bool,
  pub perms: HashSet<String>
}


/// Get information about an account.
/// The `acc` parameter can be used to query the account by id, name or get
/// information about the connection's current owner.
pub async fn rd<T: AsyncRead + AsyncWrite + Unpin>(
  conn: &mut Framed<T, blather::Codec>,
  acc: OptAccRef
) -> Result<Account, Error> {
  let mut tg = blather::Telegram::new_topic("RdAcc")?;

  match acc {
    OptAccRef::Id(id) => {
      tg.add_param("Id", id)?;
    }
    OptAccRef::Name(nm) => {
      tg.add_str("Name", &nm)?;
    }
    OptAccRef::Current => {}
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


#[derive(Debug)]
pub struct LsEntry {
  pub id: i64,
  pub name: String
}


/// Get a list of accounts.
///
/// This will only retreive a list of numeric account identifiers and the
/// associated unique account name.  To get detailed information about each
/// account the application needs to call [`rd`](self::rd) for each
/// entry.
pub async fn ls<T: AsyncRead + AsyncWrite + Unpin>(
  conn: &mut Framed<T, blather::Codec>,
  inclock: bool
) -> Result<Vec<LsEntry>, Error> {
  let mut tg = blather::Telegram::new_topic("LsAcc")?;

  match inclock {
    true => {
      tg.add_bool("All", true)?;
    }
    _ => {}
  }

  let params = crate::sendrecv(conn, &tg).await?;

  let num_entries = params.get_int::<usize>("#")?;

  let mut acclist = Vec::with_capacity(num_entries);
  for i in 0..num_entries {
    let id = format!("{}.Id", i);
    let name = format!("{}.Name", i);

    acclist.push(LsEntry {
      id: params.get_int::<i64>(&id).unwrap(),
      name: params.get_str(&name).unwrap().to_string()
    });
  }

  Ok(acclist)
}


/// Enumeration of account permission change methods.
pub enum ModPerms {
  /// Reset the account's permissions to the ones passed in the supplied
  /// HashSet.
  Set(HashSet<String>),

  /// Add the supplied permissions to the account's permissions.  Collisions
  /// are ignored.
  Grant(HashSet<String>),

  /// Remove the supplied permissions from the account's permissions.  Removal
  /// of permissions the account doesn't have are silently ignored.
  Revoke(HashSet<String>),

  /// First grant permissions to the account, then remove permissions.
  GrantRevoke(HashSet<String>, HashSet<String>)
}


/// Account fields to update.
pub struct WrAccount {
  /// New account name.
  /// This is currently not supported.
  pub name: Option<String>,

  /// New real name field.  Set to empty field to remove the current value.
  pub username: Option<String>,

  /// Whether account should be locked or unlocked.
  pub lock: Option<bool>,

  /// Account permissions.  If the `set` field is used, then `grant` and
  /// `revoke` are ignored.
  pub perms: Option<ModPerms>
}


/// Update an account.
pub async fn wr<T: AsyncRead + AsyncWrite + Unpin>(
  conn: &mut Framed<T, blather::Codec>,
  acc: AccRef,
  ai: WrAccount
) -> Result<(), Error> {
  let mut tg = blather::Telegram::new_topic("WrAcc")?;

  match acc {
    AccRef::Id(id) => {
      tg.add_param("Id", id)?;
    }
    AccRef::Name(nm) => {
      tg.add_str("Name", &nm)?;
    }
  }

  if let Some(name) = ai.name {
    tg.add_str("NewName", &name)?;
  }
  if let Some(username) = ai.username {
    tg.add_str("UserName", &username)?;
  }
  if let Some(lck) = ai.lock {
    tg.add_bool("Lock", lck)?;
  }

  if let Some(perms) = ai.perms {
    match perms {
      ModPerms::Set(set) => {
        tg.add_strit("Perms", set.iter())?;
      }
      ModPerms::Grant(set) => {
        tg.add_strit("Grant", set.iter())?;
      }
      ModPerms::Revoke(set) => {
        tg.add_strit("Revoke", set.iter())?;
      }
      ModPerms::GrantRevoke(grant, revoke) => {
        tg.add_strit("Grant", grant.iter())?;
        tg.add_strit("Revoke", revoke.iter())?;
      }
    }
  }

  crate::sendrecv(conn, &tg).await?;

  Ok(())
}


/// Remove an account.
pub async fn rm<T: AsyncRead + AsyncWrite + Unpin>(
  conn: &mut Framed<T, blather::Codec>,
  acc: AccRef
) -> Result<(), Error> {
  let mut tg = blather::Telegram::new_topic("RmAcc")?;

  match acc {
    AccRef::Id(id) => {
      tg.add_param("Id", id)?;
    }
    AccRef::Name(nm) => {
      tg.add_str("Name", &nm)?;
    }
  }

  crate::sendrecv(conn, &tg).await?;

  Ok(())
}


// vim: set ft=rust et sw=2 ts=2 sts=2 cinoptions=2 tw=79 :
