//! Helpers for the unit tests: values that are as deep as the parser allows, and files on disk.

use crate::fetch::{child_environment, Config, Scratch};
use std::path::PathBuf;
use std::process::Command;

/// A project on disk, in a folder of its own under the temporary folder. The files that are made are the only
/// things removed when it is dropped, and then the folders, which must be empty.
pub(crate) struct OnDisk {
    pub folder: PathBuf,
    files: Vec<PathBuf>,
}

impl OnDisk {
    pub fn new(test: &str, files: &[(&str, &[u8])]) -> OnDisk {
        let folder = std::env::temp_dir().join(format!("constructo-{}-{}", std::process::id(), test));
        let mut made = Vec::new();
        for (relative, bytes) in files {
            let path = folder.join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, bytes).unwrap();
            made.push(path);
        }
        OnDisk { folder, files: made }
    }
    /// Adds a file to the project, which is removed with the others when it is dropped.
    pub fn add(&mut self, relative: &str, bytes: &[u8]) {
        let path = self.folder.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, bytes).unwrap();
        self.files.push(path);
    }
}

impl Drop for OnDisk {
    fn drop(&mut self) {
        let mut folders = std::collections::BTreeSet::new();
        for file in &self.files {
            let _ = std::fs::remove_file(file);
            let mut parent = file.parent();
            while let Some(folder) = parent.filter(|folder| folder.starts_with(&self.folder)) {
                folders.insert(folder.to_path_buf());
                parent = folder.parent();
            }
        }
        // The deepest first, so that each folder is empty when it is removed.
        for folder in folders.iter().rev() {
            let _ = std::fs::remove_dir(folder);
        }
    }
}

/// A repository in a folder of its own, made by git, with the commits that a test makes in it.
pub(crate) struct Repo {
    pub folder: PathBuf,
    _scratch: Scratch,
}

impl Repo {
    pub fn new() -> Repo {
        let scratch = Scratch::new(&std::env::temp_dir()).unwrap();
        let folder = scratch.0.join("repo");
        std::fs::create_dir(&folder).unwrap();
        let repo = Repo {
            folder,
            _scratch: scratch,
        };
        repo.git(&["init", "--quiet", "--initial-branch=main"]);
        repo
    }

    pub fn git(&self, args: &[&str]) -> String {
        let mut command = Command::new("git");
        // The same environment as the git that fetches, with no configuration of the system or the user, so that a
        // variable such as `GIT_DIR`, which git sets when it runs a hook, or a hook or signing setting in the
        // configuration of the developer, cannot reach the repository that is made for the test.
        command
            .env_clear()
            .envs(child_environment(Config::Isolated, |name| std::env::var_os(name)));
        let output = command
            .arg("-C")
            .arg(&self.folder)
            .args([
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(["-c", "tag.gpgsign=false"])
            .args(args)
            .output()
            .expect("git is needed to test fetching");
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    /// Writes a file and commits everything, returning the commit.
    pub fn commit(&self, file: &str, text: &str) -> String {
        let path = self.folder.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
        self.git(&["add", "-A"]);
        self.git(&["commit", "--quiet", "-m", text]);
        self.git(&["rev-parse", "HEAD"])
    }

    pub fn url(&self) -> String {
        self.folder.display().to_string()
    }
}

/// The ways that a value can hold another, each of which the formats write differently.
pub(crate) const NESTINGS: [&str; 5] = ["array", "record", "constructor record", "tuple", "named"];

/// The text of a value that is `levels` levels deep, as the parser counts them, by the nesting that is named: a nat
/// inside `levels - 1` arrays, records, constructors with a record, tuples or named arguments.
pub(crate) fn nested_text(nesting: &str, levels: usize) -> String {
    let mut text = "0".to_string();
    for _ in 1..levels {
        text = match nesting {
            "array" => format!("[{text}]"),
            "record" => format!("{{ a: {text} }}"),
            "constructor record" => format!("C {{ a: {text} }}"),
            "tuple" => format!("C({text})"),
            "named" => format!("C(b: {text})"),
            _ => panic!("No nesting {nesting}."),
        };
    }
    format!("{text};")
}

/// Runs a test in a thread with a stack of 8 MB, which is the size of the stack of the main thread on Linux, where the
/// command runs. A test thread has 2 MB, and a value as deep as the parser allows takes more than that to read in a
/// debug build, where every frame is much larger than in a release build. The stack is given, since otherwise it is
/// read from `RUST_MIN_STACK` in the environment.
pub(crate) fn on_a_main_sized_stack(test: impl FnOnce() + Send + 'static) {
    let thread = std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(test)
        .expect("The thread starts.");
    if let Err(panic) = thread.join() {
        std::panic::resume_unwind(panic);
    }
}
