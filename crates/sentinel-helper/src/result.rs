use std::fmt;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Outcome {
    Allow,
    Deny,
    Timeout,
}

impl fmt::Display for Outcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Allow => "ALLOW",
            Self::Deny => "DENY",
            Self::Timeout => "TIMEOUT",
        };
        f.write_str(s)
    }
}

impl Outcome {
    pub fn exit_code(self) -> i32 {
        match self {
            Self::Allow => 0,
            _ => 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allow_exits_zero() {
        assert_eq!(Outcome::Allow.exit_code(), 0);
    }

    #[test]
    fn deny_and_timeout_exit_nonzero() {
        // Both are "user said no" from PAM's perspective.
        assert_eq!(Outcome::Deny.exit_code(), 1);
        assert_eq!(Outcome::Timeout.exit_code(), 1);
    }

    #[test]
    fn display_strings_are_stable_protocol() {
        // The PAM module greps stdout for these exact strings — bumping
        // them is a wire-protocol break.
        assert_eq!(Outcome::Allow.to_string(), "ALLOW");
        assert_eq!(Outcome::Deny.to_string(), "DENY");
        assert_eq!(Outcome::Timeout.to_string(), "TIMEOUT");
    }
}
