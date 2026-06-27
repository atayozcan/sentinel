// SPDX-FileCopyrightText: 2026 Atay Özcan <atay@oezcan.me>
// SPDX-License-Identifier: GPL-3.0-or-later
//! Fuzz the `Verdict` wire parser.
//!
//! The verdict string crosses the helper -> backend trust boundary: the
//! PAM module and the polkit agent both parse the helper's stdout into a
//! `Verdict`. Properties:
//!   1. parsing arbitrary bytes never panics (it returns `Result`);
//!   2. the canonical `Display` form is a *stable fixed point* of
//!      parse∘display — re-emitting a parsed verdict and re-parsing it
//!      yields the same canonical string. (Note `DENY REMEMBER`
//!      canonicalizes to `DENY`, so this is stability, not identity.)
#![no_main]

use libfuzzer_sys::fuzz_target;
use sentinel_shared::Verdict;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    if let Ok(v) = s.parse::<Verdict>() {
        let printed = v.to_string();
        let reparsed = printed
            .parse::<Verdict>()
            .expect("a canonical verdict string must re-parse");
        assert_eq!(
            printed,
            reparsed.to_string(),
            "Verdict Display must be a stable fixed point of parse∘display"
        );
    }
});
