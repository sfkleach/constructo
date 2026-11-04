//! Fetching a project from a git repository into a cache, as planned in
//! `docs/tasks/2026-10-21-projects/plan-step06.md`.
//!
//! Only `constructo project update` fetches. `check` looks in the cache for the commit that the lock names and never
//! goes to the network. The work is done by running `git`, which is treated with care, since the repository is
//! somebody else's: its transports are limited, no hook or program named by a configuration file is run, a URL cannot
//! be taken for an option, and a URL that holds a password is refused so that it is never recorded or printed.

use crate::project::ProjectError;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

/// The transports that git may use, which is also what `GIT_ALLOW_PROTOCOL` is set to.
const ALLOWED_PROTOCOLS: &str = "https:ssh:file";

/// The variables of the environment whose name starts with `GIT_` that are kept, since they say how to reach the
/// repository and are the user's own. Every other one is removed, because it could redirect git to another repository
/// or another program.
const KEPT_VARIABLES: [&str; 3] = ["GIT_SSH", "GIT_SSH_COMMAND", "GIT_ASKPASS"];

/// The settings that every call of git is given, so that a checkout is the same on every machine and runs nothing
/// that a configuration file names.
const SETTINGS: [&str; 5] = [
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
    let is_drive = head.len() >= 2 && head.as_bytes()[0].is_ascii_alphabetic() && head.as_bytes()[1] == b':';
    if head.contains(':') && !is_drive {
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

/// A folder that is made for a while and removed, with what is in it, when it is dropped.
pub(crate) struct Scratch(pub(crate) PathBuf);

impl Scratch {
    /// Makes a new folder with a name of its own in the folder given.
    pub(crate) fn new(parent: &Path) -> Result<Scratch, ProjectError> {
        static COUNT: AtomicU32 = AtomicU32::new(0);
        std::fs::create_dir_all(parent).map_err(|e| cannot(parent, "make", e))?;
        let name = format!("tmp-{}-{}", std::process::id(), COUNT.fetch_add(1, Ordering::Relaxed));
        let path = parent.join(name);
        std::fs::create_dir(&path).map_err(|e| cannot(&path, "make", e))?;
        Ok(Scratch(path))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
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

    /// The cache that the environment names: `$CONSTRUCTO_CACHE`, or else `constructo` in the cache folder of the user,
    /// which is `$XDG_CACHE_HOME` or `~/.cache`, or `%LOCALAPPDATA%` on Windows.
    pub fn from_environment() -> Result<Cache, ProjectError> {
        Cache::from_variables(|name| {
            std::env::var_os(name)
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        })
    }

    /// As `from_environment`, with the variables given by a function so that it can be tried without changing them.
    fn from_variables(get: impl Fn(&str) -> Option<PathBuf>) -> Result<Cache, ProjectError> {
        if let Some(root) = get("CONSTRUCTO_CACHE") {
            return Ok(Cache::new(root));
        }
        let base = if cfg!(windows) {
            get("LOCALAPPDATA")
        } else {
            get("XDG_CACHE_HOME").or_else(|| get("HOME").map(|home| home.join(".cache")))
        };
        match base {
            Some(base) => Ok(Cache::new(base.join("constructo"))),
            None => Err(ProjectError::Cannot(
                "Cannot find a folder for the cache; set CONSTRUCTO_CACHE to one".to_string(),
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
        let work = Scratch::new(&git_folder)?;
        let clone = work.0.join("clone");
        git(
            &["clone", "--no-checkout", "--quiet", "--", url, &path_text(&clone)],
            None,
            url,
        )?;
        // A clone has a branch of its own only for the default one. The others are the branches of the origin, so a
        // name that is not found as it is, is tried as one of those.
        let find = |name: &str| {
            git(
                &["rev-parse", "--verify", "--quiet", &format!("{}^{{commit}}", name)],
                Some(&clone),
                url,
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
        if path.is_dir() {
            return Ok(Fetched { commit, path });
        }
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
        )?;
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

fn path_text(path: &Path) -> String {
    path.display().to_string()
}

/// Runs git with the arguments, in the folder if there is one, and returns what it printed. The environment and the
/// settings are those described at the top of this module. A git that is not installed is an error that says so.
fn git(args: &[&str], folder: Option<&Path>, url: &str) -> Result<String, ProjectError> {
    let mut command = Command::new("git");
    for setting in SETTINGS {
        command.arg("-c").arg(setting);
    }
    command.args(args);
    if let Some(folder) = folder {
        command.current_dir(folder);
    }
    for (name, _) in std::env::vars_os() {
        let name = name.to_string_lossy().to_string();
        if name.starts_with("GIT_") && !KEPT_VARIABLES.contains(&name.as_str()) {
            command.env_remove(&name);
        }
    }
    command
        .env("GIT_ALLOW_PROTOCOL", ALLOWED_PROTOCOLS)
        // Without this git waits for a password to be typed, which nobody can see in a run of a tool.
        .env("GIT_TERMINAL_PROMPT", "0");
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
        let said = String::from_utf8_lossy(&output.stderr);
        let said = said.trim().lines().last().unwrap_or("it failed");
        return Err(ProjectError::Cannot(format!("Git failed for {}: {}", url, said)));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
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
        assert_eq!(
            from(&[("CONSTRUCTO_CACHE", "/c"), ("XDG_CACHE_HOME", "/x")])
                .unwrap()
                .root,
            Path::new("/c")
        );
        if !cfg!(windows) {
            assert_eq!(
                from(&[("XDG_CACHE_HOME", "/x"), ("HOME", "/h")]).unwrap().root,
                Path::new("/x/constructo")
            );
            assert_eq!(from(&[("HOME", "/h")]).unwrap().root, Path::new("/h/.cache/constructo"));
            assert!(from(&[]).is_err());
        }
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
        // Fetching it again finds it there and leaves it alone.
        std::fs::write(fetched.path.join("marker"), "x").unwrap();
        assert_eq!(cache.fetch(&url, Some(&first)).unwrap(), fetched);
        assert!(fetched.path.join("marker").exists());
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
