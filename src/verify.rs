//! Verification of a loaded set of projects against the lock file, as specified in `docs/specs/lock-file-format.md`.
//!
//! The lock records every project that is loaded other than the root project and the built-in ones. It must describe
//! exactly what is loaded: nothing missing, nothing extra, and every digest the one that the files have.

use crate::diagnostics::{Diagnostic, Pos};
use crate::lock::{Lock, LockedProject, LOCK_FILE};
use crate::project::{Project, ProjectError, ProjectSet};
use std::collections::{HashMap, HashSet};
use std::io::ErrorKind;
use std::path::Path;

/// The advice that ends the message of a problem that `constructo project update` puts right.
const UPDATE: &str = "run `constructo project update` to record it";

/// Reads the lock file in the folder of the root project. A folder with no lock file has none, which is not an error
/// here, since a project with nothing to record needs no lock file. A file that cannot be read, or does not parse, is.
pub fn read_lock(folder: &Path) -> Result<Option<Lock>, ProjectError> {
    let path = folder.join(LOCK_FILE);
    match std::fs::read_to_string(&path) {
        Ok(text) => Lock::parse(&text, &path.display().to_string())
            .map(Some)
            .map_err(ProjectError::Invalid),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(ProjectError::Cannot(format!("Cannot read {}: {}", path.display(), e))),
    }
}

/// For each project that was loaded, the project that imports it and the number of the import there, for the first
/// import of it in the order that the projects are loaded: depth-first, in the order of the `[[import]]` entries.
fn first_imports(set: &ProjectSet) -> HashMap<usize, (usize, usize)> {
    fn visit(set: &ProjectSet, index: usize, seen: &mut HashSet<usize>, found: &mut HashMap<usize, (usize, usize)>) {
        for (number, import) in set.projects[index].imports.iter().enumerate() {
            if let Some(target) = import.target {
                if seen.insert(target) {
                    found.insert(target, (index, number));
                    visit(set, target, seen, found);
                }
            }
        }
    }
    let root = set.projects.len() - 1;
    let mut found = HashMap::new();
    visit(set, root, &mut HashSet::from([root]), &mut found);
    found
}

/// The place of an import in the project-file of the project that has it.
fn at_import(set: &ProjectSet, (importer, number): (usize, usize)) -> (String, Pos) {
    let project = &set.projects[importer];
    (project.project_file(), project.imports[number].name_pos)
}

/// Compares the projects that were loaded with the lock, which is `None` if there is no lock file. The problems are all
/// returned, since each is an independent fact: a diagnostic for each digest that differs, each project or file that is
/// not in the lock, and each project or file in the lock that is not loaded. The root project is never recorded.
///
/// A file whose project has been loaded but cannot now be read is an error and not a diagnostic.
pub fn verify_lock(set: &ProjectSet, lock: Option<&Lock>) -> Result<Vec<Diagnostic>, ProjectError> {
    let root = set.projects.len() - 1;
    let lock_name = set.root().folder.join(LOCK_FILE).display().to_string();
    let loaded: Vec<(usize, &Project)> = set
        .projects
        .iter()
        .enumerate()
        .filter(|(index, project)| *index != root && !project.builtin)
        .collect();
    let first = first_imports(set);
    let mut found = Vec::new();

    let Some(lock) = lock else {
        // With nothing to record there is nothing to verify, and a lock file is not needed.
        if let Some((index, project)) = loaded.first() {
            let (file, pos) = at_import(set, first[index]);
            let message = format!(
                "The project imports {}, but there is no {}; run `constructo project update` to make one",
                project.name, LOCK_FILE
            );
            found.push(Diagnostic::new(&file, pos, message));
        }
        return Ok(found);
    };

    for (index, project) in &loaded {
        let Some(locked) = lock.get(&project.name) else {
            let (file, pos) = at_import(set, first[index]);
            let message = format!("The project {} is not in {}; {}", project.name, LOCK_FILE, UPDATE);
            found.push(Diagnostic::new(&file, pos, message));
            continue;
        };
        let actual = project.lock_entry()?;
        compare(&lock_name, locked, &actual, &mut found);
    }
    for locked in &lock.projects {
        if !loaded.iter().any(|(_, project)| project.name == locked.name) {
            let message = format!("The lock has the project {}, which is not imported", locked.name);
            found.push(Diagnostic::new(&lock_name, locked.pos, message));
        }
    }
    Ok(found)
}

