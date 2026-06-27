// SPDX-FileCopyrightText: 2026 Atay Özcan <atay@oezcan.me>
// SPDX-License-Identifier: GPL-3.0-or-later
//! Fuzz `format_message`, the `%u`/`%s`/`%p`/`%%` token substitution
//! applied to admin-supplied `message`/`secondary`/`title` templates
//! before they're shown in the dialog. Must never panic on a trailing
//! `%`, an unknown `%x`, or multibyte boundaries near a `%`.
#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use sentinel_shared::format_message;

#[derive(Arbitrary, Debug)]
struct Inputs {
    template: String,
    user: String,
    service: String,
    process: String,
}

fuzz_target!(|inp: Inputs| {
    let _ = format_message(&inp.template, &inp.user, &inp.service, &inp.process);
});
