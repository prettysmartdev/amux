#[cfg(any(target_os = "linux", target_os = "android"))]
pub mod linux;

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(target_os = "freebsd")]
pub mod freebsd;

#[cfg(target_os = "dragonfly")]
pub mod dragonfly;

#[cfg(target_os = "openbsd")]
pub mod openbsd;
