//! Library entry point — placeholder for the eventual swap.
//!
//! `Cargo.toml` currently points `[lib]` at `oldsrc/lib.rs` so that the
//! existing `amux` binary continues to build unchanged during the layered
//! refactor.  When work item 0069 swaps the `[lib]` entry to `src/lib.rs`,
//! this file becomes the real library root and the four public modules below
//! will be the API surface consumed by the `amux` and `amux-next` binaries.
//!
//! Until then, **this file is not compiled by Cargo**.  The `amux-next` binary
//! at `src/main.rs` declares the same modules via inline `mod` statements,
//! forming its own independent module tree rooted at `src/main.rs`.

#![forbid(unsafe_code)]

pub mod data;
pub mod engine;
pub mod command;
pub mod frontend;
