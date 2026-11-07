//! The digests that the lock file records, as specified in `docs/specs/lock-file-format.md`.
//!
//! A digest is written `ALGORITHM:HEX`. The only algorithm is `sha256`, and the digits are lowercase. A digest depends
//! only on the paths and the contents of the files, so it is the same wherever a project is and on every platform.

use sha2::{Digest, Sha256};

/// The name of the algorithm, which is the start of every digest.
pub const ALGORITHM: &str = "sha256";

/// The bytes of a byte order mark in UTF-8.
const BYTE_ORDER_MARK: [u8; 3] = [0xEF, 0xBB, 0xBF];

/// The bytes of a file as they are digested: a byte order mark at the start is removed, and each CRLF is replaced with
/// a LF, in one pass. A lone CR is left alone. So the same file gives the same digest whether it was saved with LF or
/// CRLF line endings, and whether or not an editor added a byte order mark.
pub fn normalise(bytes: &[u8]) -> Vec<u8> {
    let bytes = bytes.strip_prefix(&BYTE_ORDER_MARK).unwrap_or(bytes);
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\r' && bytes.get(i + 1) == Some(&b'\n') {
            out.push(b'\n');
            i += 2;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    out
}

/// The digest of the bytes as they are, written `sha256:HEX`.
fn digest(bytes: &[u8]) -> String {
    let mut text = format!("{}:", ALGORITHM);
    for byte in Sha256::digest(bytes) {
        text.push_str(&format!("{:02x}", byte));
    }
    text
}

/// The digest of a file, from its bytes as they are on disk.
pub fn file_digest(bytes: &[u8]) -> String {
    digest(&normalise(bytes))
}

/// The digest of a project, from its files as pairs of a path inside the project, written with `/`, and the digest of
/// the file. The pairs may be in any order. The digest is that of a text with a line `PATH DIGEST` for each file, in
/// order of the bytes of the paths, each line ended by a LF.
pub fn project_digest(files: &[(String, String)]) -> String {
    let mut sorted: Vec<&(String, String)> = files.iter().collect();
    sorted.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    let mut text = String::new();
    for (path, file) in sorted {
        text.push_str(path);
        text.push(' ');
        text.push_str(file);
        text.push('\n');
    }
    digest(text.as_bytes())
}

