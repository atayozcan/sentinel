// SPDX-FileCopyrightText: 2026 Atay Özcan <atay@oezcan.me>
// SPDX-License-Identifier: GPL-3.0-or-later
//! Fuzz the broker IPC frame reader. `read_frame` parses bytes off a
//! Unix socket *inside the root broker*, so it must never panic and must
//! never allocate on a hostile length prefix (the `MAX_FRAME_LEN` guard
//! runs before the body buffer is sized).
#![no_main]

use libfuzzer_sys::fuzz_target;
use sentinel_broker_proto::{Request, read_frame};

fuzz_target!(|data: &[u8]| {
    let mut cur = std::io::Cursor::new(data);
    // Drain successive frames until the input is exhausted or errors —
    // exercises the length-prefix + postcard decode path repeatedly.
    while read_frame::<_, Request>(&mut cur).is_ok() {}
});
