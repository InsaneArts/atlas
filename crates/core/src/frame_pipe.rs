//! The shared frame pipe's pure parts: its records, codecs and accounting.
//! Nothing here opens a socket, captures the screen or decides authority.
pub mod budget;
pub mod diagnostics;
pub mod model;
pub mod pixels;
pub mod wire;

#[cfg(test)]
#[path = "frame_pipe_tests.rs"]
mod tests;
