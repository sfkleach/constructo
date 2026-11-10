//! Telling the types files from the JSON files among the inputs of `convert --from=json-typed`, without reading the
//! JSON. The types files are a prefix of the inputs, and a file is looked at only as far as its first token.

use std::io::{self, BufRead};

/// What the first token of a file says that it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirstToken {
    /// There is no token, so the file is empty or has only white space and comments.
    Empty,
    /// The word `enum` or `const`, which starts a declaration, so the file is a types file.
    Types,
    /// Anything else, which is how JSON starts.
    Other,
}

/// The longest word that is looked at, which is longer than any word that starts a types file.
const WORD: usize = 8;

/// Reads from the start of a file as far as its first token, and no further. White space, a byte-order mark and `//`
/// comments before it are skipped, and so a file that starts with a long comment is read as far as the comment goes.
/// The word `enum` or `const` that is the start of a qualified name, as in `enum::Name`, is not the start of a
/// declaration, as the parser has it.
pub fn first_token(reader: &mut dyn BufRead) -> io::Result<FirstToken> {
    if skip_blanks(reader, true)? == Blanks::Slash {
        return Ok(FirstToken::Other);
    }
    let mut word = Vec::new();
    while let Some(byte) = peek(reader)? {
        if !(byte.is_ascii_alphanumeric() || byte == b'_') {
            break;
        }
        if word.len() == WORD {
            return Ok(FirstToken::Other);
        }
        word.push(byte);
        reader.consume(1);
    }
    if word.is_empty() {
        return Ok(match peek(reader)? {
            Some(_) => FirstToken::Other,
            None => FirstToken::Empty,
        });
    }
    if word != b"enum" && word != b"const" {
        return Ok(FirstToken::Other);
    }
    if skip_blanks(reader, false)? == Blanks::Slash {
        return Ok(FirstToken::Types);
    }
    Ok(match peek(reader)? {
        Some(b':') | Some(b'.') => FirstToken::Other,
        _ => FirstToken::Types,
    })
}

/// The next byte, without taking it, or `None` at the end of the file.
fn peek(reader: &mut dyn BufRead) -> io::Result<Option<u8>> {
    Ok(reader.fill_buf()?.first().copied())
}

/// Where the white space and the comments end.
#[derive(PartialEq)]
enum Blanks {
    /// At a byte that is not taken, or at the end of the file.
    Done,
    /// After a slash that does not start a comment, which has been taken. No declaration starts with a slash.
    Slash,
}

/// Takes the white space and the comments, and with `start` a byte-order mark before them.
fn skip_blanks(reader: &mut dyn BufRead, start: bool) -> io::Result<Blanks> {
    if start && reader.fill_buf()?.starts_with(&[0xEF, 0xBB, 0xBF]) {
        reader.consume(3);
    }
    while let Some(byte) = peek(reader)? {
        if byte.is_ascii_whitespace() {
            reader.consume(1);
        } else if byte == b'/' {
            reader.consume(1);
            if peek(reader)? != Some(b'/') {
                return Ok(Blanks::Slash);
            }
            skip_line(reader)?;
        } else {
            break;
        }
    }
    Ok(Blanks::Done)
}

/// Takes the rest of the line, including its end.
fn skip_line(reader: &mut dyn BufRead) -> io::Result<()> {
    loop {
        let buffer = reader.fill_buf()?;
        if buffer.is_empty() {
            return Ok(());
        }
        match buffer.iter().position(|&byte| byte == b'\n') {
            Some(at) => {
                reader.consume(at + 1);
                return Ok(());
            }
            None => {
                let all = buffer.len();
                reader.consume(all);
            }
        }
    }
}

