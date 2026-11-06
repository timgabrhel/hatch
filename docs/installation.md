# Installation and updates

[← Back to README](../README.md)

Run this on your **Linux Docker host**:

```sh
curl -fsSL https://raw.githubusercontent.com/timgabrhel/hatch/main/scripts/install.sh | sh
```

The installer detects x86_64 or ARM64, downloads the latest GitHub release, verifies
its checksum and executable version, and installs Hatch in `~/.local/bin`.
Rust is not required. Downloads require curl, tar, sha256sum, and compatible Linux libraries.

If the install directory is missing from PATH, Hatch offers to add it to your Bash,
Zsh, or POSIX shell profile. Existing PATH entries are left alone. Profile changes
require confirmation; open a new terminal afterward to pick them up.

If the Dev Container CLI is missing and Node.js/npm are available, the installer
offers to install it too. This enables configuration resolution and container startup.
Existing containers can also run using Docker metadata without the resolver.

## Update

```sh
hatch update
```

Updates the running Hatch installation in place. Hatch checks the latest release,
skips an equal or older version, verifies the download, and replaces the binary
atomically. It does not update container agents, modify shell profiles, or launch a
session. A failed download or verification leaves the existing binary intact.

## Installer options

Most users can accept the installer prompts. For scripts or custom installations:

| Option | Purpose |
|---|---|
| `--no-input` | Skip setup prompts and leave shell profiles unchanged. |
| `--with-resolver` | Install the pinned Dev Container CLI; requires Node.js/npm. |
| `--without-resolver` | Skip resolver setup. |
| `--prefix DIR` | Install under an absolute prefix instead of `~/.local`. |
| `--bin FILE` | Install a locally built executable. |
| `--no-launch` | Skip launch after a local install. Downloaded installs always finish without launching. |

Pass options to the piped installer with `sh -s --`:

```sh
curl -fsSL https://raw.githubusercontent.com/timgabrhel/hatch/main/scripts/install.sh | sh -s -- --no-input --with-resolver
```

The managed resolver lives under `PREFIX/share/hatch/resolver`; an existing
`PREFIX/bin/devcontainer` is preserved. Hatch leaves `cf` and devcontainer configuration alone.

## Get an archive

[GitHub Releases](https://github.com/timgabrhel/hatch/releases) also provides versioned
archives, checksums, documentation, and license notices. Each archive includes
`scripts/install.sh` for local installation. Archives are native GNU/Linux builds,
not universal static binaries.

See [development](development.md#build-from-source) for source builds.
