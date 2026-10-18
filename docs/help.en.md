# How to use dowel-craft

dowel-craft designs cabinet furniture: wardrobes, kitchens, chests of drawers, shelving. From the model it computes the parts, the sheet cutting, the specification and the drawings. Bazis-Mebelshchik files (.b3d, .fr3d) can be opened and saved.

## The window

- **Top** — the icon bar (new, open, save, undo, export, analysis) and the menus “File”, “Edit”, “View”, “Insert”, “Modify”, “Tools”, “Help”.
- **Left** — the “Tools” palette (what you can create and how to change it), “Project” (materials and edge banding), “Hardware” (model library by folders).
- **Centre** — “3D”, with the “4 windows” button: front, left, top and 3D at once (each with transparency and other modes; the corner button maximizes a window), “Front view”, “Parts” (table), “Cutting” (sheet maps).
- **Right** — “Model structure” (what the item consists of) and “Properties” of the selection.
- **Bottom** — a hint about what to do next. Hover an icon and the same line tells what it does.

## Your first furniture in 5 steps

1. **Material.** “Project” → the “➕ New material” button: name (for example “Chipboard 16”), thickness, sheet size → “Add material”. Modules cannot be created without a material. Add edge banding there too.
2. **Cabinet.** “Tools” → “Modules” → the floor cabinet icon. The cabinet appears already selected; its width, height, depth, plinth and doors are on the right in “Properties”.
3. **Shelves and partitions.** The shelf (or partition) icon in “Panels and fronts into an opening” → the “Front view” opens → click inside an opening. The shelf goes between the nearest parts. Gaps are set below the palette. Esc exits the tool.
4. **Doors and drawers** — the same: the door or drawers icon → click into an opening. Hinges, runners and their drilling are added automatically.
5. **Result.** “Parts” — the list with sizes and banding; “Cutting” — sheet maps. “File → Export” — cutting maps (HTML, PDF), BAZIS-Cutting project (XML), specification and estimate (CSV), drawings, .b3d model.

## Mouse and keys

- **3D:** left drag rotates, right drag pans, the wheel zooms. Click a part to select it, Ctrl+click adds to the selection. **Drag a selected part** to move it along X, Y or Z (whichever the mouse follows) in whole mm; the offset is shown in the corner.
- **Front view:** drag the selected part — its edges snap to neighbours. A free panel has orange handles: drag them to resize.
- **Right click** on a part — menu: delete, group into block, explode, clear selection.
- **1, 2, 3, 4** set the 3D camera: front, left, top, axonometric; **5** zooms to fit. **Ctrl+1…4** switch the tab: 3D, front, parts, cutting. **Alt+1…5** switch the 3D display: solid, hidden thin, wireframe, transparent, textured. All hotkeys are changed in “Edit → Settings…”: click a combination and press a new one. **Del** deletes the selection. **Esc** exits a tool or clears the selection. **Ctrl+Z / Ctrl+Y** undo / redo. **Ctrl+S** saves. **Enter** applies move, rotate, etc.
- Number fields accept expressions: `600-4`, `(800-36)/2`. Cabinet fields also accept its sizes by letter: **W** width, **H** height, **D** depth, **P** plinth, **T** material thickness; e.g. a middle partition is `(W-T)/2` (Russian letters Ш, В, Г, Ц, Т are aliases). A free panel has **L** (length), **W**, **T**. The formula becomes a number at once. Drag a field left or right to change it.

## Projects, textures, grid

- **Several projects.** Each project is a tab under the menu. “+” or “New” opens a new tab, so does opening a file. “*” after the name means unsaved changes; closing asks whether to save.
- **Material texture.** “Project” → “Choose…” at a material (or in the new material form): an image library by folders with previews and search. Add your folders with “+ Add folder…”. “by article” finds the Baltlaminat image by the material article. See it in the 3D “Textured” mode.
- **The grid** in the front view and in 3D is infinite and its spacing grows when you zoom out. Toggle it with F7 or the grid icon at the bottom right; spacing and major lines — the gear or “Edit → Settings…”, which also has the program language.

## Model structure

- The item tree: cabinets (inside — “Carcass”, “Back panels”, “Doors”, “Drawers”), groups (blocks) and free panels. Click a part to select it, a group name to select the whole group.
- **The eye** on a row hides or shows a part or a whole group in 3D and the front view (hidden parts stay in the parts table and cutting). “Show hidden” brings everything back.
- **Group** puts the selection into a group that moves and copies as one. Right-click a group to rename or ungroup it.

## Cabinets and free panels

Cabinets, corner units and countertops are **templates**: change the width and all parts are recalculated. To edit one part of a template (cut-out, fillet, own size), select it and press “Explode” — the template becomes a set of free panels.

A free panel (the “Free panel” icon) can be placed in any of three planes, moved, rotated, copied as an array, mirrored. Its contour is edited in “Properties”: corner fillets and chamfers, cut-outs.

## Hardware and fasteners

- **Fasteners** (confirmats, cam locks, dowels, shelf supports, “Minifix + dowel” — a cam lock with a dowel next to it): the “Fasteners” icon → choose the type below the palette → select a cabinet (or several free panels with Ctrl) → “Apply to selection”. Holes with coordinates appear on both parts of every joint. For cam locks choose the cup side there: “inside” the carcass (top of the bottom, underside of the top) or “outside”; applying again changes fasteners already placed. Shelf supports always go under the shelf.
- **Hardware models** (.fr3d — baskets, hinges, locks, sockets): the “Hardware” palette, folders as on disk. “Place” puts the model to the right of the modules; then move it in “Properties”. Its holes land in the panels it touches.
- Baltlaminat libraries and your own models are just folders with .fr3d files. Add folders with “+ Add folder…” in the “Hardware” palette; they are remembered.

## Checks

“Tools → Model analysis” (the magnifier icon) finds intersecting parts, fasteners off panels, hardware without a panel and parts larger than the sheet. Click a problem to select its parts.

## Bazis files

- **Open .b3d** — the model opens read-only (parts, cutting, export). To change it press the lock icon “Make editable”.
- **Export .b3d** — a model for Bazis-Mebelshchik.
- **Bazis scripts** (.js) — “Tools → Run a Bazis script”. Scripts that build and change the model work; script windows are shown.

## Edits by an AI assistant (MCP)

While the window is open, an AI assistant can connect to it (the `window_connect` tool of the dowel-craft MCP server) and edit the project in the active tab: changes appear at once, Ctrl+Z undoes them, “*” reminds you to save. No need to save and reopen the file. Works on Linux and macOS.

## If something does not work

- The reason of an error is shown in red at the bottom of the window.
- The program prints every action to the terminal it was started from (`[команда] …`, `[ошибка] …`) — these lines help to find the problem.
- A faded icon means a condition is missing: no material, nothing selected, etc. Hover it — the hint will say.
