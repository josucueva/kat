//! KAT - semantic software repository.
//!
//! Entry point for the `kat` command-line tool.
//!
//! Step 0.1: crate skeleton only. Real CLI dispatch lands with the first
//! user-visible command (`kat init`, step 0.9).

mod domain;
mod encoding;
mod repository;

fn main() {
    // Placeholder. The first command (`kat init`) replaces this in step 0.9.
}

#[cfg(test)]
mod tests {
    /// Proves the crate skeleton compiles and the test harness runs.
    /// Module wiring is verified at compile time by the `mod` declarations.
    #[test]
    fn harness_works() {}
}
