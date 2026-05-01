//! Pick which `unix-user` identity to authenticate as from polkit's list of
//! eligible identities.

use std::collections::HashMap;
use zvariant::OwnedValue;

pub type Identity = (String, HashMap<String, OwnedValue>);

/// Pick the best identity to authenticate as. Strategy: prefer the
/// non-root unix-user matching the calling process's uid; fall back to the
/// first unix-user; otherwise return None.
pub fn pick(identities: &[Identity], own_uid: u32) -> Option<u32> {
    let mut first_unix_user: Option<u32> = None;
    for (kind, details) in identities {
        if kind != "unix-user" {
            continue;
        }
        let Some(uid_val) = details.get("uid") else {
            continue;
        };
        let Ok(uid): Result<u32, _> = uid_val.try_into() else {
            continue;
        };
        if uid == own_uid {
            return Some(uid);
        }
        if first_unix_user.is_none() {
            first_unix_user = Some(uid);
        }
    }
    first_unix_user
}
