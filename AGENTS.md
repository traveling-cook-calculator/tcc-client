# AGENTS.md (tcc-client)

Rules for everyone who changes this client, people and AI assistants alike. Details: `docs/DESIGN_GUIDE.md`.

## UI

1. Build pages from `crate::ui`. No raw `button`, `input`, `textarea`, `select`, no own hint/warning boxes
   (`Notice`), no own caption styles (`Eyebrow`), no own inset groups (`Panel`), no colors outside the palette.
   `src/ui_rules.rs` fails the tests if you do.
2. Missing component? Add it to `ui/`, export it, list it in the guide, then use it. Do not copy classes.
3. Everything a guest reads comes from `side/participant/lang.rs` in German and English. Shared components on guest
   pages take a language (see `Address`).
4. Forms: quiet until the first attempt, then all errors at once; check what the server would refuse before sending.
5. Design for 390 px first. Check the result in a browser (`tools/visual-check/`), not only in the compiler.
6. New Tailwind classes need a regenerated `assets/output.css`.

## Before you hand something over

`cargo test --bin tcc` green, `cargo check --target wasm32-unknown-unknown` clean, screenshots of the changed pages at
390 px and 1200 px looked at (normal, error, locked). Say what was not checked.
