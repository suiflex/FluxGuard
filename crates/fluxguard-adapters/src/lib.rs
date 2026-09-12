//! FluxGuard client and provider source adapters.

#[macro_use]
mod support;
// Fake-binary tests spawn shell scripts, so the helpers are unix-only.
#[cfg(all(test, unix))]
#[path = "../tests/support/mod.rs"]
mod test_support;

pub mod clients;
pub mod providers;
