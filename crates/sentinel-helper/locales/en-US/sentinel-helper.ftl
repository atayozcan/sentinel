# sentinel-helper UI chrome — strings the helper renders itself.
#
# The dialog title/message/secondary are admin-provided through
# /etc/security/sentinel.conf, but if they still equal the built-in
# defaults (the strings registered in `sentinel-shared` as
# `DEFAULT_TITLE`/`DEFAULT_MESSAGE`/`DEFAULT_SECONDARY`), the helper
# substitutes the locale-specific version below. Customizing the
# config opts out of this — admin's strings are passed through.
dialog-title-default     = Authentication Required
dialog-message-default   = The application "{$process}" is requesting elevated privileges.
dialog-secondary-default = Click "Allow" to continue or "Deny" to cancel.

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
