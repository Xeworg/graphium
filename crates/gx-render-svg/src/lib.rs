//! `gx-render-svg` — Graphium's own SVG writer (PRD §8.5, §8.5.1, §8.5.2).
//!
//! The crate owns the two halves of the render side of the engine:
//!
//! - [`InterMetrics`] is the **only** implementation of `gx_core::TextMetrics`
//!   (PRD §8.4.1), with `rustybuzz` and the embedded Inter faces.
//! - [`SvgRenderer`] is the **only** place where SVG is written (PRD §8.5).
//!
//! `gx-app` injects both into `gx_core::engine::install` (F0 decision L7).

pub mod escape;
pub mod metrics;

#[cfg(test)]
pub(crate) mod test_support;

pub use escape::escape_text;
pub use metrics::{InterMetrics, MetricsError};
