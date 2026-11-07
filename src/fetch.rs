//! Fetching a project from a git repository into a cache, as planned in
//! `docs/tasks/done/2026-10-21-projects/plan-step06.md`.
//!
//! Only `constructo project update` fetches. `check` looks in the cache for the commit that the lock names and never
//! goes to the network. The work is done by running `git`, which is treated with care, since the repository is
//! somebody else's: its transports are limited, no hook or program named by a configuration file is run, a URL cannot
//! be taken for an option, and a URL that holds a password is refused so that it is never recorded or printed.

use crate::project::ProjectError;
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

/// The transports that git may use, which is also what `GIT_ALLOW_PROTOCOL` is set to.
const ALLOWED_PROTOCOLS: &str = "https:ssh:file";

/// The settings that every call of git is given, so that a checkout is the same on every machine and runs nothing
/// that a configuration file names.
///
/// `core.symlinks=true` makes git check a link out as a link, whatever the configuration of the user or the probe of
/// the file system says. Without it a file system that cannot make links would get a plain file that holds the text of
/// the target, and the digest of the file, which is the digest of that text, would differ from the one made on another
/// machine. With it such a file system makes the checkout fail, which is an error to see and not a lock that differs.
const SETTINGS: [&str; 6] = [
    "core.symlinks=true",
    "core.hooksPath=/dev/null",
    "core.autocrlf=false",
    "core.fsmonitor=false",
    "protocol.file.allow=always",
    "advice.detachedHead=false",
];

/// Why a URL cannot be used, or `Ok` if it can. A URL is one of:
///
/// - `https://HOST/PATH`, with no user name or password;
/// - `ssh://[USER@]HOST/PATH` and the short form `[USER@]HOST:PATH`, with no password;
/// - `file://PATH`, or a path to a folder.
///
/// Anything else is refused, in particular the remote helpers such as `ext::COMMAND`, which run a program. A URL that
/// starts with `-` could be taken by git for an option. The check is made before git is run, as is the limit on the
/// transports, so that neither depends on the other.
pub fn validate_url(url: &str) -> Result<(), String> {
    if url.is_empty() {
        return Err("it is empty".to_string());
    }
    if url.starts_with('-') {
        return Err("it starts with '-', which git could take for an option".to_string());
    }
    if url.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err("it has a space or a control character".to_string());
    }
    if let Some((scheme, rest)) = url.split_once("://") {
        return match scheme {
            "https" => {
                let (authority, _) = rest.split_once('/').unwrap_or((rest, ""));
                if authority.is_empty() {
                    Err("it has no host".to_string())
                } else if authority.contains('@') {
                    Err(no_secrets("a user name or a password"))
                } else {
                    Ok(())
                }
            }
            "ssh" => {
                let (authority, _) = rest.split_once('/').unwrap_or((rest, ""));
                let host = match authority.rsplit_once('@') {
                    Some((user, _)) if user.contains(':') => return Err(no_secrets("a password")),
                    Some((_, host)) => host,
                    None => authority,
                };
                if host.is_empty() {
                    Err("it has no host".to_string())
                } else {
                    Ok(())
                }
            }
            "file" => {
                if rest.is_empty() {
                    Err("it has no path".to_string())
                } else {
                    Ok(())
                }
            }
            other => Err(format!(
                "the scheme {}:// is not supported; use https, ssh or file, or a path",
                other
            )),
        };
    }
    // A remote helper is written `NAME::ADDRESS`, and one of them runs a program.
    if url.contains("::") {
        return Err("it names a remote helper, which is not supported".to_string());
    }
    // The short form of ssh, `[USER@]HOST:PATH`, has a colon before any slash. A path to a folder has no colon, except
    // after the letter of a drive.
    let head = url.split('/').next().unwrap_or(url);
    if head.contains(':') && !is_drive(head) {
        let host = match head.rsplit_once('@') {
            Some((user, _)) if user.contains(':') => return Err(no_secrets("a password")),
            Some((_, host)) => host,
            None => head,
        };
        if host.split(':').next().is_none_or(str::is_empty) {
            return Err("it has no host".to_string());
        }
    }
    Ok(())
}

/// The URL with anything before an `@` in its first part replaced by `***`, so that a message can show a URL that has
/// a secret in it without repeating the secret.
pub fn redact(url: &str) -> String {
    let (scheme, rest) = url.split_once("://").map_or(("", url), |(scheme, rest)| (scheme, rest));
    let prefix = if scheme.is_empty() {
        String::new()
    } else {
        format!("{}://", scheme)
    };
    let end = rest.find('/').unwrap_or(rest.len());
    match rest[..end].rfind('@') {
        Some(at) => format!("{}***{}", prefix, &rest[at..]),
        None => url.to_string(),
    }
}

fn no_secrets(what: &str) -> String {
    format!(
        "it has {} in it, which would be written in the lock file and shown in messages; use a credential helper or \
         an ssh key",
        what
    )
}

/// Whether the text is a commit as git writes it in full: 40 hexadecimal digits, or 64 in a repository that uses
/// SHA-256. A commit that comes from a file is checked before it is used as a part of a path.
pub fn is_commit(text: &str) -> bool {
    (text.len() == 40 || text.len() == 64) && text.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// The name of the folder in the cache that holds the checkouts of a repository: the first 16 hexadecimal digits of
/// the SHA-256 of the URL as it is written.
pub fn url_key(url: &str) -> String {
    Sha256::digest(url.as_bytes())
        .iter()
        .take(8)
        .map(|byte| format!("{:02x}", byte))
        .collect()
}

/// The start of the name of a scratch folder.
const SCRATCH_PREFIX: &str = "tmp-";

/// The name of the lock file in a scratch folder. It is made with the folder, and its modified time is renewed for as
/// long as the run that made it is going, which is how `Scratch::sweep` tells such a run from one that was killed. A
/// file is used, and not the time of the folder itself, since a file can have its time changed on every platform,
/// while on Windows a folder cannot even be opened in the ordinary way.
const SCRATCH_LOCK: &str = "lock";

/// How often the lock file of a run that is going has its time renewed. It is much less than `STALE_AFTER`.
const HEARTBEAT: std::time::Duration = std::time::Duration::from_secs(60 * 60);

/// The size of the stack of the heartbeat thread, which is the size that Rust gives a thread when none is given. A
/// stack that is too small aborts the whole process, and a large one costs only address space, since memory is used
/// only for the part of the stack that is touched.
const HEARTBEAT_STACK: usize = 2 * 1024 * 1024;

/// How long the lock file of a scratch folder may go without being renewed before the folder is taken to belong to a
/// run that was killed. A run that is going renews it every `HEARTBEAT`, so it never gets this old.
const STALE_AFTER: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);

