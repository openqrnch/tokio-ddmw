use tokio::stream::StreamExt;

use tokio_test::io::Builder;

use tokio_util::codec::Framed;

use tokio_ddmw::clntif;

#[tokio::test]
async fn tg_followed_by_buf() {
  let mut mock = Builder::new();

  mock.read(b"hello\nlen 4\n\n1234");

  let mut frm = Framed::new(mock.build(), clntif::Codec::new());

  while let Some(o) = frm.next().await {
    let o = o.unwrap();
    if let clntif::Input::Telegram(tg) = o {
      assert_eq!(tg.get_topic(), Some("hello"));
      assert_eq!(tg.get_int::<usize>("len").unwrap(), 4);
      frm.codec_mut().expect_buf(4);
      break;
    } else {
      panic!("Not a Telegram");
    }
  }

  while let Some(o) = frm.next().await {
    let o = o.unwrap();
    if let clntif::Input::Buf(bm) = o {
    } else {
      panic!("Not a Buf");
    }
  }
}

// vim: set ft=rust et sw=2 ts=2 sts=2 cinoptions=2 tw=79 :
