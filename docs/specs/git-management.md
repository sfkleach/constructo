# How git is run

A project can come from a git repository (see [the project-file](project-file-format.md)). `constructo` does not
have git built in. It runs the `git` program, and this document says exactly how: when it runs, which commands, with
which settings, in which environment, and what a repository can and cannot make it do. The cache that the commands
fill is described in [the cache](cache-management.md). Decision 30 in `docs/decisions/` gives the reasons in brief.
The code is in `src/fetch.rs`.

## What is trusted

- **The repository is somebody else's.** Its files, its `.gitattributes`, its `.gitmodules`, its links and the names
  of its branches and tags are all treated as hostile. The aim is that nothing in it can run a program, reach the
  network, read or write outside the folders that `constructo` gives git, or make a checkout differ from one machine
  to another.
- **The user is trusted.** The user's git configuration, ssh configuration, credential helper and proxy are the
  user's own, and are used where reaching a repository needs them, and nowhere else.
- **The environment of `constructo` is not passed on wholesale.** Git is given only the variables that are named
  below, so that a variable that nobody thought of cannot change what it does.

## When git is run

Only `constructo project update` runs git. `constructo check` never does: it takes the commit from the lock and the
files from the cache, and with no lock, no entry or no checkout it stops and says to run `update`. So a `check` never
touches the network and never runs a program other than itself.

`update` fetches once for each different pair of a `repo` and a `rev` that the projects import, however many imports
name the pair.

## Before git is run

The values that come from a project-file are checked first, so that git is never given one that it could take for
something else.

- **The URL** must be one of `https://HOST/PATH`, `ssh://[USER@]HOST/PATH`, the short form `[USER@]HOST:PATH`,
  `file://PATH`, or a path to a folder. Anything else is refused, in particular `ext::` and the other remote helpers,
  which run a program, and `git://`, `http://` and `ftp://`, which are not secure. A URL that starts with `-`, or has
  a space or a control character, is refused. So is one with a password, or with a user name on `https`, which is
  usually a token, since the URL is written in the lock file and in messages.
- **The revision** may not be empty, start with `-` or have a control character. Without a `rev` it is `HEAD`, the
  default branch of the repository.
- **Every command** that takes the URL has `--` before it, so that it is an argument and never an option, whatever
  is in it.

A problem with the URL or the revision is an error at the import, with exit code 2, and git is not run.

## The commands

`update` works in a scratch folder of its own in the cache, `git/tmp-…`, which is removed when the run ends. For each
repository it runs these commands. Only the second reaches the network, and only the first two read the
configuration of the user.

