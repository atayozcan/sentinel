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
