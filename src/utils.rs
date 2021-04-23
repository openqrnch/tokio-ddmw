use std::fs::File;
use std::io::BufRead;
use std::path::Path;

/// Read the first line of a file.
/// Returns `None` on any problems (file not found, unable to open, file has
/// no lines, etc).
/// Returns `Some(String)` on success.
///
/// # Notes
/// - This file will trim the end of the string, and thus it is unsuitable for
///   reading strings which may end with whitespaces.
pub(crate) fn read_single_line<P>(fname: P) -> Option<String>
where
  P: AsRef<Path>
{
  if let Ok(mut lines) = read_lines(fname.as_ref()) {
    if let Some(line) = lines.next() {
      if let Ok(l) = line {
        Some(l.trim_end().to_string())
      } else {
        None
      }
    } else {
      None
    }
  } else {
    None
  }
}


/// Return a buffered line iterator for reading a file line-by-line.
pub(crate) fn read_lines<P>(
  filename: P
) -> std::io::Result<std::io::Lines<std::io::BufReader<File>>>
where
  P: AsRef<Path>
{
  let file = std::fs::File::open(filename)?;
  Ok(std::io::BufReader::new(file).lines())
}

// vim: set ft=rust et sw=2 ts=2 sts=2 cinoptions=2 tw=79 :
