# Заимствованный код

- **QuadFurn** (github.com/rbm13s/QuadFurn) — с разрешения автора. На момент заимствования у репозитория нет файла лицензии.
  - `apps/gui/src/expr.rs` — вычисление выражений в числовых полях (`eval_math_expression`).
  - `crates/dowel/src/pdf.rs` — шрифт TrueType для PDF (`PdfTrueTypeFont`, `ensure_ttf_bytes`) и сборка файла PDF.
- **cadcraft / craft-libs** (github.com/storytold) — MIT OR Apache-2.0: оболочка окна `craft-ui` (git-зависимость).
  - `apps/gui/src/icons.rs` — способ рисования иконок кодом (`Pen` на сетке 24×24) и часть рисунков (файл, отмена, сдвиг, копия, поворот, зеркало, удалить, блок, разрушить) — из `crates/ui-egui/src/icons.rs` cadcraft; мебельные иконки свои.
  - `apps/gui/src/theme.rs` — цвета и отступы тёмной темы cadcraft (`crates/ui-egui/src/theme.rs`); `apps/gui/src/chrome.rs` — вид стартовой страницы и строки документов по образцу `crates/ui-egui/src/chrome.rs`.
