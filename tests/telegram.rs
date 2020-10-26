use tokio::stream::StreamExt;

use tokio_test::io::Builder;

use tokio_util::codec::Framed;

use tokio_ddmw::clntif;

use tokio_ddmw::Error;

#[tokio::test]
async fn valid_no_params() {
  let mut mock = Builder::new();

  mock.read(b"hello\n\n");

  let mut frm = Framed::new(mock.build(), clntif::Codec::new());

  while let Some(o) = frm.next().await {
    let o = o.unwrap();
    if let clntif::Input::Telegram(tg) = o {
      assert_eq!(tg.get_topic(), Some("hello"));
      let params = tg.into_params();
      let map = params.into_inner();
      assert_eq!(map.len(), 0);
    } else {
      panic!("Not a Telegram");
    }
  }
}


#[tokio::test]
async fn valid_with_params() {
  let mut mock = Builder::new();

  mock.read(b"hello\nmurky_waters off\nwrong_impression cows\n\n");

  let mut frm = Framed::new(mock.build(), clntif::Codec::new());

  while let Some(o) = frm.next().await {
    let o = o.unwrap();

    match o {
      clntif::Input::Telegram(tg) => {
        assert_eq!(tg.get_topic(), Some("hello"));
        let params = tg.into_params();
        let map = params.into_inner();
        assert_eq!(map.len(), 2);
        assert_eq!(map.get("murky_waters").unwrap(), "off");
        assert_eq!(map.get("wrong_impression").unwrap(), "cows");
      }
      _ => {
        panic!("Not a Telegram");
      }
    }
  }
}


#[tokio::test]
async fn bad_topic() {
  let mut mock = Builder::new();

  // space isn't allowed in topic
  mock.read(b"hel lo\n\n");

  let mut frm = Framed::new(mock.build(), clntif::Codec::new());
  if let Some(e) = frm.next().await {
    if let Err(e) = e {
      match e {
        Error::Blather(s) => {
          assert_eq!(s, "Bad format; Invalid topic character");
        }
        _ => {
          panic!("Wrong error");
        }
      }
    } else {
      panic!("Unexpected success");
    }
  } else {
    panic!("Didn't get expected frame");
  }
}


// vim: set ft=rust et sw=2 ts=2 sts=2 cinoptions=2 tw=79 :