/// Compares the files of a project with the lock entry for it, adding a diagnostic for each difference, in the order of
/// the paths. The digest of the project follows from the digests of its files, so a difference in it is shown by them.
fn compare(lock_name: &str, locked: &LockedProject, actual: &LockedProject, found: &mut Vec<Diagnostic>) {
    for (path, now) in &actual.files {
        match locked.files.get(path) {
            None => {
                let message = format!(
                    "The file {} of project {} is not in {}; {}",
                    path, locked.name, LOCK_FILE, UPDATE
                );
                found.push(Diagnostic::new(lock_name, locked.pos, message));
            }
            Some(recorded) if recorded.digest != now.digest => {
                let message = format!(
                    "Checksum mismatch for {} of project {}: the lock has {} but the file has {}",
                    path, locked.name, recorded.digest, now.digest
                );
                found.push(Diagnostic::new(lock_name, recorded.pos, message));
            }
            Some(_) => {}
        }
    }
    for (path, recorded) in &locked.files {
        if !actual.files.contains_key(path) {
            let message = format!(
                "The lock has the file {} of project {}, which is no longer in the project",
                path, locked.name
            );
            found.push(Diagnostic::new(lock_name, recorded.pos, message));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::OnDisk;

    const LIB: &str = "[project]\nname = \"example.lib\"\n";
    const SHAPES: &[u8] = b"enum Shape = Circle;\n";

    /// A root project `app` that imports `lib`, which holds the file `shapes.types`, in folders of their own.
    fn pair(test: &str) -> (OnDisk, OnDisk) {
        let lib = OnDisk::new(
            &format!("{}-lib", test),
            &[("constructo.toml", LIB.as_bytes()), ("shapes.types", SHAPES)],
        );
        let app_file = format!(
            "[project]\nname = \"example.app\"\n\n[[import]]\nproject-name = \"example.lib\"\nsource = \"{}\"\n",
            lib.folder.display()
        );
        (
            OnDisk::new(&format!("{}-app", test), &[("constructo.toml", app_file.as_bytes())]),
            lib,
        )
    }

    fn load(app: &OnDisk) -> ProjectSet {
        ProjectSet::load(&app.folder, true).unwrap()
    }

    /// The lock that `project update` would make for the set, as it would be read back from the file.
    fn locked(set: &ProjectSet) -> Lock {
        let entries: Vec<LockedProject> = set
            .projects
            .iter()
            .enumerate()
            .filter(|(index, project)| *index != set.projects.len() - 1 && !project.builtin)
            .map(|(_, project)| project.lock_entry().unwrap())
            .collect();
        Lock::parse(&Lock::new(entries).render(), "constructo.lock").unwrap()
    }

    fn messages(found: &[Diagnostic]) -> Vec<String> {
        found.iter().map(|d| d.message.clone()).collect()
    }

    #[test]
    fn a_lock_that_describes_what_is_loaded_has_no_problems() {
        let (app, _lib) = pair("verify-ok");
        let set = load(&app);
        assert!(verify_lock(&set, Some(&locked(&set))).unwrap().is_empty());
    }

    #[test]
    fn a_changed_file_is_a_mismatch_that_names_the_file_and_both_digests() {
        let (app, lib) = pair("verify-changed");
        let set = load(&app);
        let lock = locked(&set);
        let old = lock.get("example.lib").unwrap().files["shapes.types"].clone();
        std::fs::write(lib.folder.join("shapes.types"), b"enum Shape = Square;\n").unwrap();
        let found = verify_lock(&set, Some(&lock)).unwrap();
        assert_eq!(found.len(), 1);
        let new = crate::digest::file_digest(b"enum Shape = Square;\n");
        assert_eq!(
            found[0].message,
            format!(
                "Checksum mismatch for shapes.types of project example.lib: the lock has {} but the file has {}",
                old.digest, new
            )
        );
        // It is at the digest in the lock file.
        assert_eq!(found[0].pos, old.pos);
        assert!(found[0].file.ends_with("constructo.lock"), "{}", found[0].file);
    }

    #[test]
    fn a_changed_project_file_is_noticed_too() {
        let (app, lib) = pair("verify-toml");
        let set = load(&app);
        let lock = locked(&set);
        std::fs::write(
            lib.folder.join("constructo.toml"),
            b"[project]\nname = \"example.lib\"\nexclude = [\"x\"]\n",
        )
        .unwrap();
        let found = verify_lock(&set, Some(&lock)).unwrap();
        assert_eq!(found.len(), 1);
        assert!(found[0]
            .message
            .starts_with("Checksum mismatch for constructo.toml of project example.lib"));
    }

    #[test]
    fn a_new_file_and_a_file_that_has_gone_are_both_reported() {
        let (app, mut lib) = pair("verify-files");
        let set = load(&app);
        let lock = locked(&set);
        // A file is added after the set was loaded, so it is loaded again to see the file.
        lib.add("more.types", b"enum More = M;\n");
        let set = load(&app);
        let found = verify_lock(&set, Some(&lock)).unwrap();
        assert_eq!(
            messages(&found),
            ["The file more.types of project example.lib is not in constructo.lock; run `constructo project update` \
              to record it"]
        );
        // It is at the entry of the project, which is where its name is written in the lock.
        assert_eq!(found[0].pos, lock.get("example.lib").unwrap().pos);
        std::fs::remove_file(lib.folder.join("more.types")).unwrap();
        std::fs::remove_file(lib.folder.join("shapes.types")).unwrap();
        let set = load(&app);
        let found = verify_lock(&set, Some(&lock)).unwrap();
        assert_eq!(
            messages(&found),
            ["The lock has the file shapes.types of project example.lib, which is no longer in the project"]
        );
        assert_eq!(found[0].pos, lock.get("example.lib").unwrap().files["shapes.types"].pos);
    }

    #[test]
    fn a_project_that_is_not_in_the_lock_is_reported_at_the_import_that_loaded_it() {
        let (app, _lib) = pair("verify-missing");
        let set = load(&app);
        let found = verify_lock(&set, Some(&Lock::default())).unwrap();
        assert_eq!(
            messages(&found),
            ["The project example.lib is not in constructo.lock; run `constructo project update` to record it"]
        );
        // It is at the `project-name` of the import, in the project-file of the importing project. The prelude is the
        // first import in effect but is not written there.
        assert!(found[0].file.ends_with("constructo.toml"), "{}", found[0].file);
        assert_eq!(found[0].pos, Pos { line: 5, col: 16 });
    }

    #[test]
    fn a_project_in_the_lock_that_is_not_loaded_is_reported_at_its_entry() {
        let (app, _lib) = pair("verify-stale");
        let set = load(&app);
        let other = LockedProject::new(
            "example.old",
            &[("constructo.toml".to_string(), crate::digest::file_digest(b"x"))],
        );
        let mut projects = locked(&set).projects;
        projects.push(other);
        let text = Lock::new(projects).render();
        let lock = Lock::parse(&text, "constructo.lock").unwrap();
        let found = verify_lock(&set, Some(&lock)).unwrap();
        assert_eq!(
            messages(&found),
            ["The lock has the project example.old, which is not imported"]
        );
        assert_eq!(found[0].pos, lock.get("example.old").unwrap().pos);
    }

    #[test]
    fn no_lock_file_is_an_error_at_the_first_import_when_there_is_something_to_record() {
        let (app, _lib) = pair("verify-nolock");
        let set = load(&app);
        let found = verify_lock(&set, None).unwrap();
        assert_eq!(
            messages(&found),
            [
                "The project imports example.lib, but there is no constructo.lock; run `constructo project update` to \
              make one"
            ]
        );
        assert_eq!(found[0].pos, Pos { line: 5, col: 16 });
    }

    #[test]
    fn a_project_with_nothing_to_record_needs_no_lock_file() {
        let alone = OnDisk::new(
            "verify-alone",
            &[("constructo.toml", b"[project]\nname = \"example.alone\"\n")],
        );
        let set = ProjectSet::load(&alone.folder, true).unwrap();
        assert!(verify_lock(&set, None).unwrap().is_empty());
        // A lock that lists a project it does not load is still stale.
        let other = LockedProject::new(
            "example.old",
            &[("constructo.toml".to_string(), crate::digest::file_digest(b"x"))],
        );
        let found = verify_lock(&set, Some(&Lock::new(vec![other]))).unwrap();
        assert_eq!(
            messages(&found),
            ["The lock has the project example.old, which is not imported"]
        );
    }

    #[test]
    fn the_problems_are_all_reported() {
        let (app, lib) = pair("verify-all");
        let set = load(&app);
        let lock = locked(&set);
        std::fs::write(lib.folder.join("shapes.types"), b"enum Shape = Square;\n").unwrap();
        std::fs::write(
            lib.folder.join("constructo.toml"),
            b"[project]\nname = \"example.lib\"\n\n",
        )
        .unwrap();
        let found = verify_lock(&set, Some(&lock)).unwrap();
        assert_eq!(found.len(), 2);
    }

    #[test]
    fn the_lock_file_is_read_from_the_folder_of_the_project() {
        let (app, _lib) = pair("verify-read");
        assert_eq!(read_lock(&app.folder).unwrap(), None);
        let set = load(&app);
        let text = locked(&set).render();
        let with = OnDisk::new("verify-read-with", &[("constructo.lock", text.as_bytes())]);
        let lock = read_lock(&with.folder).unwrap().unwrap();
        assert!(lock.get("example.lib").is_some());
        let bad = OnDisk::new("verify-read-bad", &[("constructo.lock", b"version = 2\n")]);
        match read_lock(&bad.folder).unwrap_err() {
            ProjectError::Invalid(d) => assert!(d.message.starts_with("Unsupported lock file version 2")),
            other => panic!("not a diagnostic: {}", other),
        }
    }
}
