# Repository Guidelines for phyron-skia-canvas

## CRITICAL: Git Safety

**NEVER use `git reset --hard`, `git checkout --`, `git clean`, or any destructive git command without FIRST running `git stash`!**

Uncommitted working tree changes CANNOT be recovered after a hard reset. Always stash first:

```bash
git stash push -m "backup before reset"
git reset --hard <target>
# If something went wrong:
git stash pop
```

This applies to ALL destructive git operations. When in doubt, stash first.

---

## CRITICAL: No Unwrap/Expect Without Safety Comment

**Every `.unwrap()` and `.expect()` MUST have a `// SAFETY:` comment explaining why it cannot fail, OR must be replaced with proper error handling.**

Panics in Neon FFI crash the Node process -- never acceptable without proof of safety. Use:

- `cx.throw_error()` for Neon FFI boundaries.
- `?` for internal Rust error propagation.
- `unwrap_or()` / `unwrap_or_else()` / `unwrap_or_default()` when a fallback exists.
- `if let Some(...)` / `match` for optional values.

```rust
// BAD: panics crash the Node process.
let result = some_operation().unwrap();

// GOOD: propagate error to JS.
let result = some_operation()
    .ok_or_else(|| "operation failed".to_string())?;

// GOOD: provably safe with documented reason.
// SAFETY: `collection` was set to `Some` on the previous line.
let coll = self.collection.as_ref().unwrap();
```

---

## Build, Test, and Development Commands

```bash
# Build the project (Neon/Node native module).
cargo build

# IMPORTANT: NEVER use --release unless explicitly requested.
# Debug builds are faster and sufficient for development.

# Format code.
cargo fmt --all

# Run clippy linter.
# NOTE: --all-features includes `metal` which only compiles on macOS.
# On Linux, use: cargo clippy --all-targets --features "vulkan,window,freetype" -- -D warnings
cargo clippy --all-targets --all-features -- -D warnings

# Check code without building.
cargo check

# Run Node.js tests.
npm test
```

---

## Coding Style & Naming Conventions

- Follow standard Rust style: four-space indentation, `snake_case` for modules/functions, `CamelCase` for types.
- Write idiomatic Rust. Prefer functional style over imperative style.
- Prefer `collect()`/iterator pipelines over `new + for + push/insert`.
- Avoid unnecessary allocations, conversions, copies.
- Avoid `unsafe` code unless absolutely necessary.
- Avoid `return` statements; structure functions with if/else blocks instead.
- **NO INLINE PATHS**: Always import types at the top of the file using `use` statements. Never use inline paths like `crate::core::Error::Generic(...)` in function bodies.
- Use `SmallVec` for collections that are usually small in hot paths.

### Naming Conventions

- **Casing**: `UpperCamelCase` for types/traits/variants; `snake_case` for functions/methods/modules/variables; `SCREAMING_SNAKE_CASE` for constants/statics.
- **Conversions**: `as_` for cheap borrowed-to-borrowed; `to_` for expensive conversions; `into_` for ownership-consuming conversions.
- **Getters**: No `get_` prefix (use `width()` not `get_width()`).
- **Tests**: NEVER use `test_` prefix/suffix in test function names. The `#[test]` attribute already marks it as a test.

---

## Error Handling

- **Neon FFI boundary**: Return `cx.throw_error()` or `cx.throw_type_error()`. Never panic.
- **Internal Rust**: Propagate errors with `Result<T>` and `?`.
- **Optional values**: Use `if let Some(...)`, `.unwrap_or()`, or `.ok_or()`.
- Every `unwrap()`/`expect()` must have a `// SAFETY:` comment or be replaced.

---

## Performance Best Practices

### Memory Management

- Avoid unnecessary cloning in hot paths.
- Use `Arc`/`Rc` for shared immutable data.
- Prefer borrowing over ownership transfer when possible.

### String Handling

- Use `&str` instead of `String` where ownership is not needed.
- Avoid `.to_string()` for temporary values.
- Use `from_utf8_lossy()` instead of `from_utf8().unwrap()` for untrusted bytes.

---

## Documentation Guidelines

- All code comments containing complete sentences must end with a period.
- All doc comments must end with a period (unless headlines).
- En-dashes must be written as two dashes: `--`.
- References to types, keywords, symbols must be in backticks: `Foo`.

---

## Writing Instructions

These apply to user communication and documentation:

- Be concise. Use simple sentences. Technical jargon is fine.
- Do not overexplain basic concepts. Assume the user is technically proficient.
- Avoid flattering, corporate, or marketing language.
- Avoid vague/generic claims not substantiated by context.
- Avoid weasel words.

---

## Commit Messages

Keep commit messages concise: 2-3 sentences max.

- One sentence: state the problem/change.
- One sentence: state the fix/implementation.
- Optional: one sentence of context if needed.

No bullet points, long explanations, or multiple paragraphs.

---

## Pre-Commit Checklist

1. `cargo fmt --all`
2. `cargo clippy --all-targets --all-features -- -D warnings` -- must pass clean (on Linux, omit `--all-features` and use `--features "vulkan,window,freetype"`).
3. `cargo build` -- must compile.
4. `npm test` -- existing tests must pass.
5. All `unwrap()`/`expect()` calls must have `// SAFETY:` comments or proper error handling.
