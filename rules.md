# KAT Development Rules

Working rules for the KAT v0.1 prototype. These are the conventions I follow
for planning, coding, testing, and committing so the project stays minimal,
disciplined, and faithful to the specifications.

---

## Working agreement (how I work)

1. **One step at a time.** Follow `docs/implementation-plan.md` strictly, in
   order. Never jump ahead to a later step or to unplanned substrate work.
2. **Validate before every commit.** All of these must pass before a step is
   committed:
   - `cargo build`
   - `cargo test`
   - `cargo fmt --check`
   - `cargo clippy --all-targets -- -D warnings`
3. **Commit after each completed step**, with a clear message (see Commit
   rules). Working tree must be clean afterward.
4. **Update `docs/implementation-plan.md` after each step**: status line,
   checkbox states, a Notes line, and a progress-log row.
5. **Update the cross-step memory/summary after each step** (currently
   `docs/implementation-plan.md` progress line): progress, commit hash, and
   any new facts worth remembering.
6. **Push to `origin/main` only after the user approves.** Report the commit
   hash and ask before pushing.
7. **Report after each step**: what changed, validation results, commit hash,
   and what is next.
8. **KISS / minimalism.** Do not add abstractions, modules, or dependencies
   unless the current step directly requires them. Once Phase 0 ended, no more
   substrate work unless Phase 1 requires it.

---

## Design rules

1. **Layering is authoritative → downstream:**
   `domain + canonical format → encoding → repository storage → CLI`.
   Storage and CLI must never own or redefine semantics.
2. **Three validation layers stay separate:**
   `encoding validity ≠ repository integrity ≠ semantic validity`. Do not blur
   them (e.g. `ObjectStore` verifies bytes + hash only; it never decodes).
3. **Normative sources rule.** Ground every decision in
   `spec/canonical-format.cddl` and `docs/canonical-format.md`
   (and the other `docs/` specs). Never independently redefine semantics.
4. **Fail-closed.** Reject invalid input; never repair, normalize, or silently
   guess. Encoder and decoder are equally strict — the decoder rejects
   non-canonical encodings, not just malformed CBOR.
5. **Never let the encoder be the sole oracle** for golden vectors. Vectors
   come from the specification (hand-derived / independently verified);
   `tests/vector_conformance.rs` then proves the encoder matches them.
6. **Error types are per-layer**, composed with `thiserror` `#[from]`. Only
   define error variants that are actually reachable; no dead variants.
7. **Protocol numbers are explicit.** Envelope fields, object kinds, lifecycle
   values, and operation identifiers are written literally in the encoder and
   decoder — never derived from Rust enum discriminants.
8. **`ObjectStore` is byte-oriented** (no CBOR/decode knowledge). **`RefStore`
   has no semantic interpretation.** The invariant
   `accepted.change.result_state == accepted.state` belongs to open/integrity
   and Change publication, not to the stores.
9. **CLI stays thin**: parse + dispatch only; no framework unless a step
   requires it. The CLI never owns repository semantics.
10. **Atomic publication**: immutable objects first, `refs/accepted` last.
    Content-addressed objects are immutable and never overwritten.
11. **Property maps are ordered `Vec<(String, PropertyValue)>`** so malformed
    duplicates/ordering remain observable to validation instead of being
    normalized at construction.

---

## Coding rules

- Rust **edition 2024**, toolchain pinned to **stable** via
  `rust-toolchain.toml` (`channel = "stable"`), which resolves to each host's
  default target. Machine-local toolchain flavour is **not** committed; on a
  machine that needs a non-default target (e.g. Windows-GNU instead of MSVC),
  set it per-machine with `rustup override set <toolchain>` inside the repo.
- **Library + binary split**: `src/lib.rs` declares the modules
  (`domain`, `encoding`, `repository`); `src/main.rs` is a thin CLI.
- **Typed newtypes** for every semantic ID (UUIDv4) and `ObjectId([u8; 32])`.
  `ObjectId` is always _derived_ (hash of bytes) — there is no
  `ObjectId::new()`; UUID IDs get `new()` via
  `#[allow(clippy::new_without_default)]` (no `Default`).
- **Map-key ordering** = bytewise comparison of full deterministic text-key
  CBOR encodings (RFC 8949 §4.2.1), shared by validator and encoder
  (`encoding::cbor::cmp_encoded_text`).
- **No serde derive** for repository metadata — parse `toml::Table` manually
  and validate (rejects unsupported/malformed values).
- **dev-dependencies only for tests**: `tempfile`, `serde_json` (with
  `preserve_order`).
- **Edition 2024 quirks**: `use std::io::Write` for `write_all`; explicit
  lifetimes where needed.

---

## Commit rules

- Message format: `Step <X.Y>: <short summary>` with a body explaining what,
  why, and the validation status (test count, fmt/clippy clean).
- One logical step per commit; no mixed concerns.
- Plan/doc-only changes get a clear descriptive subject (e.g.
  `Plan: restructure Phase 1 ...`).
- Working tree must be clean after the commit; report the hash.

---

## Testing rules

- **Golden vectors**: assert exact canonical bytes **and** the externally
  derived ObjectId for every valid fixture; assert the decode → re-encode
  round-trip (`canonical_bytes(&decode_canonical(bytes)) == bytes`).
- **`invalid/encoded/` fixtures**: the decoder must reject all of them.
- **`invalid/structural/` fixtures**: `canonical_bytes` must reject with the
  specific `CanonicalStructureError`.
- **Integration tests exercise the real repository layout** (init, open,
  change) in temp dirs via `tempfile`.
- Every step adds tests that prove that step's acceptance criteria.

---

## Environment / machine facts

- Cross-platform: developed on both Windows and Linux. Toolchain is
  host-agnostic (`channel = "stable"`); a machine needing a non-default
  target uses a local `rustup override` rather than editing committed files.
  Rust installed via `rustup` on each machine; PATH via
  `$HOME/.cargo/env` (bash) / `$env:USERPROFILE\.cargo\bin` (PowerShell).
- Repo: `https://github.com/josucueva/kat.git`, default branch `main`.
- `.gitignore` currently ignores `/target` only (a `.kat/` created in-repo is
  untracked — use a temp dir or a deletable subfolder for manual `kat init`
  demos).
