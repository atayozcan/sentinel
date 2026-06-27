// SPDX-FileCopyrightText: 2026 Atay Özcan <atay@oezcan.me>
// SPDX-License-Identifier: GPL-3.0-or-later
//! Fuzz `strip_elevation_prefix`, which parses untrusted
//! `/proc/<pid>/cmdline` to recover the elevated program from a `sudo`/
//! `su`/`pkexec`/`doas` wrapper. It feeds the remember key and the
//! dialog's process name, so it must never panic on any input (embedded
//! NULs, lone flags, multibyte, pathological whitespace, deep nesting).
#![no_main]

use libfuzzer_sys::fuzz_target;
use sentinel_shared::strip_elevation_prefix;

fuzz_target!(|data: &[u8]| {
    // cmdline is NUL-joined in /proc; callers pass it as a str, so fuzz
    // the str path (lossy-decoding arbitrary bytes exercises multibyte).
    let s = String::from_utf8_lossy(data);
    let _ = strip_elevation_prefix(&s);
});
