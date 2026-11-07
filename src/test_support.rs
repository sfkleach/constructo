//! Helpers for the unit tests that need files on disk.

use crate::fetch::Scratch;
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
        // A variable such as `GIT_DIR` or `GIT_INDEX_FILE`, which git sets when it runs a hook, would make this git
        // work on the repository of the developer and not on the one made for the test.
        for (name, _) in std::env::vars_os() {
            if name.to_string_lossy().starts_with("GIT_") {
                command.env_remove(&name);
            }
        }
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
