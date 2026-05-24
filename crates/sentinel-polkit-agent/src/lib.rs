// SPDX-FileCopyrightText: 2025 Atay Özcan <atay@oezcan.me>
// SPDX-License-Identifier: GPL-3.0-or-later
//! Library surface for `sentinel-polkit-agent`.
//!
//! The agent ships as a `[[bin]]` (`src/main.rs`); this `lib.rs`
//! re-exports the same modules under a library target so integration
//! tests in `tests/` can drive the session state machine without
//! shelling out to the binary or talking to D-Bus.
//!
//! Production code paths land here too (the bin's `main.rs` declares
//! the modules with `mod foo;` for backwards compatibility, but rust
//! lets the same source files be compiled both ways without
//! duplication when both targets share `src/`).

pub mod agent;
pub mod approval_queue;
pub mod authority;
pub mod helper1;
pub mod helper_ui;
pub mod identity;
pub mod session;
pub mod socket_server;
pub mod subject;
