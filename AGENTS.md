# Agent guidelines

## Working in the tree

- The tree usually has uncommitted work, and files may change while you work. Re-read a file before
  rewriting it, and never revert a change you did not make.
- Make the change that was asked for, then stop. Verify with quiet checks:
  `nix build .#checks.<system>.<name>` or `just check --no-build`.
- Do not write probe edits into source files, and do not `git checkout` a file to undo one. Ask
  before a long live test, such as booting an emulator or running the whole dev loop.
- Update every comment and doc your change affects, in the same change.
- Commit only when asked. When rewriting history, keep the existing commit messages.

### Commits

- One commit per coherent change: a feature, a refactor, a build change.
- Subject: `<type>(<area>): <imperative summary>`, at most 72 characters, lowercase after the colon.
  - The type is one of `feat`, `fix`, `refactor`, `perf`, `test`, `docs`, `build`, `ci` or `chore`.
  - The area is a crate or part (`web`, `server`, `protocol`, `android`, `nix`), or several joined
    with commas (`feat(web,server): ...`). Leave it out when the change spans the whole workspace.
- Body: a few short lines on what changed and why. The comment style rules below apply.

## Comments

These apply to every language and file type, including Rust, TypeScript, SCSS, nix, Kotlin and
config files.

- Most code needs no comment. Write one only where a reader would otherwise have to rediscover
  something: why complex logic is built the way it is, a constraint from outside the code, or a
  source.
- No comment when the names and code already say it. That includes a doc comment that restates an
  item's name or type.
- Otherwise one or two plain sentences on why: the reason, a constraint, or a source. Name the
  concrete file, flag or constraint. Two clear sentences beat one clever one.
- Keep paper and URL references, and the reasoning behind tuned numbers.
- No em or en dashes, no `--`, no clauses joined with semicolons, no "not X but Y" contrasts, no
  flourishes or riddle-like phrasing.
- Never start a comment, or a sentence in one, with a question word. Write "The state of the
  session.", not "What state the session is in."
- A comment above the wrong block, or one describing something removed, is worse than none.

## Rust

- Make invalid states unrepresentable: enums whose variants carry exactly their data, private fields
  with checked constructors where fields must agree, `NonZero*` where zero is meaningless. Types
  generated from the emulator's protos (`crates/server/build.rs`) are the exception.
- Use newtypes, not type aliases, for domain values. `Result<T>` aliases are fine.
- Data of a fixed size is an array or a `Box<[T; N]>`, never a `Vec`. Build large boxed arrays on
  the heap (`vec![..].into_boxed_slice().try_into()`), because building them inline overflows the
  stack in debug builds.
- Precomputed tables live in a `static`, with `LazyLock` when they need runtime maths.
- In wasm code (the page), reach the browser through `web-sys` and typed `#[wasm_bindgen] extern`
  declarations, never through by-name `Reflect` helpers. Never compile JavaScript at runtime
  (`eval`, `new Function`).
- A Leptos `#[component]` in a private module carries `#[allow(unreachable_pub)]`, because the props
  builder the macro derives is always `pub`. The attribute needs no comment.
- A dependency only some platforms need goes in a `[target.'cfg(...)'.dependencies]` table. Never
  add hakari excludes (`.config/hakari.toml`) or per-variant builds.

## Rust formatting

rustfmt handles most of it. Beyond that:

- Leave a blank line after a closing brace that steps back out of an indentation level, before the
  next statement or item. Not when the next line is another closing brace, or continues the same
  expression (`.method()`, `else`). Not between `match` arms.

  ```rust
  let Some(running) = &mut self.0 else {
      self.0 = Some(Running::new(spectrum));

      return;
  };

  running.frame_count = running.frame_count.saturating_add(1);
  ```

- Keep consecutive `let` declarations together, then a blank line before the next statement that is
  not a declaration. A `let ... else` counts as logic.
- One `let` per variable. Never `let (a, b) = (x, y);` with a tuple literal on the right.
  Destructuring a tuple a function actually returns is fine.

## Writing docs

- Keep docs minimal. A page that restates the code goes stale with every change, so write only what
  a reader cannot get from the code quickly, and link to the file for the rest.
- Describe behaviour and contracts. Leave out the libraries and primitives underneath unless the
  reader has to act on them.
- Leave out numbers that change: tuned constants, ports, versions, option lists. Point at the file
  that holds them.
- When justifying a choice, compare it with the off-the-shelf option a reader would expect.
- Name the part you mean. Never write a bare "the server", "the app" or "the client" where more than
  one part could be meant. The names this workspace uses:
  - **tetherd** is the host daemon in `crates/server`.
  - **The page** is the browser app in `crates/web`.
  - **The helper** is the on-device Android program in `crates/android/helper`.
  - **The shim** is the camera `LD_PRELOAD` library in `crates/vcam`.
  - **The protocol** is the shared message types in `crates/protocol`, which tetherd and the page
    both speak.
