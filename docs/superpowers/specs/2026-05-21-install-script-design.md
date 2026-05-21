# Install Script Design

**Date:** 2026-05-21
**Topic:** curl-based install script for `pgrs`

## Overview

Provide a one-liner install experience for `pgrs` via a shell script hosted in the repository:

```bash
curl -fsSL https://raw.githubusercontent.com/pgrsst/pgrs/main/install.sh | bash
```

## Scope

- Platform: Linux only
- Install location: `~/.pgrs/bin/pgrs`
- PATH setup: auto-append to `.bashrc` and `.zshrc` if they exist
- Script location: `install.sh` at repo root, committed to git

## Script Flow

1. **Validate environment** — check `curl` is available, check OS is Linux
2. **Detect latest version** — fetch `https://api.github.com/repos/pgrsst/pgrs/releases/latest`, parse tag name
3. **Download binary** — `https://github.com/pgrsst/pgrs/releases/download/v{VERSION}/pgrs`
4. **Install binary** — create `~/.pgrs/bin/` if needed, move binary, `chmod +x`
5. **Setup PATH** — if `~/.pgrs/bin` not already in PATH, append export line to `.bashrc` and `.zshrc` (only if those files exist)
6. **Print summary** — version installed, remind user to `source ~/.bashrc`

## Error Handling

| Condition | Action |
|-----------|--------|
| `curl` not found | Print error, exit 1 |
| OS is not Linux | Print "only Linux is supported", exit 1 |
| GitHub API request fails | Print error with status, exit 1 |
| Binary download fails | Delete partial file, print error, exit 1 |
| `~/.pgrs/bin` already in PATH | Skip PATH setup silently |

## Output Format

```
Installing pgrs v0.1.2...
  Downloading binary...
  Installing to ~/.pgrs/bin/pgrs
  Adding ~/.pgrs/bin to PATH in ~/.bashrc
  Adding ~/.pgrs/bin to PATH in ~/.zshrc

pgrs v0.1.2 installed successfully!
Run 'source ~/.bashrc' to update your current shell.
```

## Files Changed

| File | Action |
|------|--------|
| `install.sh` | New — shell script at repo root |
| `README.md` | Add "Installation" section with one-liner |

## URLs

- Script: `https://raw.githubusercontent.com/pgrsst/pgrs/main/install.sh`
- Binary: `https://github.com/pgrsst/pgrs/releases/download/v{VERSION}/pgrs`
- GitHub API: `https://api.github.com/repos/pgrsst/pgrs/releases/latest`

## Out of Scope

- macOS support (future work)
- Windows support
- Custom install directory override
- Package manager integration (apt, brew, etc.)
