#![cfg_attr(feature = "deny-all-warnings", deny(warnings))]

#[macro_use]
extern crate slog;

pub mod geometry;
pub mod graphics;

use crate::graphics::HalState;
