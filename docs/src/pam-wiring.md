# PAM wiring

Sentinel needs to be referenced from the PAM stacks of whichever
services should trigger the confirmation dialog. The packages wire
`/etc/pam.d/polkit-1` automatically; everything else is opt-in.

> **Always test on a fresh install with a second root shell open.** A
> typo in a PAM file can lock you out of `sudo`. `pkexec bash` keeps
> a working privileged shell available even if the rest of the stack
> breaks.

## polkit (default)

`/etc/pam.d/polkit-1` is owned by Sentinel after a package install:

```
#%PAM-1.0
auth       sufficient pam_sentinel.so
auth       include    system-auth
account    include    system-auth
password   include    system-auth
session    include    system-auth
```

The `sufficient` control means: if Sentinel returns `PAM_SUCCESS`
(user clicked Allow), polkit skips the rest of the stack — no
password needed. Any other return (Deny / timeout / no Wayland)
falls through to `system-auth` which prompts for the password.

## sudo (opt-in)

The package does **not** wire `/etc/pam.d/sudo` automatically — a
mistake there can lock you out of root entirely.

To opt in via the source installer:

```bash
pkexec ./install.sh --enable-sudo
```

To do it manually:

```
# /etc/pam.d/sudo
#%PAM-1.0
auth       sufficient pam_sentinel.so
auth       include    system-auth
account    include    system-auth
password   include    system-auth
session    include    system-auth
```

A `sufficient` Sentinel followed by `system-auth` is the safest
shape: any Sentinel failure (helper crash, missing display)
produces `PAM_AUTH_ERR`, which makes the stack continue to the
password prompt. You're never *prevented* from authenticating;
Sentinel just adds a confirmation step on top.

## sudo-rs

`sudo-rs` reads the same `/etc/pam.d/sudo` stack as `sudo`. No
separate wiring; the steps above cover both.

## su

A `[services.su] enabled = false` block in `sentinel.conf` is the
recommended approach (Sentinel returns `PAM_IGNORE`, su falls
through to password). If you want Sentinel to gate `su` too, mirror
the sudo wiring into `/etc/pam.d/su`.

## What to do if you locked yourself out

The `pkexec bash` from your second root shell is the rescue hatch:

```bash
# In the rescue shell:
pkexec ./uninstall.sh   # restores backed-up /etc/pam.d/* files
```

If both `pkexec` and `sudo` are broken (extremely rare; would
require pam_sentinel.so to crash on every dlopen), boot to a TTY
and edit `/etc/pam.d/{sudo,polkit-1}` by hand to remove the
`pam_sentinel.so` line.

The installer's transactional state file is at
`/var/lib/sentinel/install.state`; every replaced file has a
`.pre-sentinel.bak` copy alongside it. Worst-case manual recovery:

```bash
mv /etc/pam.d/sudo.pre-sentinel.bak /etc/pam.d/sudo
mv /etc/pam.d/polkit-1.pre-sentinel.bak /etc/pam.d/polkit-1
```

## How `sufficient` interacts with the rest of the stack

PAM's `sufficient` control is "if this passes, we're done; if it
fails, keep going". That makes Sentinel a strict additive
confirmation: never weakens auth, only ever ADDS a click.

| Sentinel returns | Stack behaviour |
|------------------|-----------------|
| `PAM_SUCCESS` (Allow) | Skip rest of `auth`, grant access. |
| `PAM_AUTH_ERR` (Deny / timeout / crash) | Continue to next module → password prompt. |
| `PAM_IGNORE` (disabled / headless / fallthrough) | Continue to next module → password prompt. |

There's no configuration where Sentinel returning anything makes
auth *easier* than the underlying password stack. Worst case it's
neutral (you still type your password); best case (Allow) it's a
single click instead of a password.
