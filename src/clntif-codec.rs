//! A tokio-util [`Codec`] that is used to encode and decode the DDMW client
//! interface protocol.
//!
//! [`Codec`]: https://docs.rs/tokio-util/0.3/tokio_util/codec/index.html

use std::{
  cmp,
  collections::HashMap,
  mem
};

use bytes::{BufMut, Bytes, BytesMut};

use tokio::io;

use tokio_util::codec::Decoder;
use tokio_util::codec::Encoder;

use blather::{Telegram, Params};

use crate::err::Error;

#[derive(Clone,Debug)]
enum CodecState {
  Telegram,
  Params,
  Binary
}

pub enum Input {
  Telegram(Telegram),
  Params(Params),
  Bin(BytesMut, usize)
}


/// The Codec (exposed externally as ClntIfCodec) is used to keep track of the
/// state of the inbound and outbound communication.
#[derive(Clone, Debug)]
pub struct Codec {
  next_line_index: usize,
  max_line_length: usize,
  tg: Telegram,
  params: Params,
  state: CodecState,
  bin_remain: usize
}

impl Default for Codec {
  fn default() -> Self {
    Codec::new()
  }
}

impl Codec {
  pub fn new() -> Codec {
    Codec {
      next_line_index: 0,
      max_line_length: usize::MAX,
      tg: Telegram::new(),
      params: Params::new(),
      state: CodecState::Telegram,
      bin_remain: 0
    }
  }

  pub fn new_with_max_length(max_line_length: usize) -> Self {
    Codec {
      max_line_length,
      ..Codec::new()
    }
  }

  pub fn max_line_length(&self) -> usize {
    self.max_line_length
  }

  /// `decode_telegram_lines` has encountered an eol, determined that the
  /// string is longer than zero characters, and thus passed the line to this
  /// function to process it.
  ///
  /// The first line received is a telegram topic.  This is a required line.
  /// Following lines are parameter lines, which are a single space character
  /// separated key/value pairs.
  fn decode_telegram_line(&mut self, line: &str) -> Result<(), Error> {
    if self.tg.get_topic().is_none() {
      self.tg.set_topic(line)?;
    } else {
      let idx = line.find(' ');
      if let Some(idx) = idx {
        let (k, v) = line.split_at(idx);
        let v = &v[1..v.len()];
        self.tg.add_param(k, v);
      }
    }
    Ok(())
  }

  /// (New) data is available in the input buffer.
  /// Try to parse lines until an empty line as been encountered, at which
  /// point the buffer is parsed and returned in an [`Telegram`] buffer.
  ///
  /// If the buffer doesn't contain enough data to finalize a complete telegram
  /// buffer return `Ok(None)` to inform the calling FramedRead that more data
  /// is needed.
  ///
  /// [`Telegram`]: blather::Telegram
  fn decode_telegram_lines(
      &mut self,
      buf: &mut BytesMut
  ) -> Result<Option<Telegram>, Error> {
    loop {
      // Determine how far into the buffer we'll search for a newline. If
      // there's no max_length set, we'll read to the end of the buffer.
      let read_to = cmp::min(self.max_line_length.saturating_add(1),
          buf.len());

      let newline_offset = buf[self.next_line_index..read_to]
        .iter()
        .position(|b| *b == b'\n');

      match newline_offset {
        Some(offset) => {
          // Found an eol
          let newline_index = offset + self.next_line_index;
          self.next_line_index = 0;
          let line = buf.split_to(newline_index + 1);
          //println!("buf remain: {}", buf.len());
          let line = &line[..line.len() - 1];
          let line = without_carriage_return(line);
          let line = utf8(line)?;

          // Empty line marks end of Msg
          if line.is_empty() {
            // mem::take() can replace a member of a struct.
            // (This requires Default to be implemented for the object being
            // taken).
            return Ok(Some(mem::take(&mut self.tg)));
          } else {
            self.decode_telegram_line(&line)?;
          }
        }
        None if buf.len() > self.max_line_length => {
          return Err(Error::BadFormat("Exceeded maximum line length."
                .to_string()));
        }
        None => {
          // We didn't find a line or reach the length limit, so the next
          // call will resume searching at the current offset.
          self.next_line_index = read_to;

          // Returning Ok(None) instructs the FramedRead that more data is
          // needed.
          return Ok(None);
        }
      }
    }
  }


  fn decode_params_lines(
      &mut self,
      buf: &mut BytesMut
  ) -> Result<Option<Params>, Error> {
    loop {
      // Determine how far into the buffer we'll search for a newline. If
      // there's no max_length set, we'll read to the end of the buffer.
      let read_to = cmp::min(self.max_line_length.saturating_add(1),
          buf.len());

      let newline_offset = buf[self.next_line_index..read_to]
        .iter()
        .position(|b| *b == b'\n');

      match newline_offset {
        Some(offset) => {
          // Found an eol
          let newline_index = offset + self.next_line_index;
          self.next_line_index = 0;
          let line = buf.split_to(newline_index + 1);
          //println!("buf remain: {}", buf.len());
          let line = &line[..line.len() - 1];
          let line = without_carriage_return(line);
          let line = utf8(line)?;

          // Empty line marks end of Msg
          if line.is_empty() {
            // mem::take() can replace a member of a struct.
            // (This requires Default to be implemented for the object being
            // taken).
            return Ok(Some(mem::take(&mut self.params)));
          } else {
            let idx = line.find(' ');
            if let Some(idx) = idx {
              let (k, v) = line.split_at(idx);
              let v = &v[1..v.len()];
              self.params.add_param(k, v);
            }
          }
        }
        None if buf.len() > self.max_line_length => {
          return Err(Error::BadFormat("Exceeded maximum line length."
                .to_string()));
        }
        None => {
          // We didn't find a line or reach the length limit, so the next
          // call will resume searching at the current offset.
          self.next_line_index = read_to;

          // Returning Ok(None) instructs the FramedRead that more data is
          // needed.
          return Ok(None);
        }
      }
    }
  }


