//! A tokio-util [`Codec`] that is used to encode and decode the DDMW client
//! interface protocol.
//!
//! [`Codec`]: https://docs.rs/tokio-util/0.3/tokio_util/codec/index.html

use std::fmt;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::{cmp, collections::HashMap, mem};

use bytes::{BufMut, Bytes, BytesMut};

use tokio::io;

use tokio_util::codec::Decoder;
use tokio_util::codec::Encoder;

use blather::{KVLines, Params, Telegram};

use crate::err::Error;


/// Current state of decover
#[derive(Clone, Debug, PartialEq)]
enum CodecState {
  Telegram,
  Params,
  KVLines,
  Chunks,
  Buf,
  File,
  Writer,
  Skip
}

/// Data returned to the application when the Codec's Decode iterator is
/// called and the decoder has a complete entity to return.
pub enum Input {
  Telegram(Telegram),
  KVLines(KVLines),
  Params(Params),
  Chunk(BytesMut, usize),
  Buf(BytesMut),
  File(PathBuf),
  WriteDone,
  SkipDone
}


/// The Codec (exposed externally as ClntIfCodec) is used to keep track of the
/// state of the inbound and outbound communication.
pub struct Codec {
  next_line_index: usize,
  max_line_length: usize,
  tg: Telegram,
  params: Params,
  kvlines: KVLines,
  state: CodecState,
  bin_remain: usize,
  pathname: Option<PathBuf>,
  writer: Option<Box<dyn Write>>,
  buf: BytesMut
}

impl fmt::Debug for Codec {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("Codec").field("state", &self.state).finish()
  }
}

impl Default for Codec {
  fn default() -> Self {
    Codec::new()
  }
}


