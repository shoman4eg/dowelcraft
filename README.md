<p align="center"><img src="docs/images/logo.svg" width="128" alt="dowel-craft"></p>

<h1 align="center">dowel-craft</h1>

<p align="center">English · <a href="docs/ru/README.md">Русский</a></p>

<p align="center">
  <a href="https://github.com/shoman4eg/dowelcraft/actions/workflows/ci.yml"><img src="https://github.com/shoman4eg/dowelcraft/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://github.com/shoman4eg/dowelcraft/releases"><img src="https://img.shields.io/github/v/release/shoman4eg/dowelcraft?include_prereleases" alt="Release"></a>
  <a href="LICENSE"><img src="https://img.shields.io/github/license/shoman4eg/dowelcraft" alt="License"></a>
</p>

> ⚠️ **Early alpha.** The program is under active development: expect bugs, and the `.dcraft` project format may still change. Double-check sizes and drilling before sending anything to production.

dowel-craft designs cabinet furniture: wardrobes, kitchens, chests of drawers, vanities, shelving. From the model it computes the parts, sheet cutting, the bill of materials and the drawings. Bazis-Mebelshchik models (`.b3d`, `.fr3d`) can be opened and exported, and cutting projects are written in the BAZIS-Raskroy format.

Runs on Linux, macOS and Windows. It has a desktop window, a command line and an MCP server, so an AI assistant (Claude and others) can design furniture too.

![3D view](docs/images/3d.png)

> dowel-craft is an independent project, not affiliated with Bazis-Center. “Bazis-Mebelshchik” and “BAZIS-Raskroy” are mentioned only to describe file-format compatibility.

## Contents

