use std::mem;

use bytes::{BufMut, Bytes, BytesMut};

use tokio::io;

use tokio_util::codec::Decoder;
use tokio_util::codec::Encoder;

use ezmsg::Msg;

use crate::err::Error;

#[derive(Clone,Debug)]
enum CodecState {
  Lines,
  Binary
}

pub enum Input {
  Msg(Msg),
  Bin(BytesMut, usize)
}

#[derive(Clone, Debug)]
pub struct Codec {
  next_line_index: usize,
  max_line_length: usize,
  msg: Msg,
  state: CodecState,
  bin_remain: usize
}

impl Codec {
  pub fn new() -> Codec {
    Codec {
      next_line_index: 0,
      max_line_length: usize::MAX,
      msg: Msg::new(),
      state: CodecState::Lines,
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

  /// `decode_msg_lines` has encountered an eol, determined that the string is
  /// longer than zero characters, and thus passed the line to this function to
  /// process it.
  ///
  /// The first line received is a message topic.  This is a required line.
  /// Following lines are parameter lines, which are a single space character
  /// separated key/value pair.
  fn decode_line(&mut self, line: &str) -> Result<(), Error> {
    if self.msg.get_topic().is_none() {
      self.msg.set_topic(line)?;
    } else {
      let idx = line.find(' ');
      if let Some(idx) = idx {
        let (k, v) = line.split_at(idx);
        let v = &v[1..v.len()];
        self.msg.add_param(k, v);
      }
    }
    Ok(())
  }

  /// (New) data is available in the input buffer.
  /// Try to parse lines until an empty line as been encountered, at which
  /// point the buffer is parsed and returned in an [`Msg`] buffer.
  ///
  /// If the buffer doesn't contain enough data to finalize a complete message
  /// buffer return `Ok(None)` to inform the calling FramedRead that more data
  /// is needed.
  ///
  /// [`Msg`]: ezmsg::Msg
  fn decode_msg_lines(
      &mut self,
      buf: &mut BytesMut
  ) -> Result<Option<Msg>, Error> {
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
          if line.len() == 0 {
            // mem::take() can replace a member of a struct.
            // (This requires Default to be implemented for the object being
            // taken).
            return Ok(Some(mem::take(&mut self.msg)));
          } else {
            self.decode_line(&line)?;
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
  /// ```
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
      CodecState::Lines => {
        // If decode_msg_lines returns Some(value) it means that a complete Msg
        // buffer has been received.
        let msg = self.decode_msg_lines(buf)?;
        if let Some(msg) = msg {
          // A complete Msg was received
          return Ok(Some(Input::Msg(msg)));
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
            self.state = CodecState::Lines;
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
    }
  }
}


impl Encoder<&Msg> for Codec {
  type Error = crate::err::Error;

  fn encode(
      &mut self,
      msg: &Msg,
      buf: &mut BytesMut
  ) -> Result<(), Error> {
    msg.encoder_write(buf)?;
    Ok(())
  }
}


impl Encoder<Bytes> for Codec {
  type Error = io::Error;

  fn encode(
      &mut self,
      data: Bytes,
      buf: &mut BytesMut
  ) -> Result<(), io::Error> {
    //println!("Writing {} bin data", data.len());
    buf.reserve(data.len());
    buf.put(data);
    Ok(())
  }
}

// vim: set ft=rust et sw=2 ts=2 sts=2 cinoptions=2 tw=79 :
