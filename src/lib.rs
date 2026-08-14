//! KAT - semantic software repository.
//!
//! Library crate exposing the semantic domain, canonical encoding, and
//! physical repository layers. The `kat` binary is a thin CLI over these
//! layers.
//!
//! Layering (per `docs/architecture.md` and `docs/prototype-design.md`):
//! the domain and canonical-format rules are authoritative; storage and CLI
//! concerns are downstream.

pub mod cli;
pub mod domain;
pub mod encoding;
pub mod repository;