/// A Codec used to encode and decode the DDMW client interface protocol.
///
/// # Notes
/// Normally the Codec object is hidden inside a [`Framed`] object. In order to
/// call methods in the codec it must be accessed through the Framed object:
///
/// ```compile_fail
/// let mut conn = Framed::new(socket, Codec::new());
/// // ...
/// conn.codec_mut().expect_chunks(len);
/// ```
///
/// [`Framed`]: https://docs.rs/tokio-util/0.3/tokio_util/codec/struct.Framed.html
impl Codec {
  pub fn new() -> Codec {
    Codec {
      next_line_index: 0,
      max_line_length: usize::MAX,
      tg: Telegram::new(),
      params: Params::new(),
      kvlines: KVLines::new(),
      state: CodecState::Telegram,
      bin_remain: 0,
      pathname: None,
      writer: None,
      buf: BytesMut::new()
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
      let read_to =
        cmp::min(self.max_line_length.saturating_add(1), buf.len());

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
          return Err(Error::BadFormat(
            "Exceeded maximum line length.".to_string()
          ));
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

  /// Read buffer line-by-line, split each line at the first space character
  /// and store the left part as a key and the right part as a value in a
  /// Params structure.
  fn decode_params_lines(
    &mut self,
    buf: &mut BytesMut
  ) -> Result<Option<Params>, Error> {
    loop {
      // Determine how far into the buffer we'll search for a newline. If
      // there's no max_length set, we'll read to the end of the buffer.
      let read_to =
        cmp::min(self.max_line_length.saturating_add(1), buf.len());

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

          // Empty line marks end of Params
          if line.is_empty() {
            // Revert to expecting a telegram once a Params has been completed.
            // The application can override this when needed.
            self.state = CodecState::Telegram;

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
          return Err(Error::BadFormat(
            "Exceeded maximum line length.".to_string()
          ));
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

  fn decode_kvlines(
    &mut self,
    buf: &mut BytesMut
  ) -> Result<Option<KVLines>, Error> {
    loop {
      // Determine how far into the buffer we'll search for a newline. If
      // there's no max_length set, we'll read to the end of the buffer.
      let read_to =
        cmp::min(self.max_line_length.saturating_add(1), buf.len());

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

          // Empty line marks end of Params
          if line.is_empty() {
            // Revert to expecting a telegram once a KVLines  has been
            // completed.
            // The application can override this when needed.
            self.state = CodecState::Telegram;

            // mem::take() can replace a member of a struct.
            // (This requires Default to be implemented for the object being
            // taken).
            return Ok(Some(mem::take(&mut self.kvlines)));
          } else {
            let idx = line.find(' ');
            if let Some(idx) = idx {
              let (k, v) = line.split_at(idx);
              let v = &v[1..v.len()];
              self.kvlines.append(k, v);
            }
          }
        }
        None if buf.len() > self.max_line_length => {
          return Err(Error::BadFormat(
            "Exceeded maximum line length.".to_string()
          ));
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


  /// Set the decoder to treat the next `size` bytes as raw bytes to be
  /// received in chunks.
  ///
  /// # Decoder behavior
  /// The decoder will return an `Input::Chunk(buf, remain)` to the application
  /// each time a new chunk has been received. In addition to the actual
  /// chunk number of bytes remaining will be returned.  The remaining bytes
  /// value is adjusted to subtract the currently returned chunk, which means
  /// that the application can detect the end of the buffer by checking if
  /// the remaining value is zero.
  pub fn expect_chunks(&mut self, size: usize) {
    //println!("Expecting bin {}", size);
    self.state = CodecState::Chunks;
    self.bin_remain = size;
  }

  /// Expect a buffer of a certain size to be received.
  /// The returned buffer will be stored in process memory.
  ///
  /// # Decoder behavior
  /// One a complete buffer has been successfully reaceived the `Decoder` will
  /// return an `Input::Buf(b)` where `b` is a `bytes::BytesMut` containing the
  /// entire buffer.
  ///
  /// Once the entire buffer has been received by the `Decoder` it will revert
  /// to expect an `Input::Telegram`.
  pub fn expect_buf(&mut self, size: usize) -> Result<(), Error> {
    if size == 0 {
      return Err(Error::InvalidSize("The size must not be zero".to_string()));
    }
    self.state = CodecState::Buf;
    self.bin_remain = size;
    self.buf = BytesMut::with_capacity(size);
    Ok(())
  }

  /// Called from application that expects a certain amount of bytes of data to
  /// arrive from the peer, and that data should be stored to a file.
  ///
  /// # Decoder behavior
  /// On successful completion the Decoder will return an Input::File(pathname)
  /// once the entire file length has successfully been received, where the
  /// pathname is a PathBuf which matches the pathname parameter passed to
  /// this function.
  pub fn expect_file<P: Into<PathBuf>>(
    &mut self,
    pathname: P,
    size: usize
  ) -> Result<(), Error> {
    if size == 0 {
      return Err(Error::InvalidSize("The size must not be zero".to_string()));
    }
    self.state = CodecState::File;
    let pathname = pathname.into();
    self.writer = Some(Box::new(File::create(&pathname)?));
    self.pathname = Some(pathname);

    self.bin_remain = size;

    Ok(())
  }

  /// Called from an application to request that data should be written to a
  /// supplied writer.
  ///
  /// The writer's ownership will be transferred to the `Decoder` and will
  /// automatically be dropped once the entire buffer has been written.
  ///
  /// # Decoder behavior
  /// On successful completion the Decoder will return an Input::WriteDone to
  /// signal that the entire buffer has been received and written to the
  /// `Writer`.
  ///
  /// Once the complete `Params` buffer has been received the Decoder will
  /// revert back to waiting for a `Telegram`.
  pub fn expect_writer<W: 'static + Write>(
    &mut self,
    writer: W,
    size: usize
  ) -> Result<(), Error> {
    if size == 0 {
      return Err(Error::InvalidSize("The size must not be zero".to_string()));
    }
    self.state = CodecState::Writer;
    self.writer = Some(Box::new(writer));
    self.bin_remain = size;
    Ok(())
  }

  /// Tell the Decoder to expect lines of key/value pairs.
  ///
  /// # Decoder behavior
  /// On successful completion the Framed StreamExt next() will return an
  /// Input::Params(params) once a complete `Params` buffer has been received.
  ///
  /// Once the complete `Params` buffer has been received the Decoder will
  /// revert back to waiting for a `Telegram`.
  pub fn expect_params(&mut self) {
    self.state = CodecState::Params;
  }

  /// Tell the Decoder to expect lines ordered key/value pairs.
  ///
  /// # Decoder behavior
  /// On successful completion the Framed StreamExt next() will return an
  /// Input::KVLines(kvlines) once a complete `KVLines` buffer has been
  /// received.
  ///
  /// Once the complete `KVLines` buffer has been received the Decoder will
  /// revert back to waiting for a `Telegram`.
  pub fn expect_kvlines(&mut self) {
    self.state = CodecState::KVLines;
  }

  /// Skip bytes.
  ///
  /// # Decoder behavior
  /// Simply ignore the number of specified bytes, then revert back to waiting
  /// for a Telegram.
  pub fn skip(&mut self, size: usize) -> Result<(), Error> {
    if size == 0 {
      return Err(Error::InvalidSize("The size must not be zero".to_string()));
    }
    self.state = CodecState::Skip;
    self.bin_remain = size;
    Ok(())
  }
}

fn utf8(buf: &[u8]) -> Result<&str, io::Error> {
  std::str::from_utf8(buf).map_err(|_| {
    io::Error::new(
      io::ErrorKind::InvalidData,
      "Unable to decode input as UTF8"
    )
  })
}

fn without_carriage_return(s: &[u8]) -> &[u8] {
  if let Some(&b'\r') = s.last() {
    &s[..s.len() - 1]
  } else {
    s
  }
}


/// A Decoder implementation that is used to assist in decoding data arriving
/// over a DDM client interface.
///
/// The default behavior for the Decoder is to wait for a Telegram buffer.  It
/// will, on success, return an `Input::Telegram(tg)`, where `tg` is a
/// `blather::Telegram` object.
impl Decoder for Codec {
  type Item = Input;
  type Error = crate::err::Error;

  fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Input>, Error> {
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
      CodecState::KVLines => {
        // If decode_telegram_lines returns Some(value) it means that a
        // complete buffer has been received.
        let kvlines = self.decode_kvlines(buf)?;
        if let Some(kvlines) = kvlines {
          // A complete Params buffer was received
          return Ok(Some(Input::KVLines(kvlines)));
        }

        // Returning Ok(None) tells the caller that we need more data
        Ok(None)
      }

      CodecState::Chunks => {
        if buf.is_empty() {
          // Need more data
          return Ok(None);
        }

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
        Ok(Some(Input::Chunk(buf.split_to(read_to), self.bin_remain)))
      }
      CodecState::Buf => {
        if buf.is_empty() {
          // Need more data
          return Ok(None);
        }
        let read_to = cmp::min(self.bin_remain, buf.len());

        // Transfer data from input to output buffer
        self.buf.put(buf.split_to(read_to));

        self.bin_remain -= read_to;
        if self.bin_remain != 0 {
          // Need more data
          return Ok(None);
        }

        // When no more data is expected for this binary part, revert to
        // expecting Msg lines
        self.state = CodecState::Telegram;

        // Return a buffer and the amount of data remaining, this buffer
        // included.  The application can check if remain is 0 to determine
        // if it has received all the expected binary data.
        Ok(Some(Input::Buf(mem::take(&mut self.buf))))
      }
      CodecState::File | CodecState::Writer => {
        if buf.is_empty() {
          return Ok(None); // Need more data
        }

        // Read as much data as available or requested and write it to our
        // output.
        let read_to = cmp::min(self.bin_remain, buf.len());
        if let Some(ref mut f) = self.writer {
          f.write_all(&buf.split_to(read_to))?;
        }

        self.bin_remain -= read_to;
        if self.bin_remain != 0 {
          return Ok(None); // Need more data
        }

        // At this point the entire expected buffer has been received

        // Close file
        self.writer = None;

        // Return a buffer and the amount of data remaining, this buffer
        // included.  The application can check if remain is 0 to determine
        // if it has received all the expected binary data.
        let ret = if self.state == CodecState::File {
          let pathname = if let Some(ref fname) = self.pathname {
            fname.clone()
          } else {
            return Err(Error::BadState("Missing pathname".to_string()));
          };

          // Reset the pathname
          self.pathname = None;

          Input::File(pathname)
        } else {
          Input::WriteDone
        };

        // Revert to the default of expecting a telegram.
        self.state = CodecState::Telegram;

        Ok(Some(ret))
      } // CodecState::{File|Writer}
      CodecState::Skip => {
        if buf.is_empty() {
          return Ok(None); // Need more data
        }

        // Read as much data as available or requested and write it to our
        // output.
        let read_to = cmp::min(self.bin_remain, buf.len());
        let _ = buf.split_to(read_to);

        self.bin_remain -= read_to;
        if self.bin_remain != 0 {
          return Ok(None); // Need more data
        }

        // Revert to the default of expecting a telegram.
        self.state = CodecState::Telegram;

        Ok(Some(Input::SkipDone))
      } // CodecState::Skip
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


impl Encoder<&KVLines> for Codec {
  type Error = crate::err::Error;

  fn encode(
    &mut self,
    kvlines: &KVLines,
    buf: &mut BytesMut
  ) -> Result<(), Error> {
    kvlines.encoder_write(buf)?;
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
