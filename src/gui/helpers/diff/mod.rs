use super::*;

// The name-diff engine: general LCS diff (lcs), front/end structural diff,
// and diff-segment rendering.
mod front_end;
mod lcs;
mod render;

#[cfg(test)]
pub(crate) use front_end::ExactOp;
pub(crate) use front_end::FrontEndDiff;
pub(crate) use lcs::{diff_colored, push_seg};
pub(crate) use render::render_segs;
