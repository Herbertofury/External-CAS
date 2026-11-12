// SPDX-License-Identifier: GPL-3.0-or-later
//! Native, lossless RIG/CLIP intake and explicitly scoped source-tool sampling.
//! Unknown data is retained. Sampling does not execute events or solve IK and
//! does not claim to reproduce the game's interpolation or skeleton selection.
mod clip;
mod matrix;
mod reader;
mod rig;

pub use clip::*;
pub use matrix::Matrix4;
pub use rig::*;

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnimationError {
    Truncated { offset: usize, needed: usize },
    Invalid { offset: usize, reason: &'static str },
    Unsupported(String),
}

impl fmt::Display for AnimationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated { offset, needed } => write!(
                f,
                "truncated animation data at {offset} (need {needed} bytes)"
            ),
            Self::Invalid { offset, reason } => {
                write!(f, "invalid animation data at {offset}: {reason}")
            }
            Self::Unsupported(reason) => write!(f, "unsupported animation: {reason}"),
        }
    }
}
impl std::error::Error for AnimationError {}
pub type Result<T> = std::result::Result<T, AnimationError>;

pub(crate) fn invalid(offset: usize, reason: &'static str) -> AnimationError {
    AnimationError::Invalid { offset, reason }
}
pub(crate) fn unsupported(reason: impl Into<String>) -> AnimationError {
    AnimationError::Unsupported(reason.into())
}
pub(crate) fn fingerprint(bytes: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes).into()
}
