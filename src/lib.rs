//! Generate Rust bindings and wrappers for a CTP SDK.
//!
//! The public API validates an SDK directory, parses its CTP declarations,
//! and writes a standalone generated crate containing the FFI layer and Rust
//! wrappers.

/// Code generation for a validated CTP SDK.
pub mod codegen;
/// SDK discovery, validation, and header decoding.
pub mod sdk;

/// Small filesystem utility types used by the generator.
pub mod misc;
