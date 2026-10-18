//! Numeric fields that evaluate expressions: you can type `600-4`, `720+18`, `(800-36)/2`, "600 mm + 100",
//! and in the cabinet editor its dimensions as letters: `W/2`, `H-100`, `D-T` (single-letter Russian names are aliases).
//! The evaluator is taken from QuadFurn (github.com/rbm13s/QuadFurn, `src/ui_panels.rs`) with the author's permission.

use std::cell::RefCell;

use eframe::egui;

thread_local! {
    /// Formula variables of the current editor: (names, value, description).
    static VARS: RefCell<Vec<([&'static str; 2], f64, &'static str)>> = const { RefCell::new(Vec::new()) };
}

/// Formula variables for the duration of `body` (cabinet editor: width, height…).
pub fn with_vars<R>(vars: Vec<([&'static str; 2], f64, &'static str)>, body: impl FnOnce() -> R) -> R {
    VARS.with(|v| *v.borrow_mut() = vars);
    let r = body();
    VARS.with(|v| v.borrow_mut().clear());
    r
}

/// Substitute variables and strip "mm"; an unknown word is an error (not silently skipped).
fn substitute(input: &str) -> Option<String> {
    let mut out = String::new();
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut String| -> Option<()> {
        if word.is_empty() {
            return Some(());
        }
        let w = word.to_lowercase();
        if w != "mm" && w != dowel::i18n::bazis("unit.mm") {
            let value = VARS.with(|v| v.borrow().iter().find(|(names, _, _)| names.iter().any(|n| n.to_lowercase() == w)).map(|(_, x, _)| *x))?;
            out.push_str(&format!("({value})"));
        }
        word.clear();
        Some(())
    };
    for c in input.chars() {
        if c.is_alphabetic() {
            word.push(c);
        } else {
            flush(&mut word, &mut out)?;
            out.push(c);
        }
    }
    flush(&mut word, &mut out)?;
    Some(out)
}

/// Hint for a numeric field: what can be typed.
fn formula_help() -> String {
    // The primary name is English, the Russian one is in parentheses as an alias: "W (<ru>) — width (800)".
    let vars = VARS.with(|v| v.borrow().iter().map(|(names, x, what)| {
        let alias = if names.len() > 1 { format!(" ({})", names[1..].join(", ")) } else { String::new() };
        format!("{}{alias} — {} ({x})", names[0], crate::i18n::t(what))
    }).collect::<Vec<_>>());
    let mut s = crate::i18n::t("Click to type a number or a formula: 600-4, (800-36)/2. Drag left or right to change.");
    if !vars.is_empty() {
        s.push('\n');
        s.push_str(&crate::i18n::t("In a formula you can use: "));
        s.push_str(&vars.join(", "));
    }
    s
}

/// Evaluates a math expression in a numeric input field (supports `+`, `-`, `*`, `/`, parentheses `(...)`,
/// dot and comma as decimal separator, plus spaces and unit suffixes).
/// Examples: `"600+100"` -> `700.0`, `"600-4"` -> `596.0`, `"800-18-18"` -> `764.0`, `"(800-36)/2"` -> `382.0`.
pub fn eval_math_expression(input: &str) -> Option<f64> {
    let input = substitute(input)?;
    let cleaned: String = input
        .chars()
        .map(|c| match c {
            ',' => '.',
            '−' | '–' | '—' => '-',
            '×' => '*',
            '÷' => '/',
            other => other,
        })
        .filter(|c| matches!(c, '0'..='9' | '.' | '+' | '-' | '*' | '/' | '(' | ')'))
        .collect();

    if cleaned.is_empty() {
        return None;
    }

    struct Parser<'a> {
        bytes: &'a [u8],
        pos: usize,
    }

    impl<'a> Parser<'a> {
        fn peek(&self) -> Option<u8> {
            self.bytes.get(self.pos).copied()
        }

        fn parse_expr(&mut self) -> Option<f64> {
            let mut left = self.parse_term()?;
            while let Some(op) = self.peek() {
                if op == b'+' || op == b'-' {
                    self.pos += 1;
                    let right = self.parse_term()?;
                    if op == b'+' {
                        left += right;
                    } else {
                        left -= right;
                    }
                } else {
                    break;
                }
            }
            Some(left)
        }

        fn parse_term(&mut self) -> Option<f64> {
            let mut left = self.parse_factor()?;
            while let Some(op) = self.peek() {
                if op == b'*' || op == b'/' {
                    self.pos += 1;
                    let right = self.parse_factor()?;
                    if op == b'*' {
                        left *= right;
                    } else {
                        if right.abs() < 1e-12 {
                            return None;
                        }
                        left /= right;
                    }
                } else {
                    break;
                }
            }
            Some(left)
        }

        fn parse_factor(&mut self) -> Option<f64> {
            match self.peek()? {
                b'+' => {
                    self.pos += 1;
                    self.parse_factor()
                }
                b'-' => {
                    self.pos += 1;
                    Some(-self.parse_factor()?)
                }
                b'(' => {
                    self.pos += 1;
                    let val = self.parse_expr()?;
                    if self.peek() == Some(b')') {
                        self.pos += 1;
                        Some(val)
                    } else {
                        None
                    }
                }
                _ => self.parse_number(),
            }
        }

        fn parse_number(&mut self) -> Option<f64> {
            let start = self.pos;
            let mut has_digit = false;
            let mut has_dot = false;
            while let Some(b) = self.peek() {
                if b.is_ascii_digit() {
                    has_digit = true;
                    self.pos += 1;
                } else if b == b'.' && !has_dot {
                    has_dot = true;
                    self.pos += 1;
                } else {
                    break;
                }
            }
            if !has_digit {
                return None;
            }
            let s = std::str::from_utf8(&self.bytes[start..self.pos]).ok()?;
            s.parse::<f64>().ok()
        }
    }

    let mut parser = Parser {
        bytes: cleaned.as_bytes(),
        pos: 0,
    };
    let val = parser.parse_expr()?;
    if parser.pos == parser.bytes.len() && val.is_finite() {
        Some(val)
    } else {
        None
    }
}

/// `DragValue` that understands expressions.
pub fn drag<N: egui::emath::Numeric>(value: &mut N) -> egui::DragValue<'_> {
    // At most two decimals: calculation tails (118.00000000000786) aren't shown, the value is unchanged.
    egui::DragValue::new(value).custom_parser(eval_math_expression).max_decimals(2)
}

