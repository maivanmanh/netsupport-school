use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Teacher,
    Student,
}

impl fmt::Display for Role {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Teacher => formatter.write_str("teacher"),
            Self::Student => formatter.write_str("student"),
        }
    }
}

impl FromStr for Role {
    type Err = ParseRoleError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "teacher" => Ok(Self::Teacher),
            "student" => Ok(Self::Student),
            _ => Err(ParseRoleError(value.to_owned())),
        }
    }
}

#[derive(Debug, Error)]
#[error("invalid role '{0}'; expected teacher or student")]
pub struct ParseRoleError(String);

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PresentationSource {
    LocalTeacherDisplay { display_index: u32 },
    RemoteStudentDisplay { student_id: u64, display_index: u32 },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_parsing_is_case_insensitive() {
        assert_eq!("Teacher".parse::<Role>().unwrap(), Role::Teacher);
        assert!("observer".parse::<Role>().is_err());
    }
}