  /// Called from application in order to instruct decoder that binary data is
  /// going to be received.  The number of bytes must be supplied.
  ///
  /// Note that normally the Codec object is hidden inside a [`Framed`]
  /// object, and in order to call this method it must be accessed through the
  /// Framed object:
  ///
  /// ```compile_fail
  /// let mut conn = Framed::new(socket, Codec::new());
  /// // ...
  /// conn.codec_mut().expect_bin(len);
  /// ```
  ///
  /// [`Framed`]: https://docs.rs/tokio-util/0.3/tokio_util/codec/struct.Framed.html
  pub fn expect_bin(&mut self, size: usize) {
    //println!("Expecting bin {}", size);
    self.state = CodecState::Binary;
    self.bin_remain = size;
  }

  /// Tell the Decoder to expect lines of key/value pairs.
  pub fn expect_params(&mut self) {
    self.state = CodecState::Params;
  }
}


fn utf8(buf: &[u8]) -> Result<&str, io::Error> {
  std::str::from_utf8(buf)
    .map_err(|_| io::Error::new(io::ErrorKind::InvalidData,
          "Unable to decode input as UTF8"))
}

fn without_carriage_return(s: &[u8]) -> &[u8] {
  if let Some(&b'\r') = s.last() {
    &s[..s.len() - 1]
  } else {
    s
  }
}


/// The Decoder implementation of the client interface codec handles two
/// primary states:
///
/// - Getting a line-based protocol.
/// - Getting fixed-length binary data.
impl Decoder for Codec {
  type Item = Input;
  type Error = crate::err::Error;

  fn decode(
      &mut self,
      buf: &mut BytesMut
  ) -> Result<Option<Input>, Error> {

    // The codec's internal decoder state denotes whether lines or binary data
    // is currently being expected.
    match self.state {
      CodecState::Telegram => {
        // If decode_telegram_lines returns Some(value) it means that a
        // complete buffer has been received.
        let tg = self.decode_telegram_lines(buf)?;
        if let Some(tg) = tg {
          // A complete Telegram was received
          return Ok(Some(Input::Telegram(tg)));
        }

        // Returning Ok(None) tells the caller that we need more data
        Ok(None)
      }
      CodecState::Params => {
        // If decode_telegram_lines returns Some(value) it means that a
        // complete buffer has been received.
        let params = self.decode_params_lines(buf)?;
        if let Some(params) = params {
          // A complete Params buffer was received
          return Ok(Some(Input::Params(params)));
        }

        // Returning Ok(None) tells the caller that we need more data
        Ok(None)
      }
      CodecState::Binary => {
        if !buf.is_empty() {
          let read_to = cmp::min(self.bin_remain, buf.len());

          self.bin_remain -= read_to;
          if self.bin_remain == 0 {
            // When no more data is expected for this binary part, revert to
            // expecting Msg lines
            self.state = CodecState::Telegram;
          }

          // Return a buffer and the amount of data remaining, this buffer
          // included.  The application can check if remain is 0 to determine
          // if it has received all the expected binary data.
          Ok(Some(Input::Bin(buf.split_to(read_to), self.bin_remain)))
        } else {
          // Need more data
          Ok(None)
        }
      }
    } // match self.state
  }
}


impl Encoder<&Telegram> for Codec {
  type Error = crate::err::Error;

  fn encode(
      &mut self,
      tg: &Telegram,
      buf: &mut BytesMut
  ) -> Result<(), Error> {
    tg.encoder_write(buf)?;
    Ok(())
  }
}


impl Encoder<&Params> for Codec {
  type Error = crate::err::Error;

  fn encode(
      &mut self,
      params: &Params,
      buf: &mut BytesMut
  ) -> Result<(), Error> {
    params.encoder_write(buf)?;
    Ok(())
  }
}


impl Encoder<&HashMap<String, String>> for Codec {
  type Error = crate::err::Error;

  fn encode(
      &mut self,
      data: &HashMap<String, String>,
      buf: &mut BytesMut
  ) -> Result<(), Error> {
    // Calculate the amount of space required
    let mut sz = 0;
    for (k, v) in data.iter() {
      // key space + whitespace + value space + eol
      sz += k.len() + 1 + v.len() + 1;
    }

    // Terminating empty line
    sz += 1;

    //println!("Writing {} bin data", data.len());
    buf.reserve(sz);

    for (k, v) in data.iter() {
      buf.put(k.as_bytes());
      buf.put_u8(b' ');
      buf.put(v.as_bytes());
      buf.put_u8(b'\n');
    }
    buf.put_u8(b'\n');

    Ok(())
  }
}


impl Encoder<Bytes> for Codec {
  type Error = crate::err::Error;

  fn encode(
      &mut self,
      data: Bytes,
      buf: &mut BytesMut
  ) -> Result<(), crate::err::Error> {
    buf.reserve(data.len());
    buf.put(data);
    Ok(())
  }
}


// vim: set ft=rust et sw=2 ts=2 sts=2 cinoptions=2 tw=79 :