/// A numeric field that can be dragged with the mouse even if its value is re-read from the project every frame:
/// while dragging, the intermediate number is kept in egui memory and used instead of the project value.
/// Without this the per-frame mouse delta was lost and the field "wouldn't drag". The caller sends the command on release.
pub fn add<N: egui::emath::Numeric + Send + Sync + 'static>(ui: &mut egui::Ui, value: &mut N, style: impl FnOnce(egui::DragValue<'_>) -> egui::DragValue<'_>) -> egui::Response {
    let id = ui.next_auto_id().with("drag_in_progress");
    if let Some(v) = ui.data(|d| d.get_temp::<N>(id)) {
        *value = v;
    }
    // Wider field so typing a formula is comfortable.
    let r = ui.scope(|ui| {
        let w = ui.spacing().interact_size.x.max(80.0);
        ui.spacing_mut().interact_size.x = w;
        ui.add(style(drag(value)))
    })
    .inner;
    let r = r.on_hover_text(formula_help());
    if r.dragged() {
        ui.data_mut(|d| d.insert_temp(id, *value));
    } else {
        ui.data_mut(|d| d.remove::<N>(id));
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expressions() {
        assert_eq!(eval_math_expression("600+100"), Some(700.0));
        assert_eq!(eval_math_expression("600-4"), Some(596.0));
        assert_eq!(eval_math_expression("600 mm + 100"), Some(700.0));
        assert_eq!(eval_math_expression(&format!("600 {} + 100", dowel::i18n::bazis("unit.mm"))), Some(700.0));
        assert_eq!(eval_math_expression("(800 - 36) / 2"), Some(382.0));
        assert_eq!(eval_math_expression("1,5*2"), Some(3.0));
        assert_eq!(eval_math_expression("10/0"), None);
        assert_eq!(eval_math_expression("abc"), None);
    }

    /// The value is re-read "from the project" every frame (10), and the mouse drags the field by 60 points:
    /// after release it must be 10 + 60, not 10 + the last frame's delta.
    #[test]
    fn drag_survives_value_from_project() {
        let ctx = egui::Context::default();
        let mut field_rect = egui::Rect::NOTHING;
        let mut committed = None;
        let frame = |events: Vec<egui::Event>, field_rect: &mut egui::Rect, committed: &mut Option<f64>| {
            let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 200.0))), events, ..Default::default() };
            let mut out = ctx.run_ui(input, |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    let mut v = 10.0f64;
                    let r = add(ui, &mut v, |d| d.speed(1.0));
                    *field_rect = r.rect;
                    if r.drag_stopped() {
                        *committed = Some(v);
                    }
                });
            });
            out.textures_delta.clear();
        };
        frame(vec![], &mut field_rect, &mut committed);
        let start = field_rect.center();
        let press = |pressed| egui::Event::PointerButton { pos: start, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
        frame(vec![egui::Event::PointerMoved(start), press(true)], &mut field_rect, &mut committed);
        for k in 1..=6 {
            frame(vec![egui::Event::PointerMoved(start + egui::vec2(10.0 * k as f32, 0.0))], &mut field_rect, &mut committed);
        }
        let end = start + egui::vec2(60.0, 0.0);
        frame(vec![egui::Event::PointerButton { pos: end, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() }], &mut field_rect, &mut committed);
        let v = committed.expect("drag finished");
        assert!((v - 70.0).abs() < 2.0, "got {v}, expected about 70");
    }

    /// Editor variables via Russian aliases (`<ru>/2`, `<ru>-100`, lowercase too); an unknown word is an error.
    #[test]
    fn variables_in_formulas() {
        let (w, t, mm) = (dowel::i18n::bazis("alias.W"), dowel::i18n::bazis("alias.T"), dowel::i18n::bazis("unit.mm"));
        with_vars(vec![(["W", w], 800.0, "width"), (["T", t], 16.0, "thickness")], || {
            assert_eq!(eval_math_expression(&format!("{w}/2")), Some(400.0));
            assert_eq!(eval_math_expression(&format!("{}-2*{t}", w.to_lowercase())), Some(768.0));
            assert_eq!(eval_math_expression(&format!("(W - T) / 2 {mm}")), Some(392.0));
            assert_eq!(eval_math_expression("Q+1"), None);
        });
        // Outside the editor there are no letters.
        assert_eq!(eval_math_expression(&format!("{w}/2")), None);
    }
}