/// Where the types files end among the `count` inputs, which is the number of them that are types files: they are the
/// first inputs, and what follows is JSON. With `marker`, the number of inputs before a `--`, the types files are
/// those, and no input is looked at, since the marker says what they are. Without it each input is looked at in turn,
/// by `first`, which gives what its first token says, and the types files end before the first input whose first token
/// is not that of a declaration. An input with no token is a types file that is left out of the JSON, as the marker
/// would have said, so the walk goes on past it. The inputs that follow the end are not looked at.
pub fn types_end<E>(
    count: usize,
    marker: Option<usize>,
    mut first: impl FnMut(usize) -> Result<FirstToken, E>,
) -> Result<usize, E> {
    if let Some(marker) = marker {
        return Ok(marker.min(count));
    }
    for index in 0..count {
        if first(index)? == FirstToken::Other {
            return Ok(index);
        }
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufReader, Cursor, Read};

    fn token(text: &str) -> FirstToken {
        first_token(&mut Cursor::new(text.as_bytes().to_vec())).unwrap()
    }

    #[test]
    fn a_declaration_starts_a_types_file() {
        assert_eq!(token("enum A = X;"), FirstToken::Types);
        assert_eq!(token("const x = 1;"), FirstToken::Types);
        assert_eq!(token("\n  enum A = X;"), FirstToken::Types);
        assert_eq!(token("enum"), FirstToken::Types);
        assert_eq!(token("\u{feff}enum A = X;"), FirstToken::Types);
    }

    #[test]
    fn json_starts_with_something_else() {
        for text in [
            "{}",
            "[1]",
            "\"enum\"",
            "5",
            "true",
            "null",
            "enumeration",
            "constant",
            "/x",
            "-1",
        ] {
            assert_eq!(token(text), FirstToken::Other, "{text}");
        }
        assert_eq!(token("enum::A;"), FirstToken::Other);
        assert_eq!(token("enum . A::B;"), FirstToken::Other);
    }

    #[test]
    fn a_file_with_no_token_is_empty() {
        assert_eq!(token(""), FirstToken::Empty);
        assert_eq!(token("  \n\t\r\n"), FirstToken::Empty);
        assert_eq!(token("// nothing here\n  // or here"), FirstToken::Empty);
        assert_eq!(token("\u{feff}"), FirstToken::Empty);
    }

    #[test]
    fn comments_before_the_first_token_are_skipped() {
        assert_eq!(token("// types\n// more\nenum A = X;"), FirstToken::Types);
        assert_eq!(token("// not json\n{}"), FirstToken::Other);
        assert_eq!(token("enum // comment\n A = X;"), FirstToken::Types);
        assert_eq!(token("enum // comment\n ::A;"), FirstToken::Other);
    }

    /// A reader that counts what it has been asked for, of a file that never ends.
    struct Endless {
        byte: u8,
        given: usize,
    }

    impl Read for Endless {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            buffer.fill(self.byte);
            self.given += buffer.len();
            Ok(buffer.len())
        }
    }

    #[test]
    fn only_the_start_of_a_large_file_is_read() {
        for byte in *b"{a5" {
            let mut reader = BufReader::with_capacity(4096, Endless { byte, given: 0 });
            assert_eq!(first_token(&mut reader).unwrap(), FirstToken::Other);
            assert!(reader.get_ref().given <= 4096, "{}", reader.get_ref().given);
        }
    }

    #[test]
    fn a_comment_is_read_to_its_end_and_no_further() {
        let mut text = b"// ".to_vec();
        text.extend(std::iter::repeat_n(b'x', 100_000));
        text.extend(b"\nenum A = X; ");
        text.extend(std::iter::repeat_n(b'y', 100_000));
        let mut reader = BufReader::with_capacity(4096, Cursor::new(text));
        assert_eq!(first_token(&mut reader).unwrap(), FirstToken::Types);
    }

    fn end(tokens: &[FirstToken], marker: Option<usize>) -> (usize, Vec<usize>) {
        let mut looked = Vec::new();
        let at = types_end::<()>(tokens.len(), marker, |index| {
            looked.push(index);
            Ok(tokens[index])
        })
        .unwrap();
        (at, looked)
    }

    #[test]
    fn the_types_files_end_at_the_first_json_file() {
        use FirstToken::*;
        assert_eq!(end(&[Types, Types, Other, Other], None), (2, vec![0, 1, 2]));
        assert_eq!(end(&[Other], None), (0, vec![0]));
        assert_eq!(end(&[Types, Types], None), (2, vec![0, 1]));
        assert_eq!(end(&[], None), (0, vec![]));
    }

    #[test]
    fn an_empty_file_is_skipped_before_the_first_json_file_and_is_json_after_it() {
        use FirstToken::*;
        assert_eq!(end(&[Empty, Types, Empty, Other, Empty], None), (3, vec![0, 1, 2, 3]));
        assert_eq!(end(&[Empty, Empty], None), (2, vec![0, 1]));
    }

    #[test]
    fn a_types_file_after_the_json_is_not_looked_at() {
        use FirstToken::*;
        assert_eq!(end(&[Types, Other, Types, Types], None), (1, vec![0, 1]));
    }

    #[test]
    fn the_marker_ends_the_types_files_and_nothing_is_looked_at() {
        use FirstToken::*;
        assert_eq!(end(&[Types, Empty, Other, Types], Some(2)), (2, vec![]));
        assert_eq!(end(&[Other, Other], Some(0)), (0, vec![]));
        assert_eq!(end(&[Types, Types], Some(2)), (2, vec![]));
    }

    #[test]
    fn a_json_file_before_the_marker_is_left_to_the_types_parser() {
        use FirstToken::*;
        assert_eq!(end(&[Other, Types], Some(2)).0, 2);
    }

    #[test]
    fn an_input_that_cannot_be_read_ends_the_walk_with_its_error() {
        let result = types_end(3, None, |index| {
            if index == 1 {
                Err("unreadable")
            } else {
                Ok(FirstToken::Types)
            }
        });
        assert_eq!(result, Err("unreadable"));
    }
}
