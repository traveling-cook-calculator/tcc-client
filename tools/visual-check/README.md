# Visual check

Looks at the real app (compiled to WASM) in a headless Firefox against a mock backend, at phone and desktop width.

```sh
# 1. build and bundle (once per change)
cargo build --bin tcc --target wasm32-unknown-unknown --release
wasm-bindgen target/wasm32-unknown-unknown/release/tcc.wasm --out-dir tools/visual-check/site/pkg --target web --no-typescript
cp -r assets tools/visual-check/site/assets
npx tailwindcss -i assets/input.css -o tools/visual-check/site/assets/output.css --minify
# tools/visual-check/site also needs config.json (API base http://localhost:8081, see below) and an index.html that
# loads /pkg/tcc.js and calls init('/pkg/tcc_bg.wasm') with <div id="main"></div>.

# 2. mock backend + app server
python3 tools/visual-check/mock_api.py          # EDIT_DEADLINE=2026-11-01T18:00:00Z for "deadline over"

# 3. screenshots (pip install selenium; Firefox and geckodriver on PATH or FIREFOX= / GECKODRIVER=)
cd tools/visual-check && python3 scenarios.py states     # canceled / in review / no email
python3 scenarios.py interact                            # opened sections, unsaved-changes bar
python3 scenarios.py register                            # registration form with errors
# images: tools/visual-check/out/*.png (390 px = *_m, 1200 px = *_d)
```

`config.json` in `site/`: `{"auth": {...any...}, "api": {"base_url": "http://localhost:8081"}, "legal": {...as in public/config.json...}}`.
`wasm-bindgen` must match the version in `Cargo.lock`. Look at the pictures; the compiler does not see layout.
