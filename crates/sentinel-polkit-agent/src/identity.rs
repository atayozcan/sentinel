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

#[cfg(test)]
mod tests {
    use super::*;
    use zvariant::Value;

    fn unix_user(uid: u32) -> Identity {
        let mut details: HashMap<String, OwnedValue> = HashMap::new();
        details.insert("uid".to_string(), Value::U32(uid).try_to_owned().unwrap());
        ("unix-user".to_string(), details)
    }

    fn unix_group(gid: u32) -> Identity {
        let mut details: HashMap<String, OwnedValue> = HashMap::new();
        details.insert("gid".to_string(), Value::U32(gid).try_to_owned().unwrap());
        ("unix-group".to_string(), details)
    }

    #[test]
    fn prefers_matching_uid_even_when_listed_later() {
        let ids = vec![unix_user(0), unix_user(1000), unix_user(1001)];
        assert_eq!(pick(&ids, 1000), Some(1000));
    }

    #[test]
    fn falls_back_to_first_unix_user_if_no_uid_match() {
        let ids = vec![unix_user(1000), unix_user(1001)];
        assert_eq!(pick(&ids, 9999), Some(1000));
    }

    #[test]
    fn skips_non_unix_user_kinds() {
        let ids = vec![unix_group(0), unix_user(1000)];
        assert_eq!(pick(&ids, 1000), Some(1000));
    }

    #[test]
    fn returns_none_for_empty_identities() {
        assert_eq!(pick(&[], 1000), None);
    }

    #[test]
    fn returns_none_when_only_non_unix_user() {
        let ids = vec![unix_group(100), unix_group(101)];
        assert_eq!(pick(&ids, 1000), None);
    }

    #[test]
    fn skips_entries_missing_uid_field() {
        let no_uid: Identity = ("unix-user".to_string(), HashMap::new());
        let ids = vec![no_uid, unix_user(1000)];
        assert_eq!(pick(&ids, 1000), Some(1000));
    }
}
