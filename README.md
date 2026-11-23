# Kapix

Instant screenshot, annotation & OCR tool for Windows — 100% Rust, one portable `.exe`, no runtime.

- Hotkey → frozen overlay, drag, copy. Snaps to windows, magnifier loupe with HEX sampler.
- Annotate without menus: arrow, rectangle, pen, highlighter, text, irreversible pixelate, step badges.
- One-hotkey OCR through the built-in `Windows.Media.Ocr` — zero bundled models.
- ~1.1 MB executable, no WebView, no .NET, no network access, no telemetry.

Full requirements: [docs/PRD.md](docs/PRD.md).

## Hotkeys

| Default | Action |
| :--- | :--- |
| `Ctrl+Shift+S` | Region capture (overlay) |
| `Ctrl+Shift+O` | OCR a region → text on clipboard |
| `Ctrl+Shift+F` | Capture the monitor under the cursor |
| `Ctrl+Shift+A` | Capture the active window |

Inside the overlay: `Enter`/`Ctrl+C` copy · `Ctrl+S` save · `O` OCR · `Ctrl+P` pin · `C` copy HEX ·
`A R P H T B N` tools · `1`–`6` colors · wheel = size · `F` fill/backing · `Ctrl+Z`/`Ctrl+Y` ·
arrows nudge (`Shift` ×10) · `Esc`/right-click cancel.

Config lives at `%APPDATA%\Kapix\config.toml` (tray → *Edit config*; picked up automatically on the
next capture).

## CLI

```
kapix                                  # start the tray daemon
kapix capture --mode region --copy
kapix capture --mode ocr --stdout
kapix capture --mode window --out shot.png
kapix capture --mode fullscreen [--gdi] --out shot.jpg
```

## Workspace

| Crate | Layer | Contents |
| :--- | :--- | :--- |
| `kapix-core` | backend, pure Rust | geometry, BGRA images + PNG, pixelate, annotation model + undo, config, hotkeys, templates |
| `kapix-win` | backend, Windows | DXGI Desktop Duplication (+HDR) with GDI fallback, window snap, clipboard (`CF_DIBV5` + `PNG`), WinRT OCR, WIC JPEG, dialogs, registry |
| `kapix-ui` | frontend, pure Rust | overlay state machine, `tiny-skia` renderer (shared by preview and export), loupe, toolbar, toasts |
| `kapix-app` | entrypoint | tray daemon, hotkeys, Win32 window hosts (overlay, toast, pin), CLI |

`kapix-core` and `kapix-ui` are platform-independent, so the whole UI is unit-tested and
snapshot-rendered on Linux too: `cargo run --release -p kapix-ui --example snapshots` writes PNGs
to `snapshots-out/`, and `--example bench_paint` measures 4K repaint times.

## Building

```
cargo build --release -p kapix-app          # on Windows → target/release/kapix.exe
```

Cross-compiling from Linux: `cargo xwin build --release -p kapix-app --target x86_64-pc-windows-msvc`
(needs `cargo install cargo-xwin`, `clang`, `lld`).

CI (`.github/workflows/ci.yml`) runs fmt, clippy, all tests on Windows, a pure-Rust dependency
check, the PRD size gate (warn > 2.5 MiB, fail > 4 MiB) and a CLI capture smoke test.
Tagging `vX.Y.Z` builds `kapix-x64.exe` + `kapix-arm64.exe` with SHA-256 sums into a draft release.

## Deviations from the PRD

- **No `arboard`:** clipboard is written directly with Win32 so both `CF_DIBV5` and `PNG` formats
  are published (the PRD's "enhancement" path) without arboard's `image` dependency.
- **No `softbuffer`:** the overlay buffer is blitted with `SetDIBitsToDevice` directly (what
  softbuffer does on Windows anyway) — one less dependency.
- **`tiny-skia` and `kapix-ui` compile at `opt-level = 3`** (rest is `z`): ~4× faster 4K repaints
  for ~160 KB.

## Status

Implemented end to end, but nothing has run on real Windows hardware yet apart from CI. The manual
hardware matrix in PRD §11 (mixed DPI, negative coordinates, HDR, RDP, rotated monitors) is still to do.

## License

MIT
