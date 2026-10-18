//! v14 session control. Encoded bytes never authorize a target or launch an app.
use crate::{invalid, video::Binding, WindowTarget};
use serde::{Deserialize, Serialize};
use std::io;
pub const VERSION: u16 = 14;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Parameters {
    pub target: WindowTarget,
    pub binding: Binding,
    pub width: u16,
    pub height: u16,
    pub fps: u8,
}
impl Parameters {
    pub fn validate(&self) -> io::Result<()> {
        self.binding.validate()?;
        if self.target.handle == 0
            || self.target.pid == 0
            || self.target.created == 0
            || self.target.incarnation == 0
            || self.width == 0
            || self.height == 0
            || self.width > 1920
            || self.height > 1080
            || !self.width.is_multiple_of(2)
            || !self.height.is_multiple_of(2)
            || self.fps != 30
        {
            return Err(invalid("Invalid video target or output bounds"));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    Start { parameters: Parameters },
    Stop { binding: Binding },
    Keyframe { binding: Binding },
}
impl Operation {
    pub fn binding(&self) -> Binding {
        match self {
            Self::Start { parameters } => parameters.binding,
            Self::Stop { binding } | Self::Keyframe { binding } => *binding,
        }
    }
    pub fn validate(&self) -> io::Result<()> {
        match self {
            Self::Start { parameters } => parameters.validate(),
            _ => self.binding().validate(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum Reply {
    Started { parameters: Parameters },
    Stopped { binding: Binding },
    KeyframeRequested { binding: Binding },
}
impl Reply {
    pub fn validate_for(&self, operation: &Operation) -> io::Result<()> {
        operation.validate()?;
        let matches = match (self, operation) {
            (
                Self::Started { parameters: got },
                Operation::Start {
                    parameters: expected,
                },
            ) => got == expected,
            (Self::Stopped { binding: got }, Operation::Stop { binding: expected })
            | (
                Self::KeyframeRequested { binding: got },
                Operation::Keyframe { binding: expected },
            ) => got == expected,
            _ => false,
        };
        if !matches {
            return Err(invalid(
                "Video reply does not match authorized operation/session",
            ));
        }
        Ok(())
    }
}
