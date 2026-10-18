# Release Credentials

This document covers the one-time setup required on each workstation before
running `just draft-release` or `just publish-release`.

## crates.io (trusted publisher — no token required)

`cargo publish` runs in GitHub Actions via the trusted publisher mechanism.
No crates.io API token is needed on the local machine or in GitHub secrets.

The first publish of a brand-new crate may need to be done another way, because
there is no crate settings page to configure until the crate exists. Check the
current crates.io documentation on trusted publishing before the first release.

One-time setup on crates.io (per crate, not per machine):
1. Log in at [crates.io](https://crates.io).
2. Go to the `constructo` crate → **Settings → Trusted Publishing**.
3. Add a trusted publisher: repository `sfkleach/constructo`, workflow
   `release-publish.yml`.

## SSH signing key

Required for `git tag -s` (signed release tags). Git has supported SSH keys for
signing since version 2.34; GitHub also needs the public key added as a
**Signing Key** (Settings → SSH and GPG keys) to show "Verified".

Quick check — confirm git is configured to use SSH signing:

```bash
git config --global gpg.format        # should show: ssh
git config --global user.signingkey   # should show path to your .pub file
git config --global commit.gpgsign    # should show: true
git config --global tag.gpgsign       # should show: true
```

If any of these are missing, set them with `git config --global`, using your
SSH public key (for example `~/.ssh/id_ed25519.pub`) for `user.signingkey` and
`ssh` for `gpg.format`. Then test with:

```bash
git tag -s test-sign -m "test" && git tag -v test-sign && git tag -d test-sign
```

## GitHub CLI authentication

`just draft-release` and `just publish-release` use `gh`. Confirm it is
authenticated:

```bash
gh auth status
```

If not, run:

```bash
gh auth login
```
