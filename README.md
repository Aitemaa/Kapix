<p align="center">
  <img src="assets/banner.svg" alt="Kapix: instant screenshot, annotation and OCR" width="100%">
</p>

<p align="center">
  <img src="https://img.shields.io/badge/platform-Windows%2010%20%7C%2011-262626?style=flat-square&labelColor=0a0a0a" alt="Platform: Windows 10 | 11">
  <img src="https://img.shields.io/badge/built%20with-Rust%20%2B%20tiny--skia-262626?style=flat-square&labelColor=0a0a0a" alt="Built with Rust + tiny-skia">
  <img src="https://img.shields.io/badge/status-early%20stage-e8262d?style=flat-square&labelColor=0a0a0a" alt="Status: early stage">
  <img src="https://img.shields.io/badge/size-~1.3%20MB-262626?style=flat-square&labelColor=0a0a0a" alt="Size: ~1.3 MB">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-262626?style=flat-square&labelColor=0a0a0a" alt="License: MIT"></a>
</p>

<p align="center">
  <a href="#hotkeys">Hotkeys</a> ·
  <a href="#cli">CLI</a> ·
  <a href="#workspace">Workspace</a> ·
  <a href="#building">Building</a> ·
  <a href="docs/PRD.md">PRD</a> ·
  <a href="#license">License</a>
</p>

---

Instant screenshot, annotation & OCR tool for Windows — 100% Rust, one portable `.exe`, no runtime.

Kapix = **Ka**rel + **pix** (pixels). Same family as Kavox and Kadex: the same tokens, Archivo
typeface and one red accent, with light/dark following Windows.

- Hotkey → frozen overlay, drag, copy. Snaps to windows, magnifier loupe with HEX sampler.
- Annotate without menus: arrow, rectangle, pen, highlighter, text, irreversible pixelate, step badges.
- One-hotkey OCR through the built-in `Windows.Media.Ocr` — zero bundled models.
- ~1.3 MB executable, no WebView, no .NET, no network access, no telemetry.

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

MIT. The bundled Archivo typeface (Latin subset in `assets/fonts`) is under the SIL Open Font
License — see `assets/fonts/OFL.txt`.
