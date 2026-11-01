//! Helpers for the unit tests that need files on disk.

use std::path::PathBuf;

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
