# SPDX-FileCopyrightText: 2025 Atay Özcan <atay@oezcan.me>
# SPDX-License-Identifier: GPL-3.0-or-later
# sentinel-helper UI chrome — strings the helper renders itself.
#
# The dialog title/message are admin-provided through
# /etc/security/sentinel.conf, but if they still equal the built-in
# defaults (the strings registered in `sentinel-shared` as
# `DEFAULT_TITLE`/`DEFAULT_MESSAGE`), the helper substitutes the
# locale-specific version below. Customizing the config opts out of
# this — admin's strings are passed through.
#
# `secondary` (an admin-set hint line below the message) has no
# built-in default; if the admin sets one, the helper renders it
# verbatim.
dialog-title-default     = Authentication Required
dialog-message-default   = The application "{$process}" is requesting elevated privileges.

button-allow = Allow
button-deny  = Deny

toggle-show-details = ▸ Show details
toggle-hide-details = ▾ Hide details

# {$seconds} is replaced with the integer count of seconds remaining
# until the dialog auto-denies.
auto-deny-in = Auto-deny in {$seconds}s

detail-command      = Command
detail-pid          = PID
detail-cwd          = Working dir
detail-requested-by = Requested by
detail-action       = Action

# Suffix appended when a long /proc field is clipped for display.
truncated-suffix = … [truncated]