- [Install](#install)
- [Features](#features)
- [Screenshots](#screenshots)
- [The window](#the-window)
- [Hotkeys](#hotkeys)
- [Command line](#command-line)
- [MCP: design with AI](#mcp-design-with-ai)
- [Files](#files)
- [Building from source](#building-from-source)
- [Code layout](#code-layout)
- [Releasing](#releasing)

## Install

**Homebrew** (macOS, Linux):

```sh
brew tap shoman4eg/tap
brew trust shoman4eg/tap
brew install dowel-craft
```

This installs `dowel-craft`; macOS does not block Homebrew installs, so no quarantine step is needed.

On macOS, `brew install --cask dowel-craft-app` also puts **Dowel Craft** with its icon into Applications and Launchpad. The app is not notarized yet; if macOS refuses to open it, run `xattr -dr com.apple.quarantine "/Applications/Dowel Craft.app"`.

**Or** download a build from [Releases](https://github.com/shoman4eg/dowelcraft/releases):

| System | Installer | Archive |
|---|---|---|
| Windows x86_64 | `dowel-craft-windows-x86_64.msi` | `dowel-craft-windows-x86_64.zip` |
| macOS Apple Silicon (M1 and newer) | `dowel-craft-macos-arm64.dmg` | `dowel-craft-macos-arm64.tar.gz` |
| macOS Intel | `dowel-craft-macos-x86_64.dmg` | `dowel-craft-macos-x86_64.tar.gz` |
| Linux x86_64 | — | `dowel-craft-linux-x86_64.tar.gz` |

- **Windows `.msi`:** installs into Program Files, adds Dowel Craft to the Start menu, opens `.dcraft` projects with a double click and puts `dowel-craft` on `PATH`.
- **macOS `.dmg`:** open it and drag Dowel Craft onto Applications.

The archives need no installing: unpack and run `dowel-craft`. It is one program:
- `dowel-craft` or `dowel-craft project.dcraft` — the window;
- `dowel-craft cut …`, `dowel-craft spec …` and the other commands — the command line;
- `dowel-craft mcp` — the MCP server for AI.

On Windows the archive also has `dowel-craft.com`: in a terminal `dowel-craft` runs it, so command output shows up there.
The macOS archive also has `Dowel Craft.app`.

- **macOS:** builds are not signed, so the system blocks the first launch. Right-click the app → Open, or remove the quarantine flag: `xattr -dr com.apple.quarantine "/Applications/Dowel Craft.app"` (for an archive: `xattr -dr com.apple.quarantine dowel-craft-macos-*`).
- **Windows:** SmartScreen may warn about an unknown publisher — “More info” → “Run anyway”.
- **Linux:** needs the usual desktop libraries (Wayland or X11, Vulkan/OpenGL), present on any graphical system.

## Features

### Design
- **Cabinet templates:** floor cabinet, wall cabinet, L-shaped corner unit, countertop. Change the width and every part is recalculated.
- **Cabinet options:** size, plinth, back panel (nailed on or in grooves: the sides, bottom and top get a groove 1 mm wider than the back panel, 8 mm deep, 16 mm from the back edge; width, depth and offset are adjustable), partitions, shelves per section, doors, drawers (front heights bottom to top, or “auto” — sharing the free height equally), carcass and front materials, edge banding.
- **Fill an opening with the mouse:** shelf, partition, door, drawers — click inside an opening in the front view and the part fits between its neighbours. Offsets are adjustable. A second door cannot be placed into an occupied opening.
- **Free panels** in three planes: move, rotate, array copy, mirror, resize with handles. Contour: fillets, chamfers, cut-outs, grooves along an edge.
- **Grooves** show on part drawings (with width × depth), in “Panels for production” and are kept in `.b3d` export; BAZIS-Raskroy XML does not get them yet.
- **Explode a template** — the cabinet becomes a set of panels you can edit one by one.
- **Groups (blocks):** group the selection, move and copy it as a whole.
- **Ruler:** measure between part corners and contour points in 3D, “4 windows” and the front view.
- **Selection:** click, Ctrl+click, a Shift+drag frame, “Select the same material”; **mass replacement** of a material or edge banding in the whole project or in the selection.
- **Formulas in fields:** `600-4`, `(W-T)/2` (W, H, D, P, T — cabinet width, height, depth, plinth, thickness).
- **Undo and redo** for every change; an invalid change (a shelf above the cabinet, etc.) is rejected and the reason is shown at the bottom of the window.
- **Several projects** in tabs, with a reminder about unsaved changes.

### Fasteners and hardware
- **Fasteners** with drilling on both parts of every joint: confirmat screws, cam locks (Minifix), dowels, shelf supports, “Minifix + dowel”. The cam cup can face inside or outside the carcass.
- **32 mm system:** spacing between fastener points is a multiple of 32 mm, centred on the joint; shelf supports get spare holes (2 below and 2 above, 32 mm apart) so a shelf can be moved.
- **Hinges and drawer runners** come with doors and drawers, with their own drilling.
- **Bazis hardware models** (`.fr3d`: baskets, hinges, handles, sockets) — a folder library with previews; their holes are drilled into the panels they touch.
- **Drilling scheme** on the part drawings, with hole coordinates.

### Viewing
- **3D** with orbit, pan and zoom; display modes: solid, hidden lines thin, wireframe, transparent (x-ray), textured.
- **“4 windows”:** front, left, top and 3D at once; any window can be maximized.
- **Front view** for placing and dragging parts, snapping to neighbours.
- **Material textures:** an image library with search, Baltlaminat decor lookup by article number, adjustable texture tile size.
- **Model structure:** the product tree, hiding parts and groups.
- **Grid** in 3D and the front view, with an adjustable step.

### Output
- **Parts:** a table with cut sizes, material and edge banding.
- **Sheet cutting:** tries many layouts — fewest sheets first, then the largest single offcut. Sheet size, kerf and edge trim per material.
- **Documents:**
  - cutting maps and “Panels for production” (printable HTML, PDF);
  - a BAZIS-Raskroy project (XML, the same as Mebelshchik writes it);
  - an OBJ 3D model for visualisation (Blender, 3ds Max, SketchUp Pro): parts as named objects, hardware meshes, materials with textures in a .mtl, mm;
  - specification and cost estimate (CSV for Excel): boards in m², edge banding in m, hardware in pcs; with prices (per m², per metre, per piece) — sums, the total cost and the weight of the product (from the weight of 1 m² of each board);
  - assembly and part drawings (A4, HTML or PDF);
  - a model for Bazis-Mebelshchik (`.b3d`).
- **Model check:** overlapping parts, fasteners missing a panel, hardware without a panel, parts larger than a sheet.

### Bazis compatibility
- Open `.b3d` and `.fr3d` models for viewing, cutting and documents; “Make editable” turns a model into your own project.
- Export your project to `.b3d`.
- Run BAZIS-Script scripts (`.js`) that build and change the model; script dialogs are shown. Details (in Russian): [docs/scripts.md](docs/scripts.md).

### Other
- English and Russian: the window (Edit → Settings → Language; English by default), and the command line and MCP server with `--lang ru` or `DOWEL_LANG=ru` (messages, part names, cutting maps, drawings, CSV).
- Every hotkey is configurable.
- An AI assistant can edit the project right in the open window (Linux and macOS).

## Screenshots

| | |
|---|---|
| ![X-ray](docs/images/xray.png) **X-ray:** shelves, fasteners and part axes are visible | ![4 windows](docs/images/quad.png) **4 windows:** front, left, top and 3D |
| ![Wireframe](docs/images/wire.png) **Wireframe** | ![Front view](docs/images/front.png) **Front view:** placing shelves, doors and drawers |
| ![Parts](docs/images/parts.png) **Parts:** cut sizes, material, edge banding | ![Cutting](docs/images/cut.png) **Sheet cutting** with usable offcuts |

## The window

```
dowel-craft [project.dcraft | model.b3d]
```

- **Top** — a button bar (new, open, save, undo, export, analyze) and the menus.
- **Left** — “Tools” (what to create and how to modify it), “Project” (materials and edge banding), “Hardware” (the model library).
- **Centre** — “3D” (with the “4 windows” button), “Front”, “Parts”, “Cutting”.
- **Right** — “Model structure” and the “Properties” of the selection.
- **Bottom** — a hint on what to do next, and errors.

First piece of furniture: “Project” → “New material” → “Tools” → floor cabinet → its sizes in “Properties” → shelves and doors by clicking into an opening → “Cutting” and “File → Export”. The full guide is [docs/help.en.md](docs/help.en.md) (also in the program under “Help”).

## Hotkeys

Defaults (change them in “Edit → Settings…”):

| Keys | Action |
|---|---|
| 1, 2, 3, 4 | 3D camera: front, left, top, axonometric |
| 5 | zoom to fit |
| Ctrl+1…4 | tab: 3D, front, parts, cutting |
| Alt+1…5 | 3D display: solid, hidden thin, wireframe, transparent, textured |
| Ctrl+S | save |
| Ctrl+Z / Ctrl+Y | undo / redo |
| Shift + drag | selection frame (Ctrl+Shift — add to the selection) |
| M | ruler: click two points (snaps to part vertices) |
| Del | delete the selection |
| F7 | grid |
| Esc | leave the tool or clear the selection |

Mouse in 3D: left button orbits (or drags the selected part), right button pans, the wheel zooms, Ctrl+click adds to the selection.

## Command line

A model is a Bazis `.b3d` file or your own `.dcraft` project.

| Command | What it does |
|---|---|
| `dowel-craft spec <model> [-o out.csv]` | parts specification |
| `dowel-craft cut <model> [-o maps.html] [--sheet 2800x2070] [--kerf 4] [--trim 10]` | cutting: maps and “Panels for production” |
| `dowel-craft smeta <model> [-o smeta.csv] [--coef 1.2]` | board, edge banding and hardware usage, like the Bazis estimate, with prices, sums and weight |
| `dowel-craft raskroy <model> [-o project.xml] [--sheet 2800x2070]` | a BAZIS-Raskroy project |
| `dowel-craft drawings <model> [-o drawings.html\|.pdf]` | assembly and part drawings |
| `dowel-craft check <model>` | model check |
| `dowel-craft dump <file.b3d> [depth]` | the Bazis model tree (for debugging) |
| `dowel-craft import <file.b3d> [-o project.dcraft]` | Bazis model → your project |
| `dowel-craft b3d <project.dcraft> [-o model.b3d]` | your project → Bazis model |
| `dowel-craft obj <model> [-o model.obj]` | 3D model for visualisation (OBJ + MTL) |
| `dowel-craft script <project.dcraft> <script.js>` | run a Bazis script on the project |
| `dowel-craft design <project.dcraft> <command> ['{json}']` | a project command; the file is created if missing |

`--sheet` is the sheet size in mm, `--kerf` the saw kerf, `--trim` the edge trim, `--coef` the usage factor. For a `.dcraft` project they come from the materials unless given.

Output is in English; add `--lang ru` (or set `DOWEL_LANG=ru`) for Russian messages, part names and documents.

A whole project from the command line:

```sh
dowel-craft design wardrobe.dcraft material.add '{"name":"Chipboard White 16","thickness":16,"sheet":[2750,1830]}'
dowel-craft design wardrobe.dcraft edge.add '{"name":"PVC 0.4","thickness":0.4,"width":19}'
dowel-craft design wardrobe.dcraft cabinet.add '{"width":1200,"height":2200,"depth":600,"material":"Chipboard White 16","edge":"PVC 0.4","plinth":80,"doors":2}'
dowel-craft design wardrobe.dcraft shelf.add '{"cabinet":1,"height":700}'
dowel-craft cut wardrobe.dcraft
```

`dowel-craft design --help` lists every command with its parameters (materials, edge banding, cabinets, drawers, corner units, countertops, panels, blocks, fasteners, undo).

## MCP: design with AI

`dowel-craft mcp` runs an MCP server (JSON-RPC over stdio). Claude Code picks it up automatically in this repository through `.mcp.json`.

Elsewhere, with Claude Code:

```sh
claude mcp add dowel-craft -- dowel-craft mcp
```

Other MCP clients take the same in their settings:

```json
{
  "mcpServers": {
    "dowel-craft": { "type": "stdio", "command": "dowel-craft", "args": ["mcp"] }
  }
}
```

After a Homebrew install or the Windows installer `dowel-craft` is on `PATH`; with a downloaded archive, use the full path to the file.

Upgrading from 0.3 or older: the separate `dowel-craft-mcp` is gone. Re-add the server: `claude mcp remove dowel-craft`, then the command above.

Tools:
- `project_new`, `project_open`, `project_import`, `project_save`;
- `command` — any project command (like `dowel-craft design`);
- `run_script` — a BAZIS-Script script on the project;
- `parts`, `smeta`, `cut`, `export` (maps, Raskroy XML, CSV, drawings, `.b3d`);
- `render_front`, `render_iso` — front view and 3D pictures (PNG);
- `window_connect`, `window_disconnect` — work on the project open in the window: changes show up at once and Ctrl+Z in the window undoes them.

Example prompt: “build a kitchen: a 600 base unit with three drawers, an 800 sink unit with two doors, wall units above them and a countertop; show the view and the cutting”.

## Files

| Extension | What it is |
|---|---|
| `.dcraft` | your project (JSON): materials, edge banding, cabinets, panels, fasteners |
| `.b3d` | a Bazis-Mebelshchik model: open, export |
| `.fr3d` | a Bazis fragment (hardware): library, insert into the project |
| `.js` | a BAZIS-Script script |
| `.xml` | a BAZIS-Raskroy project (export) |

Window settings (language, hotkeys, grid, library folders, recent files) live in `~/.config/dowel-craft/` (Linux, macOS) or `%APPDATA%\dowel-craft\` (Windows).

## Building from source

Requires Rust 1.95 or newer ([rustup.rs](https://rustup.rs)).

```sh
cargo build --release
```

The program appears as `target/release/dowel-craft`.

On Linux the window needs the `libxkbcommon` and `libwayland` headers (Debian/Ubuntu: `sudo apt install libxkbcommon-dev libwayland-dev`).

Checks:

```sh
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Some tests compare results with Bazis sample files kept in local `reference/` and `example/` folders. They are not in the repository; without them those tests are skipped.

## Code layout

- `crates/dowel` — all the logic:
  - `b3d.rs` — reading and writing `.b3d`; `contour.rs` — contours; `panel.rs` — parts and edge banding;
  - `design/` — your project: cabinets, filling openings, commands, undo, analysis;
  - `fasteners.rs` — fasteners and drilling;
  - `nest.rs` — sheet cutting; `report.rs` — maps and tables; `raskroy.rs` — Raskroy XML;
  - `drawing.rs`, `pdf.rs` — drawings and PDF; `export.rs` — `.b3d` export;
  - `job.rs` — “open → cut → documents” for the CLI and the window;
  - `live.rs` — the MCP link to the open window;
  - `i18n.rs` and `lang/` — the interface language: code is English, `lang/ru.tsv` holds the Russian (english<TAB>russian), `lang/bazis.tsv` the Russian names Bazis needs (formula letters, XML values). A test fails if Cyrillic gets into code.
- `crates/script` — the BAZIS-Script runtime (Boa engine).
- `apps/gui` — the `dowel-craft` program: the window (egui 0.36, 3D on wgpu); commands and `mcp` are handed to:
  - `apps/cli` — the command line;
  - `apps/mcp` — the MCP server.
- `apps/console` — `dowel-craft.com` for Windows terminals.
- `docs/` — user guide, the BAZIS-Raskroy XML format (`formats/raskroy-xml.md`), scripts.

## Releasing

GitHub Actions does the builds:
- **CI** (`.github/workflows/ci.yml`) — on every push to `main` and every PR: clippy and tests on Linux, builds on macOS and Windows.
- **Release** (`.github/workflows/release.yml`) — on a `v*` tag: builds for Linux, macOS (Apple Silicon and Intel), Windows, archives in a GitHub Release, and the Homebrew formula and cask in [shoman4eg/homebrew-tap](https://github.com/shoman4eg/homebrew-tap) (needs the `HOMEBREW_TAP_TOKEN` secret).

```sh
git tag v0.4.0
git push origin v0.4.0
```

## License

MIT, see [LICENSE](LICENSE). Third-party code is listed in [NOTICE.md](NOTICE.md).