/// Whether the text is well-formed as a digest, which is `sha256:` and 64 lowercase hexadecimal digits.
pub fn is_digest(text: &str) -> bool {
    match text.strip_prefix(ALGORITHM).and_then(|rest| rest.strip_prefix(':')) {
        Some(hex) => hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The files of the worked example in `docs/specs/lock-file-format.md`, where the second is saved with CRLF.
    const PROJECT_FILE: &[u8] = b"[project]\nname = \"com.example.lib\"\n";
    const TYPES_FILE_CRLF: &[u8] = b"enum Shape = Circle;\r\nconst unit: Shape = Circle;\r\n";
    const PROJECT_FILE_DIGEST: &str = "sha256:1cfdac805747c77556236b673fc4fb71a344aa39d1f8dfdb37bf53fbfae70830";
    const TYPES_FILE_DIGEST: &str = "sha256:1b0ecdd124592b686f69f8d4544f644fa34b2732bdb94e0500eda45c77025346";
    const PROJECT_DIGEST: &str = "sha256:753517eedcd17f9bae3699b7e6b2a1be5f91f4e382a3d80de5f0f59ebdc5b163";

    #[test]
    fn the_digest_of_nothing_is_the_known_one() {
        assert_eq!(
            file_digest(b""),
            "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn the_worked_example_of_the_spec_has_the_digests_that_it_gives() {
        assert_eq!(file_digest(PROJECT_FILE), PROJECT_FILE_DIGEST);
        assert_eq!(file_digest(TYPES_FILE_CRLF), TYPES_FILE_DIGEST);
        let files = vec![
            ("constructo.toml".to_string(), PROJECT_FILE_DIGEST.to_string()),
            ("shapes/circle.types".to_string(), TYPES_FILE_DIGEST.to_string()),
        ];
        assert_eq!(project_digest(&files), PROJECT_DIGEST);
    }

    #[test]
    fn line_endings_do_not_change_a_file_digest() {
        let lf = b"enum Shape = Circle;\nconst unit: Shape = Circle;\n";
        assert_eq!(file_digest(lf), file_digest(TYPES_FILE_CRLF));
        // The raw bytes with CRLF give another digest, so the normalisation is what makes them equal.
        assert_ne!(digest(TYPES_FILE_CRLF), file_digest(TYPES_FILE_CRLF));
    }

    #[test]
    fn a_byte_order_mark_does_not_change_a_file_digest() {
        let with = [&BYTE_ORDER_MARK[..], PROJECT_FILE].concat();
        assert_eq!(file_digest(&with), PROJECT_FILE_DIGEST);
        // Only one at the start is removed.
        let twice = [&BYTE_ORDER_MARK[..], &BYTE_ORDER_MARK[..], PROJECT_FILE].concat();
        assert_ne!(file_digest(&twice), PROJECT_FILE_DIGEST);
        // One that is not at the start is a part of the text.
        let inside = [PROJECT_FILE, &BYTE_ORDER_MARK[..]].concat();
        assert_ne!(file_digest(&inside), PROJECT_FILE_DIGEST);
    }

    #[test]
    fn a_lone_carriage_return_is_left_alone() {
        assert_eq!(normalise(b"a\rb\r\nc\r"), b"a\rb\nc\r");
        // The replacement is done once: CR CR LF becomes CR LF and not LF.
        assert_eq!(normalise(b"a\r\r\nb"), b"a\r\nb");
        assert_eq!(normalise(b"\r"), b"\r");
        assert_eq!(normalise(b"\n"), b"\n");
    }

    #[test]
    fn the_order_of_the_files_does_not_change_the_project_digest() {
        let a = ("a.types".to_string(), file_digest(b"enum A = X;"));
        let b = ("b.types".to_string(), file_digest(b"enum B = Y;"));
        assert_eq!(
            project_digest(&[a.clone(), b.clone()]),
            project_digest(&[b.clone(), a.clone()])
        );
        // The paths and the contents both matter.
        let renamed = ("c.types".to_string(), b.1.clone());
        assert_ne!(
            project_digest(&[a.clone(), b.clone()]),
            project_digest(&[a.clone(), renamed])
        );
        assert_ne!(project_digest(&[a.clone(), b]), project_digest(&[a]));
    }

    #[test]
    fn paths_are_sorted_by_their_bytes() {
        // An uppercase letter sorts before a lowercase one, and a slash before a letter, in the order of the bytes.
        let digest_of = |path: &str| (path.to_string(), file_digest(b"x"));
        let expected = {
            let lines = |paths: &[&str]| {
                let mut text = String::new();
                for path in paths {
                    text.push_str(&format!("{} {}\n", path, file_digest(b"x")));
                }
                super::digest(text.as_bytes())
            };
            lines(&["B.types", "a.types", "a/b.types", "ab.types"])
        };
        let files = vec![
            digest_of("ab.types"),
            digest_of("a/b.types"),
            digest_of("a.types"),
            digest_of("B.types"),
        ];
        assert_eq!(project_digest(&files), expected);
    }

    #[test]
    fn a_project_with_no_files_has_the_digest_of_nothing() {
        assert_eq!(project_digest(&[]), file_digest(b""));
    }

    #[test]
    fn a_digest_is_recognised_by_its_form() {
        assert!(is_digest(PROJECT_DIGEST));
        for bad in [
            "",
            "sha256:",
            "sha256:abc",
            "sha1:1cfdac805747c77556236b673fc4fb71a344aa39d1f8dfdb37bf53fbfae70830",
            "1cfdac805747c77556236b673fc4fb71a344aa39d1f8dfdb37bf53fbfae70830",
            // Uppercase digits and a digit that is not hexadecimal are refused.
            "sha256:1CFDAC805747C77556236B673FC4FB71A344AA39D1F8DFDB37BF53FBFAE70830",
            "sha256:1cfdac805747c77556236b673fc4fb71a344aa39d1f8dfdb37bf53fbfae7083g",
            // One digit too many.
            "sha256:1cfdac805747c77556236b673fc4fb71a344aa39d1f8dfdb37bf53fbfae708300",
        ] {
            assert!(!is_digest(bad), "{}", bad);
        }
    }
}
