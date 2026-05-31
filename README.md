# cliup

`cliup` is a small macOS CLI tool that updates a whitelisted set of developer and AI command line tools. It supports tools installed by global npm packages, Homebrew formulae, Homebrew casks, and trusted self-update commands such as `claude update`.

`launchd` only triggers `cliup run` on a schedule. Day-to-day package management stays in `cliup add`, `cliup remove`, and `cliup list`.

## Install

```sh
cargo install --path .
```

## Initialize

```sh
cliup init
```

This creates:

- `~/.cliup/config.json`
- `~/.cliup/logs/`

## Add Tools

```sh
cliup add npm @openai/codex
cliup add npm @anthropic-ai/claude-code
cliup add npm @jackwener/opencli
cliup add brew pi-coding-agent
cliup add cask visual-studio-code
cliup add self claude claude update
```

For `self`, the first name is the command used for existence checks, and the remaining arguments are saved as the trusted update command.

## List Tools

```sh
cliup list
```

Example output:

```text
npm    @openai/codex
brew   pi-coding-agent
cask   visual-studio-code
self   claude -> claude update
```

## Remove Tools

```sh
cliup remove @openai/codex
cliup remove visual-studio-code
cliup remove claude
```

`remove` deletes every configured package whose `name` matches.

## Change Schedule

```sh
cliup schedule 10 30
cliup install-launchd
```

The schedule uses 24-hour time. `hour` must be `0..23`, and `minute` must be `0..59`.

## Manual Update

```sh
cliup run
```

## Dry Run

```sh
cliup run --dry-run
```

Dry-run mode prints and logs the update commands that would run, but does not execute the update commands.

## Install launchd

```sh
cliup install-launchd
```

This reads `schedule.hour` and `schedule.minute` from `~/.cliup/config.json`, creates `~/.cliup/bin/cliup-run.sh`, and installs:

```text
~/Library/LaunchAgents/com.user.cliup.plist
```

## Status

```sh
cliup status
```

## Logs

```sh
cliup log
cliup log -n 200
```

Update logs are written to:

```text
~/.cliup/logs/update.log
```

launchd stdout/stderr logs are written to:

```text
~/.cliup/logs/launchd.out.log
~/.cliup/logs/launchd.err.log
```

## Doctor

```sh
cliup doctor
```

`doctor` prints the current `cliup` binary path, PATH, config/log status, tool versions, launchctl availability, and installed/missing status for configured packages.

## Uninstall launchd

```sh
cliup uninstall-launchd
```

This unloads and removes the launchd plist. It does not delete `config.json` or logs.

## Notes

- Do not add both npm and brew versions of the same tool unless you intentionally want both entries.
- `cliup` only updates already installed tools. Missing npm/brew/cask packages are skipped and are not installed automatically.
- `self` commands are treated as trusted local commands and are executed through `sh -lc`.
- External command PATH is launchd-safe and prepends `/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin`.

## Local Test Steps

```sh
cargo build
cargo install --path .
cliup init
cliup add npm @jackwener/opencli
cliup list
cliup doctor
cliup run --dry-run
cliup schedule 10 30
cliup install-launchd
cliup status
cliup log
```