1. **Asking how ssh is run**, in the scratch folder:

   ```sh
   git config --get core.sshCommand
   ```

   It only reads a setting, and fails, which is not an error, when it is not set. See [ssh](#ssh).

2. **The clone:**

   ```sh
   git clone --no-checkout --quiet --template= -- URL SCRATCH/clone
   ```

   It clones the whole repository, since a commit cannot always be fetched by its hash. It checks nothing out, so no
   file of the commit is written and no attribute of the repository is applied. `--template=` stops a folder of
   templates that the configuration of the user names from being copied into the clone.

3. **Finding the commit**, in the clone:

   ```sh
   git rev-parse --verify --quiet REV^{commit}
   ```

   `^{commit}` makes the result a commit in every case: a branch, a tag, an annotated tag (whose object is not the
   commit), or a commit, in full or abbreviated. The result must be 40 hexadecimal digits, or 64 in a repository that
   uses SHA-256, or it is an error.

4. **A branch that is not the default one.** A clone has a branch of its own only for the default branch. The others
   are `origin/NAME`, so a name that step 3 cannot find is tried again as `origin/REV`. If neither finds it, the error
   is `The repository URL has no revision "REV"`.

5. **The checkout:**

   ```sh
   git --git-dir SCRATCH/clone/.git --work-tree SCRATCH/tree checkout --quiet --detach COMMIT
   ```

   The files of the commit are written into a folder apart from the clone, so that the checkout has no `.git` folder
   and holds the files of the commit and nothing else. Submodules are not checked out.

Then the checkout is compared with the one in the cache, if there is one, or renamed into the cache, as
[the cache](cache-management.md) says. The clone is thrown away with the scratch folder.

## The settings of every call

Every call is given these with `-c`. A setting given so wins over every configuration file.

- `core.symlinks=true`: a link is checked out as a link, whatever the configuration of the user or the probe of the
  file system says. A file system that cannot make links fails the checkout, instead of giving a plain file that holds
  the target, whose digest would differ from the one made on another machine.
- `core.hooksPath=/dev/null`: no hook is run, from the configuration of the user, a template or anywhere else. The
  folder has no hooks in it.
- `core.autocrlf=false`: the line endings of a file are not changed by a setting of the user. The repository's own
  `.gitattributes` still apply; see [what a repository can still affect](#what-a-repository-can-still-affect).
- `core.fsmonitor=false`: no file system monitor, which is a program, is started.
- `protocol.file.allow=always`: a repository that is a local folder is allowed whatever the configuration says. Which
  transports are allowed at all is set by `GIT_ALLOW_PROTOCOL`.
- `advice.detachedHead=false`: no advice is printed about the detached checkout.

## The environment

Git starts with an empty environment. It is given the variables below from the environment of `constructo`, those that
are set, and then the ones that `constructo` sets itself, which win over any of the same name. Nothing else reaches
it. The list is `child_environment` in `src/fetch.rs`.

**Passed to every call:**

- `PATH`, and `TMPDIR`. On Windows: `PATH`, `PATHEXT`, `SYSTEMROOT`, `COMSPEC`, `TEMP` and `TMP`, which any program
  needs there. `PATH` is how git finds ssh and the helpers that a configuration names, and how `constructo` finds git.

**Passed to the calls that use the configuration of the user** (steps 1 and 2) as well:

- `HOME` and `XDG_CONFIG_HOME`, and on Windows `USERPROFILE`, `HOMEDRIVE`, `HOMEPATH`, `APPDATA` and `LOCALAPPDATA`:
  where the git configuration of the user is, and with it the credential helper.
- `SSH_AUTH_SOCK`: the agent of ssh, which holds the keys.
- `GIT_SSH`, `GIT_SSH_COMMAND` and `GIT_ASKPASS`: how the user runs ssh, or asks for a password.
- `http_proxy`, `https_proxy`, `HTTPS_PROXY`, `all_proxy`, `ALL_PROXY`, `no_proxy` and `NO_PROXY`: a proxy, in the
  forms that curl reads. Curl ignores `HTTP_PROXY` in capitals on purpose.
- `SSL_CERT_FILE` and `SSL_CERT_DIR`: the certificates of the authorities to trust.

**Set by `constructo`:**

- `GIT_ALLOW_PROTOCOL=https:ssh:file`, for every call: no other transport, whatever the URL or the configuration says,
  so a rewrite of the URL by `url.*.insteadOf` cannot change the transport either.
- `GIT_TERMINAL_PROMPT=0`, for every call: git never waits for a user name or a password to be typed.
- `GIT_SSH_COMMAND=ssh -o BatchMode=yes`, for the clone, if the user has not said how to run ssh. See [ssh](#ssh).
- `GIT_CONFIG_NOSYSTEM=1`, for steps 3 to 5: no system configuration.
- `GIT_CONFIG_GLOBAL`, set to the null device, for steps 3 to 5: no user configuration. With no `HOME` either, a git
  older than 2.32, which does not know this variable, finds none too.
- `GIT_ATTR_NOSYSTEM=1`, for steps 3 to 5: no system file of attributes.

**Not passed, on purpose:**

- `GIT_DIR`, `GIT_WORK_TREE`, `GIT_INDEX_FILE` and the like, which would point git at another repository. Git sets
  them when it runs a hook, so a `constructo` that is run from a hook would otherwise work on the wrong repository.
- `GIT_EXEC_PATH`, `GIT_CONFIG_*` and the like, which would make git run other programs or read other files.
- `GIT_SSL_NO_VERIFY`, which would switch off the check of certificates, and `GIT_SSL_CAINFO`. A file of certificates
  that is needed goes in the git configuration, as `http.sslCAInfo`.
- `SSH_ASKPASS` and `DISPLAY`, so ssh does not open a window to ask for anything.
- The locale, so that git's messages are the same on every machine.

A user who keeps their git configuration somewhere else with `GIT_CONFIG_GLOBAL` has it left out of the clone too, and
then has to give the credential helper or the proxy in the usual files.

## The user's configuration

Only steps 1 and 2 read the user's configuration, and the system's. Step 2 needs them to reach the repository: the
credential helper, `core.sshCommand`, the settings of a proxy and of certificates, and `url.*.insteadOf`. They are the
user's own, so they are trusted. Even so, the settings above win over them, and the clone checks nothing out, so no
filter, hook or monitor that a configuration names is run.

Steps 3 to 5 read no configuration but that of the clone itself, which the clone wrote. They are where the
repository's files are handled, and a configuration could otherwise name a program that the repository asks for. The
case that matters is a filter: a `.gitattributes` in the repository can say `filter=lfs`, or any other name, and a
checkout would run the program that the user's configuration gives for that name. That would run a program for
somebody else's repository, before `constructo` has looked at any file, and it would make the checkout differ between
a machine that has the program and one that has not.

## ssh

ssh can ask questions on the terminal, such as a passphrase or whether to trust a host that it has not seen. A fetch
that waits for an answer can hang a build. So, unless the user has said how to run ssh, git is given
`GIT_SSH_COMMAND=ssh -o BatchMode=yes`, and ssh fails instead of asking.

Git looks for how to run ssh in this order: `GIT_SSH_COMMAND`, then `core.sshCommand` in the configuration, then
`GIT_SSH`. The default is given only when none of the three is set, which is why step 1 asks git for
`core.sshCommand`, so that it never overrides what the user chose. Step 1 runs in the scratch folder, which is not a
repository, so it reads the system and user files only, unless the cache is itself inside a repository, whose
configuration is then the user's own too.

With batch mode, a passphrase comes from the agent of ssh, and a host is trusted by connecting to it once with ssh.
When an ssh fetch fails in batch mode, the message says that it was in batch mode and what to do.

## What a repository can still affect

- **Its attributes of line endings and encoding.** A `.gitattributes` that says `text`, `eol=crlf`, `ident` or
  `working-tree-encoding` is applied by the checkout, as it is part of how the repository says its files are written
  out. The digest of a file is taken after a CRLF is made LF and a byte order mark removed (see
  [the lock file](lock-file-format.md)), so a checkout with CRLF on Windows has the same digest as one with LF.
- **Its links.** They are checked out as links. Whether they stay inside the clone is then checked by `constructo`
  before any file is read, and for every link that a project selects, whether or not its target exists. See
  [the project-file](project-file-format.md).
- **Its size.** The clone is of the whole repository, every time. There is no limit on how long it takes or how much
  it holds.

It cannot:

- run a hook, since none is run;
- run a filter, a monitor or a remote helper, since none is configured in steps 3 to 5 and the URL is checked first;
- bring in a submodule, since submodules are neither cloned nor checked out. A submodule is an empty folder in the
  checkout;
- fetch a file stored with Git LFS. Such a file is checked out as the small pointer file that is in the repository,
  so a types file kept in LFS is read as its pointer, which is then an error in that file.

## Errors

Every failure of git is an error at the `[[import]]`, with exit code 2, and nothing is written to the lock.

- A git that is not installed: `Cannot run git: it is not installed, and projects from a repository need it`.
- A command that fails: `Git failed for URL: …`, with what git printed on its standard error, on one line, up to and
  including the first line that starts with `fatal:` or `error:`. The cause, such as ssh's
  `Host key verification failed.`, is usually before that line, and git's advice after it.
- A revision that is not there: `The repository URL has no revision "REV"`.

A URL is shown as it is written. One with a password or a token in it is refused before anything is run, so a message
never shows a secret.

## Versions and platforms

- The commands and settings are tested with git 2.43 on Linux.
- A git older than 2.32 does not know `GIT_CONFIG_GLOBAL`, and is kept from the files of the user by having no `HOME`.
  A git older than 2.36 may take `core.fsmonitor` to be the name of a program, and so run the program `false`, which
  does nothing. No oldest version has been established or tested.
- On Windows the null device is `NUL`, the variables passed are the ones listed above, and `core.hooksPath=/dev/null`
  is a folder that does not exist, which has no hooks either. None of this is tested on Windows.

## Related

- [The cache](cache-management.md), for where the checkouts go and how runs share them.
- [The lock file](lock-file-format.md), for what `update` records and `check` verifies.
- [The project-file](project-file-format.md), for `repo`, `rev` and `subdir`.
- Decisions 29 and 30, in `docs/decisions/`.
