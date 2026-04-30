#![forbid(unsafe_code)]
// Layer 0 types are not yet consumed by any frontend; suppress dead-code
// warnings for the duration of the refactor (work items 0066–0070).
#![allow(dead_code)]

mod command;
mod data;
mod engine;
mod frontend;

fn main() {
    println!("amux-next: Layer 0 only — see aspec/architecture/2026-grand-architecture.md");
}
