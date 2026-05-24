# Troubleshooting

## "I can't log in / `sudo` doesn't work" — recovery

`pkexec ./uninstall.sh` from your second root shell undoes everything
the installer did, restoring `.pre-sentinel.bak` files in place.

If both `sudo` and `pkexec` are broken:
1. Boot to a TTY (Ctrl+Alt+F2 typically).
2. Log in as root, or sudo into a recovery shell.
3. Restore manually:
   ```bash
   mv /etc/pam.d/sudo.pre-sentinel.bak /etc/pam.d/sudo
   mv /etc/pam.d/polkit-1.pre-sentinel.bak /etc/pam.d/polkit-1
   rm /usr/lib/security/pam_sentinel.so
   ```

## The dialog never appears

**Check the agent is registered:**
```bash
pgrep -fxa /usr/lib/sentinel-polkit-agent
journalctl -t sentinel-polkit-agent --since "5 minutes ago" --no-pager
```

You should see:
```
agent socket listening at /run/user/1000/sentinel-agent.sock
registered as polkit auth agent (object path /com/github/sentinel/PolkitAgent)
```

**If "another agent is registered, retrying":** something else
(cosmic-osd, polkit-gnome, polkit-kde) is winning the registration
race. The install script tries to kill them; on COSMIC,
`cosmic-session` will respawn `cosmic-osd` aggressively. Workaround:

```bash
pkexec chmod -x /usr/bin/cosmic-osd
```

(Loses brightness/volume OSDs but keeps Sentinel as the sole polkit
agent.)

**If the agent is running but the dialog still doesn't show:**
likely the compositor doesn't implement `zwlr-layer-shell-v1`.
Sentinel auto-falls-back to xdg-toplevel on GNOME/Mutter, but force
it with:

```bash
sentinel-helper --windowed --title test --message hi
```

## `pkexec` shows "Error executing command as another user: Not authorized"

That's pkexec's standard error after a failed auth — including a
clean Deny click in Sentinel. The "incident has been reported" line
is hardcoded in pkexec(1). Sentinel can't suppress it from the
agent side; polkit doesn't differentiate "user politely declined"
from "auth failed" in its protocol.

## `sudo true` shows "sudo-rs" in the dialog instead of "true"

Make sure you're on v0.6.1+ (this was fixed in that release). Earlier
versions read `/proc/<sudo-pid>/exe` without stripping the elevation
wrapper.

```bash
sentinel-helper --version
```

## `sudo -v` (or topgrade / paru cred-cache) shows "sudo-rs" not the
##  parent process

Fixed in v0.7.0. When the elevation wrapper has no target argv
(`sudo -v`), the PAM module walks up to PPid and uses the parent's
exe.

## Dialog appears but the wrong language

Sentinel's helper reads `LC_ALL` → `LC_MESSAGES` → `LANG` to pick
its embedded locale bundle. PAM modules typically run inside
privileged binaries (sudo, helper-1) that scrub `LANG` from their
env, so the helper recovers locale variables from
`/proc/<requesting-pid>/environ` against a strict allowlist.

Check the test:
```bash
LANG=tr_TR.UTF-8 pkexec true
```

Should render the Turkish dialog. If it doesn't:

- Check `/proc/<your-shell-pid>/environ` actually has `LANG=...`
- Check the locale tag is one of the 12 shipped: en-US, de-DE,
  es-ES, fr-FR, it-IT, ja-JP, nl-NL, pl-PL, pt-BR, ru-RU, tr-TR,
  zh-CN. Other tags fall back to en-US.

## I want more verbose logs

The agent supports `--debug`:

```bash
pkill -fx /usr/lib/sentinel-polkit-agent
/usr/lib/sentinel-polkit-agent --debug &
```

The `--debug` mode dumps `details[…]` from every
`BeginAuthentication` call — useful for diagnosing process-name
display bugs.

The PAM module always logs at INFO level under syslog identifier
`pam_sentinel` (AUTH facility). Get all auth events from the last
5 minutes:

```bash
journalctl -t pam_sentinel -t sentinel-polkit-agent \
    --since "5 minutes ago" --no-pager | grep "event=auth"
```

## Reporting bugs

[`bug_report.yml`](https://github.com/atayozcan/sentinel/issues/new?template=bug_report.yml)
is the standard template. For compositor compatibility specifically,
the [compositor compat template](https://github.com/atayozcan/sentinel/issues/new?template=compositor_compat.yml)
feeds the README compatibility table directly.

For security issues, use private vulnerability reporting — see
[security policy](./security.md).