/// A folder that is made for a while and removed, with what is in it, when it is dropped.
pub(crate) struct Scratch(pub(crate) PathBuf, Heartbeat);

/// A thread that renews the modified time of a lock file every so often while its folder is in use, so that
/// `Scratch::sweep` can tell a run that is still going from one that was killed, however long the run takes. It stops
/// when it is dropped.
struct Heartbeat {
    stop: Option<std::sync::mpsc::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Heartbeat {
    /// Starts renewing the time of the lock file, which is held open, and so not opened again each time. A thread that
    /// cannot be started is an error, since a run with no heartbeat could have its folder swept by another.
    fn start(lock: std::fs::File, every: std::time::Duration) -> std::io::Result<Heartbeat> {
        let (stop, stopped) = std::sync::mpsc::channel::<()>();
        // The size of the stack is given, since otherwise it is read from `RUST_MIN_STACK` in the environment.
        let thread = std::thread::Builder::new()
            .name("heartbeat".to_string())
            .stack_size(HEARTBEAT_STACK)
            .spawn(move || {
                // Only a timeout means that it is time to renew. A message or a dropped sender means stop.
                while let Err(std::sync::mpsc::RecvTimeoutError::Timeout) = stopped.recv_timeout(every) {
                    // The file was made by this run and is held open by this thread, so changing its time fails only
                    // if the file system itself does, which the work of the fetch in the same folder runs into as
                    // well. The result is ignored because there is nothing to do about it here, and the next interval
                    // tries again.
                    let _ = lock.set_modified(std::time::SystemTime::now());
                }
            })?;
        Ok(Heartbeat {
            stop: Some(stop),
            thread: Some(thread),
        })
    }

    fn stop(&mut self) {
        // Dropping the sender wakes the thread at once, so that waiting for it does not take an interval. The thread
        // owns the lock file, so it is closed when the thread ends, before the folder is removed.
        self.stop.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for Heartbeat {
    fn drop(&mut self) {
        self.stop();
    }
}

impl Scratch {
    /// Makes a new folder with a name of its own in the folder given, with a lock file in it. The name has the process,
    /// the time and a count in it. A folder that is there already is left alone, since it may belong to a run that is
    /// still going (in a container that shares the cache, a process number is not unique), and another name is tried.
    pub(crate) fn new(parent: &Path) -> Result<Scratch, ProjectError> {
        Scratch::new_with(parent, HEARTBEAT)
    }

    fn new_with(parent: &Path, every: std::time::Duration) -> Result<Scratch, ProjectError> {
        static COUNT: AtomicU32 = AtomicU32::new(0);
        let name = || {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |since| since.as_nanos());
            format!(
                "{}{}-{}-{}",
                SCRATCH_PREFIX,
                std::process::id(),
                nanos,
                COUNT.fetch_add(1, Ordering::Relaxed)
            )
        };
        Scratch::make(parent, name, every)
    }

    /// Makes the folder with the first name that `name` gives that is not taken, and its lock file.
    fn make(
        parent: &Path,
        mut name: impl FnMut() -> String,
        every: std::time::Duration,
    ) -> Result<Scratch, ProjectError> {
        std::fs::create_dir_all(parent).map_err(|e| cannot(parent, "make", e))?;
        loop {
            let path = parent.join(name());
            match std::fs::create_dir(&path) {
                Ok(()) => {
                    let lock_path = path.join(SCRATCH_LOCK);
                    let lock = std::fs::File::create(&lock_path).map_err(|e| {
                        // The folder is this run's, so it is removed again, since nothing else would.
                        let _ = std::fs::remove_dir(&path);
                        cannot(&lock_path, "make", e)
                    })?;
                    // The folder is this run's, so it is removed again if the heartbeat cannot start, as above.
                    let heartbeat = Heartbeat::start(lock, every).map_err(|e| {
                        let _ = std::fs::remove_dir_all(&path);
                        ProjectError::Cannot(format!("Cannot start the heartbeat of {}: {}", path.display(), e))
                    })?;
                    return Ok(Scratch(path, heartbeat));
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(cannot(&path, "make", e)),
            }
        }
    }

    /// Removes the scratch folders in the folder given whose lock file was last renewed more than `STALE_AFTER` ago. A
    /// run that is killed leaves its folder behind, and nothing else would ever remove it. A folder whose lock file is
    /// younger may belong to a run that is still going, so it is left. A folder with no lock file, which a run that was
    /// killed between making the folder and making the file leaves, is judged by its own time instead. This is a
    /// tidy-up, so a failure to do any of it is ignored.
    pub(crate) fn sweep(parent: &Path) {
        let Ok(entries) = std::fs::read_dir(parent) else {
            return;
        };
        let stale = |path: &Path| {
            std::fs::metadata(path)
                .and_then(|meta| meta.modified())
                .ok()
                .and_then(|modified| modified.elapsed().ok())
                .is_some_and(|age| age > STALE_AFTER)
        };
        for entry in entries.flatten() {
            // The prefix is checked first, so that nothing is removed that this module did not make.
            let folder = entry.path();
            if !entry.file_name().to_string_lossy().starts_with(SCRATCH_PREFIX) || !folder.is_dir() {
                continue;
            }
            let lock = folder.join(SCRATCH_LOCK);
            let old = if lock.exists() { stale(&lock) } else { stale(&folder) };
            if old {
                let _ = std::fs::remove_dir_all(&folder);
            }
        }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        // The heartbeat is stopped first, so that it does not touch a folder that is being removed.
        self.1.stop();
        // The folder was made by `Scratch::new` for the purpose and holds only what git put in it.
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn cannot(path: &Path, action: &str, e: std::io::Error) -> ProjectError {
    ProjectError::Cannot(format!("Cannot {} {}: {}", action, path.display(), e))
}

/// A project fetched from a repository: the commit that was found, and the folder that holds its files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fetched {
    pub commit: String,
    pub path: PathBuf,
}

/// The cache of the checkouts, in a folder. Under it `git/KEY/COMMIT/` holds the files of one commit of one
/// repository, where KEY is the `url_key`. A checkout is made in a folder of its own and moved into place when it is
/// whole, and it is never changed afterwards, so any number of projects can share one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cache {
    root: PathBuf,
}

impl Cache {
    pub fn new(root: PathBuf) -> Cache {
        Cache { root }
    }

    /// The cache in the folder that is given, which comes from `--cache` or the configuration file, or else the one
    /// that the environment names.
    pub fn resolve(given: Option<&Path>) -> Result<Cache, ProjectError> {
        match given {
            Some(folder) => Ok(Cache::new(folder.to_path_buf())),
            None => Cache::from_environment(),
        }
    }

    /// The cache that the environment names: `$CONSTRUCTO_CACHE`, or else `constructo` in the cache folder of the user,
    /// which is `$XDG_CACHE_HOME` or `~/.cache`, or `%LOCALAPPDATA%` on Windows. A variable that is empty counts as
    /// not set.
    pub fn from_environment() -> Result<Cache, ProjectError> {
        Cache::from_variables(|name| {
            std::env::var_os(name)
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        })
    }

    /// As `from_environment`, with the variables given by a function so that it can be tried without changing them.
    ///
    /// A folder from the environment has to be absolute, since a relative one would depend on the folder that the run
    /// happens to be in, which is a second thing that nobody can see. A relative `CONSTRUCTO_CACHE` was set for
    /// `constructo`, so it is an error. The others are the user's general settings, and the XDG specification says
    /// that a relative one is to be ignored, so they are.
    fn from_variables(get: impl Fn(&str) -> Option<PathBuf>) -> Result<Cache, ProjectError> {
        if let Some(root) = get("CONSTRUCTO_CACHE") {
            if !root.is_absolute() {
                return Err(ProjectError::Cannot(format!(
                    "CONSTRUCTO_CACHE is {}, which is not an absolute path; set it to one, or give the folder with \
                     --cache",
                    root.display()
                )));
            }
            return Ok(Cache::new(root));
        }
        let absolute = |name: &str| get(name).filter(|folder| folder.is_absolute());
        let base = if cfg!(windows) {
            absolute("LOCALAPPDATA")
        } else {
            absolute("XDG_CACHE_HOME").or_else(|| absolute("HOME").map(|home| home.join(".cache")))
        };
        match base {
            Some(base) => Ok(Cache::new(base.join("constructo"))),
            None => Err(ProjectError::Cannot(
                "Cannot find a folder for the cache; give one with --cache, or as `cache` in the [global] table of the \
                 configuration file, or set CONSTRUCTO_CACHE"
                    .to_string(),
            )),
        }
    }

    /// The folder where the checkout of a commit of a repository is, or would be.
    pub fn checkout_path(&self, url: &str, commit: &str) -> PathBuf {
        self.root.join("git").join(url_key(url)).join(commit)
    }

    /// The folder of the checkout of the commit, if it is in the cache. A commit that is not well-formed is never in
    /// it, so a bad one from a lock file cannot be made into a path that leaves the cache.
    pub fn lookup(&self, url: &str, commit: &str) -> Option<PathBuf> {
        if !is_commit(commit) {
            return None;
        }
        Some(self.checkout_path(url, commit)).filter(|path| path.is_dir())
    }

    /// Fetches the repository, finds the commit that `rev` names, which is a tag, a branch or a commit, or the head of
    /// the default branch without one, and puts its files in the cache unless they are there already. The repository
    /// is cloned in full, since a commit cannot always be asked for by its hash, and then `rev-parse` works out the
    /// commit in every case, including that of an annotated tag.
    pub fn fetch(&self, url: &str, rev: Option<&str>) -> Result<Fetched, ProjectError> {
        validate_url(url)
            .map_err(|reason| ProjectError::Cannot(format!("Cannot use the repository {}: {}", redact(url), reason)))?;
        let rev = rev.unwrap_or("HEAD");
        if rev.is_empty() || rev.starts_with('-') || rev.chars().any(|c| c.is_control()) {
            return Err(ProjectError::Cannot(format!(
                "Cannot use the revision \"{}\" of {}: it is empty, or starts with '-', or has a control character",
                rev, url
            )));
        }
        let git_folder = self.root.join("git");
        Scratch::sweep(&git_folder);
        let work = Scratch::new(&git_folder)?;
        let clone = work.0.join("clone");
        // Whether the configuration of the user says how to run ssh. `git config --get` fails when it does not, and a
        // git that cannot be run fails here too, which the clone then reports. It is run in the folder of this run,
        // which is not a repository, so only the system and user files are read, unless the cache is itself inside
        // a repository, whose configuration is then the user's own as well.
        let ssh_configured = git(
            &["config", "--get", "core.sshCommand"],
            Some(&work.0),
            url,
            Config::User { ssh_configured: true },
        )
        .is_ok();
        git(
            // No template, so that a folder of templates in the configuration of the user puts nothing in the clone.
            &[
                "clone",
                "--no-checkout",
                "--quiet",
                "--template=",
                "--",
                url,
                &path_text(&clone),
            ],
            None,
            url,
            Config::User { ssh_configured },
        )?;
        // A clone has a branch of its own only for the default one. The others are the branches of the origin, so a
        // name that is not found as it is, is tried as one of those.
        let find = |name: &str| {
            git(
                &["rev-parse", "--verify", "--quiet", &format!("{}^{{commit}}", name)],
                Some(&clone),
                url,
                Config::Isolated,
            )
        };
        let found = find(rev)
            .or_else(|_| find(&format!("origin/{}", rev)))
            .map_err(|_| ProjectError::Cannot(format!("The repository {} has no revision \"{}\"", url, rev)))?;
        let commit = found.trim().to_string();
        if !is_commit(&commit) {
            return Err(ProjectError::Cannot(format!(
                "Git gave \"{}\" for the revision \"{}\" of {}, which is not a commit",
                commit, rev, url
            )));
        }
        let path = self.checkout_path(url, &commit);
        // The files go in a work tree apart from the clone, so that the checkout has no `.git` folder.
        let tree = work.0.join("tree");
        std::fs::create_dir(&tree).map_err(|e| cannot(&tree, "make", e))?;
        git(
            &[
                "--git-dir",
                &path_text(&clone.join(".git")),
                "--work-tree",
                &path_text(&tree),
                "checkout",
                "--quiet",
                "--detach",
                &commit,
            ],
            None,
            url,
            Config::Isolated,
        )?;
        // A checkout in the cache is reused, but only if it is what a fresh one is. The lock records the digests of
        // what is in the cache, so a file that was changed there would be recorded as the content of the commit.
        if path.is_dir() {
            return match differences(&tree, &path) {
                None => Ok(Fetched { commit, path }),
                Some(difference) => Err(ProjectError::Cannot(format!(
                    "The checkout of the commit {} of {} in the cache has been changed: {}. Remove the folder {} and \
                     run `constructo project update` again",
                    commit,
                    redact(url),
                    difference,
                    path.display()
                ))),
            };
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| cannot(parent, "make", e))?;
        }
        match std::fs::rename(&tree, &path) {
            Ok(()) => {}
            // Another run may have put the same commit there first, which is as good.
            Err(_) if path.is_dir() => {}
            Err(e) => return Err(cannot(&path, "make", e)),
        }
        Ok(Fetched { commit, path })
    }
}

/// What a folder holds at a path: a folder, a file with the digest of its bytes, or a link with its target.
#[derive(PartialEq)]
enum Entry {
    Folder,
    File(String),
    Link(PathBuf),
}

/// Every path under the folder, written with `/` and relative to it, with what is at it. Links are not followed.
fn entries(folder: &Path) -> std::io::Result<std::collections::BTreeMap<String, Entry>> {
    fn visit(root: &Path, folder: &Path, found: &mut std::collections::BTreeMap<String, Entry>) -> std::io::Result<()> {
        for item in std::fs::read_dir(folder)? {
            let item = item?;
            let path = item.path();
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let kind = item.file_type()?;
            if kind.is_symlink() {
                found.insert(relative, Entry::Link(std::fs::read_link(&path)?));
            } else if kind.is_dir() {
                found.insert(relative, Entry::Folder);
                visit(root, &path, found)?;
            } else {
                let digest = Sha256::digest(std::fs::read(&path)?);
                found.insert(
                    relative,
                    Entry::File(digest.iter().map(|byte| format!("{:02x}", byte)).collect()),
                );
            }
        }
        Ok(())
    }
    let mut found = std::collections::BTreeMap::new();
    visit(folder, folder, &mut found)?;
    Ok(found)
}

/// How the folder in the cache differs from a fresh checkout, or `None` if it does not. The first difference by path
/// is described. A folder that cannot be read is a difference too.
fn differences(fresh: &Path, cached: &Path) -> Option<String> {
    let (fresh, cached) = match (entries(fresh), entries(cached)) {
        (Ok(fresh), Ok(cached)) => (fresh, cached),
        (_, Err(e)) => return Some(format!("it cannot be read ({})", e)),
        (Err(e), _) => return Some(format!("the fresh checkout cannot be read ({})", e)),
    };
    let paths: std::collections::BTreeSet<&String> = fresh.keys().chain(cached.keys()).collect();
    for path in paths {
        match (fresh.get(path), cached.get(path)) {
            (Some(a), Some(b)) if a == b => {}
            (Some(_), Some(_)) => return Some(format!("{} is not what it was", path)),
            (Some(_), None) => return Some(format!("{} is missing", path)),
            (None, _) => return Some(format!("{} is not in the commit", path)),
        }
    }
    None
}

fn path_text(path: &Path) -> String {
    path.display().to_string()
}

/// Which configuration of git a call may use, and so which variables of the environment it is given.
#[derive(PartialEq, Clone, Copy, Debug)]
pub(crate) enum Config {
    /// The system and user files, as for any use of git. Fetching needs them, for the credential helper, the settings
    /// of ssh and a proxy. `ssh_configured` says whether the configuration of git says how to run ssh, with
    /// `core.sshCommand`. If neither it nor the environment does, ssh is run in batch mode (see `BATCH_SSH`).
    User { ssh_configured: bool },
    /// No system or user file. A call that only reads a clone that is already here needs nothing from them, and a
    /// repository is somebody else's: its `.gitattributes` can name a filter, such as `lfs`, and a checkout would run
    /// the program that the configuration of the user gives for it, before the checks on the files are made, and so
    /// give a different checkout on a machine that has no such program.
    Isolated,
}

/// How ssh is run when the user has not said. In batch mode ssh never asks for anything on the terminal, such as a
/// passphrase or whether to trust a host it has not seen, and fails instead, since a fetch is not a conversation. A
/// passphrase comes from the agent of ssh, and a host is trusted by connecting to it once with ssh.
const BATCH_SSH: &str = "ssh -o BatchMode=yes";

/// The file that is always empty.
const NULL_DEVICE: &str = if cfg!(windows) { "NUL" } else { "/dev/null" };

/// The variables that every call of git is passed from the environment of `constructo`: what is needed to find and run
/// programs at all. `PATH` is how git finds ssh and the helpers that a configuration names.
const PASSED_ALWAYS: &[&str] = if cfg!(windows) {
    &["PATH", "PATHEXT", "SYSTEMROOT", "COMSPEC", "TEMP", "TMP"]
} else {
    &["PATH", "TMPDIR"]
};

/// The variables that a call with `Config::User` is passed as well, which are what reaching a repository needs:
///
/// - the home folder, where git finds the configuration of the user and with it the credential helper;
/// - the agent of ssh, and the three variables of git that say how to run ssh or ask for a password;
/// - the proxy, in the forms that curl reads (it ignores `HTTP_PROXY` in capitals on purpose), and the certificates of
///   the authorities that OpenSSL is to trust.
const PASSED_FOR_FETCHING: &[&str] = &[
    "HOME",
    "XDG_CONFIG_HOME",
    "USERPROFILE",
    "HOMEDRIVE",
    "HOMEPATH",
    "APPDATA",
    "LOCALAPPDATA",
    "SSH_AUTH_SOCK",
    "GIT_SSH",
    "GIT_SSH_COMMAND",
    "GIT_ASKPASS",
    "http_proxy",
    "https_proxy",
    "HTTPS_PROXY",
    "all_proxy",
    "ALL_PROXY",
    "no_proxy",
    "NO_PROXY",
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
];

/// The whole environment of a call of git, given a function that reads a variable of the environment of
/// `constructo`. Git starts with nothing else, so a variable that is not named here cannot reach it: in particular not
/// `GIT_DIR`, `GIT_WORK_TREE` and `GIT_INDEX_FILE`, which would point it at another repository, nor `GIT_EXEC_PATH`
/// and `GIT_CONFIG_*`, which would make it run other programs, nor the locale, so that git speaks the same on every
/// machine. The variables that git is given a value for come after the ones that are passed, and win over them.
pub(crate) fn child_environment(
    config: Config,
    get: impl Fn(&str) -> Option<OsString>,
) -> Vec<(&'static str, OsString)> {
    let passed = match config {
        Config::User { .. } => [PASSED_ALWAYS, PASSED_FOR_FETCHING].concat(),
        Config::Isolated => PASSED_ALWAYS.to_vec(),
    };
    // The order in which git looks for how to run ssh is `GIT_SSH_COMMAND`, `core.sshCommand`, then `GIT_SSH`, so
    // the default is only given when none of them is there, and it never overrides a choice of the user.
    let batch = config == Config::User { ssh_configured: false }
        && get("GIT_SSH_COMMAND").is_none()
        && get("GIT_SSH").is_none();
    let mut environment: Vec<(&'static str, OsString)> = passed
        .into_iter()
        .filter_map(|name| get(name).map(|value| (name, value)))
        .collect();
    let mut set = |name: &'static str, value: &str| environment.push((name, OsString::from(value)));
    set("GIT_ALLOW_PROTOCOL", ALLOWED_PROTOCOLS);
    // Without this git waits for a password to be typed, which nobody can see in a run of a tool.
    set("GIT_TERMINAL_PROMPT", "0");
    if batch {
        set("GIT_SSH_COMMAND", BATCH_SSH);
    }
    if config == Config::Isolated {
        // No `HOME` is passed either, for versions of git before 2.32, which do not know `GIT_CONFIG_GLOBAL`.
        set("GIT_CONFIG_NOSYSTEM", "1");
        set("GIT_CONFIG_GLOBAL", NULL_DEVICE);
        set("GIT_ATTR_NOSYSTEM", "1");
    }
    environment
}

/// Runs git with the arguments, in the folder if there is one, and returns what it printed. Its environment is the one
/// that `child_environment` gives, and nothing else, and its settings are `SETTINGS`. A git that is not installed is
/// an error that says so.
fn git(args: &[&str], folder: Option<&Path>, url: &str, config: Config) -> Result<String, ProjectError> {
    let mut command = Command::new("git");
    for setting in SETTINGS {
        command.arg("-c").arg(setting);
    }
    command.args(args);
    if let Some(folder) = folder {
        command.current_dir(folder);
    }
    let environment = child_environment(config, |name| std::env::var_os(name));
    let batch = environment
        .iter()
        .any(|(name, value)| *name == "GIT_SSH_COMMAND" && value == BATCH_SSH);
    command.env_clear().envs(environment);
    let output = command.output().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            ProjectError::Cannot(
                "Cannot run git: it is not installed, and projects from a repository need it".to_string(),
            )
        } else {
            ProjectError::Cannot(format!("Cannot run git: {}", e))
        }
    })?;
    if !output.status.success() {
        let mut message = format!(
            "Git failed for {}: {}",
            url,
            failure(&String::from_utf8_lossy(&output.stderr))
        );
        if batch && is_ssh(url) {
            message.push_str(
                " (ssh is run in batch mode, so it cannot ask for a passphrase or whether to trust the host: use the \
                 agent of ssh, connect to the host with ssh once, or say how to run ssh with core.sshCommand or \
                 GIT_SSH_COMMAND)",
            );
        }
        return Err(ProjectError::Cannot(message));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// What git said when it failed, on one line: the lines up to the first that starts with `fatal:` or `error:`. The
/// cause, such as `Host key verification failed.` from ssh, often comes before that line, and advice after it.
fn failure(stderr: &str) -> String {
    let mut said = Vec::new();
    for line in stderr.lines().map(str::trim).filter(|line| !line.is_empty()) {
        said.push(line);
        if line.starts_with("fatal:") || line.starts_with("error:") {
            break;
        }
    }
    if said.is_empty() {
        "it failed".to_string()
    } else {
        said.join(" ")
    }
}

/// Whether git reaches the repository by ssh: an `ssh://` URL, or the short form `[USER@]HOST:PATH`, which has a colon
/// before any slash that is not after the letter of a drive.
fn is_ssh(url: &str) -> bool {
    if let Some((scheme, _)) = url.split_once("://") {
        return scheme == "ssh";
    }
    let head = url.split('/').next().unwrap_or(url);
    head.contains(':') && !is_drive(head)
}

/// Whether the text starts with the letter of a drive and a colon, as in `C:`.
fn is_drive(head: &str) -> bool {
    head.len() >= 2 && head.as_bytes()[0].is_ascii_alphabetic() && head.as_bytes()[1] == b':'
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::Repo;

    #[test]
    fn the_urls_that_can_be_used() {
        for good in [
            "https://github.com/example/lib.git",
            "https://example.com",
            "ssh://git@example.com/example/lib.git",
            "ssh://example.com:2222/lib",
            "git@github.com:example/lib.git",
            "example.com:lib",
            "file:///srv/repos/lib",
            "/srv/repos/lib",
            "../lib",
            "lib",
            "C:/repos/lib",
            "C:\\repos\\lib",
            "./a:b/lib",
        ] {
            assert_eq!(validate_url(good), Ok(()), "{}", good);
        }
    }

    #[test]
    fn the_urls_that_are_refused_say_why() {
        let refused = |url: &str| validate_url(url).expect_err(url);
        assert!(refused("").contains("empty"));
        assert!(refused("--upload-pack=touch x").contains("starts with '-'"));
        assert!(refused("-x").contains("option"));
        assert!(refused("ext::sh -c touch% x").contains("space"));
        assert!(refused("ext::sh").contains("remote helper"));
        assert!(refused("fd::3").contains("remote helper"));
        assert!(refused("git://example.com/lib").contains("git://"));
        assert!(refused("http://example.com/lib").contains("http://"));
        assert!(refused("ftp://example.com/lib").contains("ftp://"));
        assert!(refused("https://").contains("no host"));
        assert!(refused("https:///lib").contains("no host"));
        assert!(refused("ssh://").contains("no host"));
        assert!(refused("ssh://git@/lib").contains("no host"));
        assert!(refused("file://").contains("no path"));
        assert!(refused("lib\n").contains("control"));
        assert!(refused("a b").contains("space"));
    }

    #[test]
    fn a_secret_in_a_url_is_refused_and_is_not_repeated() {
        for url in [
            "https://user:s3cret@example.com/lib",
            "https://token@example.com/lib",
            "ssh://user:s3cret@example.com/lib",
            "user:s3cret@example.com:lib",
        ] {
            let reason = validate_url(url).expect_err(url);
            assert!(reason.contains("credential helper"), "{}", reason);
            assert!(!reason.contains("s3cret") && !reason.contains("token"), "{}", reason);
        }
        // A user name alone is how ssh is reached, so it is fine there.
        assert_eq!(validate_url("ssh://git@example.com/lib"), Ok(()));
    }

    #[test]
    fn a_secret_is_left_out_of_a_url_that_is_shown() {
        assert_eq!(redact("https://user:pw@example.com/lib"), "https://***@example.com/lib");
        assert_eq!(redact("user:pw@example.com:lib"), "***@example.com:lib");
        assert_eq!(redact("https://example.com/a@b"), "https://example.com/a@b");
        assert_eq!(redact("/srv/lib"), "/srv/lib");
    }

    #[test]
    fn a_commit_is_forty_or_sixty_four_lowercase_hexadecimal_digits() {
        assert!(is_commit(&"a".repeat(40)));
        assert!(is_commit(&"0123456789abcdef".repeat(4)));
        for bad in [
            "",
            "abc",
            &"a".repeat(41),
            &"A".repeat(40),
            &"g".repeat(40),
            "../../etc/passwd",
        ] {
            assert!(!is_commit(bad), "{}", bad);
        }
    }

    #[test]
    fn the_key_of_a_url_is_sixteen_digits_that_depend_on_the_url_alone() {
        let key = url_key("https://example.com/lib.git");
        assert_eq!(key.len(), 16);
        assert!(key.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
        assert_eq!(key, url_key("https://example.com/lib.git"));
        assert_ne!(key, url_key("https://example.com/lib"));
        // The first eight bytes of the SHA-256 of the text, which `sha256sum` gives as well.
        assert_eq!(url_key(""), "e3b0c44298fc1c14");
    }

    #[test]
    fn the_cache_is_where_the_environment_says() {
        let from = |pairs: &[(&str, &str)]| {
            let pairs: Vec<(String, PathBuf)> = pairs.iter().map(|(k, v)| (k.to_string(), PathBuf::from(v))).collect();
            Cache::from_variables(move |name| pairs.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone()))
        };
        // An absolute folder on this platform, and the same folder with the parts after it.
        let root = if cfg!(windows) { "C:\\c" } else { "/c" };
        assert_eq!(
            from(&[("CONSTRUCTO_CACHE", root), ("XDG_CACHE_HOME", "/x")])
                .unwrap()
                .root,
            Path::new(root)
        );
        // A relative CONSTRUCTO_CACHE is refused, and says which variable it is.
        let error = from(&[("CONSTRUCTO_CACHE", "cache"), ("HOME", root)])
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("CONSTRUCTO_CACHE is cache, which is not an absolute path"),
            "{}",
            error
        );
        if !cfg!(windows) {
            assert_eq!(
                from(&[("XDG_CACHE_HOME", "/x"), ("HOME", "/h")]).unwrap().root,
                Path::new("/x/constructo")
            );
            assert_eq!(from(&[("HOME", "/h")]).unwrap().root, Path::new("/h/.cache/constructo"));
            // A relative XDG_CACHE_HOME is ignored, as the XDG specification says, and so is a relative HOME.
            assert_eq!(
                from(&[("XDG_CACHE_HOME", "x"), ("HOME", "/h")]).unwrap().root,
                Path::new("/h/.cache/constructo")
            );
            let error = from(&[("HOME", "h")]).unwrap_err().to_string();
            assert!(error.contains("give one with --cache"), "{}", error);
            assert!(from(&[]).is_err());
        }
    }

    #[test]
    fn a_cache_that_is_given_comes_before_the_environment() {
        assert_eq!(
            Cache::resolve(Some(Path::new("given"))).unwrap().root,
            Path::new("given")
        );
    }

    #[test]
    fn the_checkout_of_a_commit_has_a_stable_path() {
        let cache = Cache::new(PathBuf::from("/c"));
        let commit = "a".repeat(40);
        assert_eq!(
            cache.checkout_path("https://example.com/lib", &commit),
            Path::new("/c/git")
                .join(url_key("https://example.com/lib"))
                .join(&commit)
        );
        // A commit that is not well-formed is never looked up, whatever is on disk.
        assert_eq!(cache.lookup("https://example.com/lib", "../../x"), None);
    }

    fn cache() -> (Cache, Scratch) {
        let scratch = Scratch::new(&std::env::temp_dir()).unwrap();
        (Cache::new(scratch.0.join("cache")), scratch)
    }

    /// Sets the modified time of a file or a folder to longer ago than `STALE_AFTER`.
    fn make_stale(path: &Path) {
        let old = std::time::SystemTime::now() - STALE_AFTER - std::time::Duration::from_secs(60);
        let file = if path.is_dir() {
            std::fs::File::open(path)
        } else {
            std::fs::OpenOptions::new().write(true).open(path)
        };
        file.unwrap().set_modified(old).unwrap();
    }

    #[test]
    fn a_scratch_folder_has_a_lock_file_and_a_name_of_its_own() {
        let (_cache, scratch) = cache();
        let parent = scratch.0.join("git");
        let first = Scratch::new(&parent).unwrap();
        let second = Scratch::new(&parent).unwrap();
        assert_ne!(first.0, second.0);
        assert!(first.0.join(SCRATCH_LOCK).is_file() && second.0.join(SCRATCH_LOCK).is_file());
    }

    #[test]
    fn a_name_that_is_taken_is_skipped_and_what_has_it_is_left_alone() {
        let (_cache, scratch) = cache();
        let parent = scratch.0.join("git");
        let taken = parent.join("tmp-taken");
        std::fs::create_dir_all(&taken).unwrap();
        std::fs::write(taken.join("theirs"), "x").unwrap();
        let mut names = ["tmp-taken", "tmp-free"].into_iter().map(String::from);
        let made = Scratch::make(&parent, || names.next().unwrap(), HEARTBEAT).unwrap();
        assert_eq!(made.0, parent.join("tmp-free"));
        assert!(taken.join("theirs").is_file() && !taken.join(SCRATCH_LOCK).exists());
    }

    #[test]
    fn a_scratch_folder_is_swept_when_its_lock_file_is_stale() {
        let (_cache, scratch) = cache();
        let parent = scratch.0.join("git");
        let live = Scratch::new(&parent).unwrap();
        let folder = |name: &str, lock: bool| {
            let path = parent.join(name);
            std::fs::create_dir(&path).unwrap();
            if lock {
                std::fs::write(path.join(SCRATCH_LOCK), "").unwrap();
            }
            path
        };
        // A stale lock file is what counts, even in a folder whose own time is new.
        let stale_lock = folder("tmp-stale-lock", true);
        make_stale(&stale_lock.join(SCRATCH_LOCK));
        // A lock file that is new keeps a folder whose own time is old.
        let fresh_lock = folder("tmp-fresh-lock", true);
        make_stale(&fresh_lock);
        // With no lock file, the time of the folder decides.
        let old_bare = folder("tmp-old-bare", false);
        make_stale(&old_bare);
        let new_bare = folder("tmp-new-bare", false);
        // Only a folder with the prefix is ever swept.
        let unrelated = folder("unrelated", true);
        make_stale(&unrelated.join(SCRATCH_LOCK));
        make_stale(&unrelated);
        Scratch::sweep(&parent);
        assert!(!stale_lock.exists() && !old_bare.exists());
        assert!(fresh_lock.is_dir() && new_bare.is_dir() && unrelated.is_dir() && live.0.is_dir());
    }

    #[test]
    fn a_scratch_folder_in_use_keeps_its_lock_file_young() {
        let (_cache, outer) = cache();
        let parent = outer.0.join("git");
        let scratch = Scratch::new_with(&parent, std::time::Duration::from_millis(20)).unwrap();
        let lock = scratch.0.join(SCRATCH_LOCK);
        make_stale(&lock);
        // Within a short time the heartbeat has renewed the lock file, so a sweep leaves the folder.
        let young = (0..100).any(|_| {
            std::thread::sleep(std::time::Duration::from_millis(50));
            std::fs::metadata(&lock)
                .and_then(|meta| meta.modified())
                .is_ok_and(|modified| modified.elapsed().is_ok_and(|age| age < STALE_AFTER))
        });
        assert!(young, "the heartbeat should have renewed the lock file");
        Scratch::sweep(&parent);
        assert!(scratch.0.is_dir());
    }

    #[test]
    fn a_checkout_in_the_cache_that_was_changed_is_refused() {
        let repo = Repo::new();
        repo.commit("a.types", "enum A = X;\n");
        repo.commit("sub/b.types", "enum B = Y;\n");
        let (cache, _scratch) = cache();
        let url = repo.url();
        let first = cache.fetch(&url, None).unwrap();
        // A second fetch of the same commit finds the checkout as it was.
        assert_eq!(cache.fetch(&url, None).unwrap(), first);
        let refused = |what: &str| {
            let error = cache.fetch(&url, None).unwrap_err().to_string();
            assert!(error.contains("in the cache has been changed"), "{}", error);
            assert!(error.contains(what), "{}", error);
            assert!(error.contains(&first.path.display().to_string()), "{}", error);
            assert!(error.contains("constructo project update"), "{}", error);
        };
        // A file that is changed.
        std::fs::write(first.path.join("a.types"), "enum A = Z;\n").unwrap();
        refused("a.types is not what it was");
        std::fs::write(first.path.join("a.types"), "enum A = X;\n").unwrap();
        // A file that is added.
        std::fs::write(first.path.join("extra.types"), "").unwrap();
        refused("extra.types is not in the commit");
        std::fs::remove_file(first.path.join("extra.types")).unwrap();
        // A file that is removed.
        std::fs::remove_file(first.path.join("sub/b.types")).unwrap();
        refused("sub/b.types is missing");
        std::fs::write(first.path.join("sub/b.types"), "enum B = Y;\n").unwrap();
        assert_eq!(cache.fetch(&url, None).unwrap(), first);
    }

    /// The environment of a call of git, as a map, given the variables of the environment of `constructo`.
    fn environment_of(config: Config, outside: &[(&str, &str)]) -> std::collections::BTreeMap<String, String> {
        let get = |name: &str| outside.iter().find(|(k, _)| *k == name).map(|(_, v)| OsString::from(v));
        child_environment(config, get)
            .into_iter()
            .map(|(name, value)| (name.to_string(), value.to_string_lossy().to_string()))
            .collect()
    }

    const OUTSIDE: [(&str, &str); 11] = [
        ("PATH", "/bin"),
        ("HOME", "/home/u"),
        ("SSH_AUTH_SOCK", "/run/agent"),
        ("https_proxy", "http://proxy:8080"),
        ("GIT_SSH_COMMAND", "ssh -i key"),
        ("GIT_DIR", "/elsewhere/.git"),
        ("GIT_EXEC_PATH", "/elsewhere/libexec"),
        ("GIT_CONFIG_GLOBAL", "/elsewhere/config"),
        ("GIT_ALLOW_PROTOCOL", "ext"),
        ("LANG", "fr_FR.UTF-8"),
        ("SOMETHING_ELSE", "x"),
    ];

    #[test]
    fn a_fetch_is_given_what_reaching_a_repository_needs_and_nothing_else() {
        let environment = environment_of(Config::User { ssh_configured: false }, &OUTSIDE);
        for name in ["PATH", "HOME", "SSH_AUTH_SOCK", "https_proxy", "GIT_SSH_COMMAND"] {
            assert_eq!(
                environment.get(name).map(String::as_str),
                OUTSIDE.iter().find(|(k, _)| *k == name).map(|(_, v)| *v),
                "{}",
                name
            );
        }
        for name in [
            "GIT_DIR",
            "GIT_EXEC_PATH",
            "GIT_CONFIG_GLOBAL",
            "LANG",
            "SOMETHING_ELSE",
        ] {
            assert!(!environment.contains_key(name), "{} should not be passed", name);
        }
        // The values that git is given win over any that the environment has.
        assert_eq!(environment["GIT_ALLOW_PROTOCOL"], ALLOWED_PROTOCOLS);
        assert_eq!(environment["GIT_TERMINAL_PROMPT"], "0");
        assert!(!environment.contains_key("GIT_CONFIG_NOSYSTEM"));
    }

    #[test]
    fn a_call_that_only_reads_the_clone_has_no_configuration_and_no_home() {
        let environment = environment_of(Config::Isolated, &OUTSIDE);
        let names: Vec<&str> = environment.keys().map(String::as_str).collect();
        assert_eq!(
            names,
            [
                "GIT_ALLOW_PROTOCOL",
                "GIT_ATTR_NOSYSTEM",
                "GIT_CONFIG_GLOBAL",
                "GIT_CONFIG_NOSYSTEM",
                "GIT_TERMINAL_PROMPT",
                "PATH"
            ]
        );
        assert_eq!(environment["GIT_CONFIG_GLOBAL"], NULL_DEVICE);
    }

    #[test]
    fn a_variable_that_is_not_set_is_not_passed() {
        let environment = environment_of(Config::User { ssh_configured: false }, &[]);
        let names: Vec<&str> = environment.keys().map(String::as_str).collect();
        assert_eq!(names, ["GIT_ALLOW_PROTOCOL", "GIT_SSH_COMMAND", "GIT_TERMINAL_PROMPT"]);
    }

    #[test]
    fn ssh_is_run_in_batch_mode_unless_the_user_says_how_to_run_it() {
        let ssh =
            |config: Config, outside: &[(&str, &str)]| environment_of(config, outside).get("GIT_SSH_COMMAND").cloned();
        let fetching = Config::User { ssh_configured: false };
        assert_eq!(ssh(fetching, &[]).as_deref(), Some(BATCH_SSH));
        // The choice of the user wins, whether it is in the environment or in the configuration of git.
        assert_eq!(
            ssh(fetching, &[("GIT_SSH_COMMAND", "ssh -i k")]).as_deref(),
            Some("ssh -i k")
        );
        assert_eq!(ssh(fetching, &[("GIT_SSH", "/usr/bin/plink")]), None);
        assert_eq!(ssh(Config::User { ssh_configured: true }, &[]), None);
        // A call that only reads the clone does not use ssh at all.
        assert_eq!(ssh(Config::Isolated, &[]), None);
    }

    #[test]
    fn the_message_of_git_keeps_the_cause_and_leaves_out_the_advice() {
        let stderr = "Host key verification failed.\r\nfatal: Could not read from remote repository.\n\nPlease make \
                      sure you have the correct access rights\nand the repository exists.\n";
        assert_eq!(
            failure(stderr),
            "Host key verification failed. fatal: Could not read from remote repository."
        );
        assert_eq!(failure("warning: x\n"), "warning: x");
        assert_eq!(failure(""), "it failed");
    }

    #[test]
    fn a_url_that_git_reaches_by_ssh_is_told_apart() {
        for url in [
            "ssh://git@example.com/lib",
            "git@github.com:example/lib.git",
            "example.com:lib",
        ] {
            assert!(is_ssh(url), "{}", url);
        }
        for url in [
            "https://example.com/lib",
            "file:///srv/lib",
            "/srv/lib",
            "../lib",
            "C:/repos/lib",
            "./a:b/lib",
        ] {
            assert!(!is_ssh(url), "{}", url);
        }
    }

    #[test]
    fn a_branch_a_tag_and_the_default_are_found_as_commits() {
        let repo = Repo::new();
        let first = repo.commit("a.types", "enum A = X;\n");
        repo.git(&["tag", "v1"]);
        repo.git(&["tag", "--annotate", "--message", "one", "v1-annotated"]);
        let second = repo.commit("b.types", "enum B = Y;\n");
        repo.git(&["branch", "side", &first]);
        let (cache, _scratch) = cache();
        let url = repo.url();
        assert_eq!(cache.fetch(&url, None).unwrap().commit, second);
        assert_eq!(cache.fetch(&url, Some("main")).unwrap().commit, second);
        assert_eq!(cache.fetch(&url, Some("v1")).unwrap().commit, first);
        // An annotated tag is a tag object, and the commit is what it points to.
        assert_eq!(cache.fetch(&url, Some("v1-annotated")).unwrap().commit, first);
        assert_eq!(cache.fetch(&url, Some("side")).unwrap().commit, first);
        assert_eq!(cache.fetch(&url, Some(&first)).unwrap().commit, first);
        // A commit can be given by a unique start of it.
        assert_eq!(cache.fetch(&url, Some(&second[..10])).unwrap().commit, second);
    }

    #[test]
    fn the_files_of_the_commit_are_in_the_cache_without_a_git_folder() {
        let repo = Repo::new();
        let first = repo.commit("sub/a.types", "enum A = X;\n");
        repo.commit("sub/b.types", "enum B = Y;\n");
        let (cache, _scratch) = cache();
        let url = repo.url();
        let fetched = cache.fetch(&url, Some(&first)).unwrap();
        assert_eq!(fetched.path, cache.checkout_path(&url, &first));
        assert_eq!(
            std::fs::read_to_string(fetched.path.join("sub/a.types")).unwrap(),
            "enum A = X;\n"
        );
        // The commit after it is not there, and nor is the history.
        assert!(!fetched.path.join("sub/b.types").exists());
        assert!(!fetched.path.join(".git").exists());
        assert_eq!(cache.lookup(&url, &first), Some(fetched.path.clone()));
        assert_eq!(cache.lookup(&url, &"0".repeat(40)), None);
        // Fetching it again finds it there and leaves it alone, unless it has been changed.
        assert_eq!(cache.fetch(&url, Some(&first)).unwrap(), fetched);
        std::fs::write(fetched.path.join("marker"), "x").unwrap();
        assert!(cache.fetch(&url, Some(&first)).is_err());
        assert!(
            fetched.path.join("marker").exists(),
            "a changed checkout is refused and not repaired"
        );
        std::fs::remove_file(fetched.path.join("marker")).unwrap();
        assert_eq!(cache.fetch(&url, Some(&first)).unwrap(), fetched);
        // Nothing is left behind next to the checkouts but the folders of the repositories.
        let names: Vec<_> = std::fs::read_dir(cache.root.join("git"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(names, vec![url_key(&url)]);
    }

    #[test]
    fn a_revision_that_does_not_exist_is_an_error_that_names_it() {
        let repo = Repo::new();
        repo.commit("a.types", "enum A = X;\n");
        let (cache, _scratch) = cache();
        let message = cache.fetch(&repo.url(), Some("nothing-here")).unwrap_err().to_string();
        assert!(message.contains("has no revision \"nothing-here\""), "{}", message);
    }

    #[test]
    fn a_repository_that_does_not_exist_is_an_error_that_names_it() {
        let (cache, scratch) = cache();
        let url = scratch.0.join("nowhere").display().to_string();
        let message = cache.fetch(&url, None).unwrap_err().to_string();
        assert!(message.contains("Git failed for"), "{}", message);
        assert!(message.contains(&url), "{}", message);
    }

    #[test]
    fn a_url_that_is_refused_is_not_given_to_git() {
        let (cache, _scratch) = cache();
        let message = cache.fetch("ext::sh -c true", None).unwrap_err().to_string();
        assert!(message.contains("Cannot use the repository"), "{}", message);
        let message = cache
            .fetch("https://user:s3cret@example.com/lib", None)
            .unwrap_err()
            .to_string();
        assert!(message.contains("https://***@example.com/lib"), "{}", message);
        assert!(!message.contains("s3cret"), "{}", message);
        let message = cache
            .fetch("/nowhere", Some("--upload-pack=x"))
            .unwrap_err()
            .to_string();
        assert!(message.contains("starts with '-'"), "{}", message);
    }
}
