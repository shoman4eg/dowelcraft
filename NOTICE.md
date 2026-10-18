# Borrowed code

- **QuadFurn** (github.com/rbm13s/QuadFurn) — used with the author's permission. The repository had no license file at the time of borrowing.
  - `apps/gui/src/expr.rs` — expression evaluation in number fields (`eval_math_expression`).
  - `crates/dowel/src/pdf.rs` — TrueType font for PDF (`PdfTrueTypeFont`, `ensure_ttf_bytes`) and PDF file assembly.
- **cadcraft / craft-libs** (github.com/storytold) — MIT OR Apache-2.0: the `craft-ui` window shell (git dependency).
  - `apps/gui/src/icons.rs` — drawing icons in code (`Pen` on a 24×24 grid) and some of the pictures (file, undo, move, copy, rotate, mirror, delete, block, explode) from cadcraft's `crates/ui-egui/src/icons.rs`; the furniture icons are our own.
  - `apps/gui/src/theme.rs` — colours and spacing of the cadcraft dark theme (`crates/ui-egui/src/theme.rs`); `apps/gui/src/chrome.rs` — start page and document bar modelled on `crates/ui-egui/src/chrome.rs`.
- **SolidCraft** (github.com/tangency-labs/solidcraft) — MIT OR Apache-2.0: packaging modelled on SolidCraft's `packaging/`.
  - `packaging/windows/dowel-craft.wxs` — MSI installer (WiX), modelled on `assets/platforms/windows/solidcraft.wxs`.
  - `apps/gui/src/viewport.rs` — 3D grid plane choice (`grid_axis`: floor first, the facing wall for an edge-on view, with hysteresis) and the 1-2-5 grid step, after `grid_plane` and `grid_step` in `crates/engine/src/view.rs`.
  - `apps/gui/build.rs` — icon in the `.exe`; the `.dmg` step in `.github/workflows/release.yml` — modelled on `packaging/macos/package.sh`.
