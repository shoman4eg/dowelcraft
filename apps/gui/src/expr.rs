//! Числовые поля с вычислением выражений: можно набрать `600-4`, `720+18`, `(800-36)/2`, «600 мм + 100»,
//! а в редакторе корпуса — его размеры буквами: `W/2`, `H-100`, `D-T` (русские `Ш`, `В`, `Г`, `Т` — синонимы).
//! Вычислитель взят из QuadFurn (github.com/rbm13s/QuadFurn, `src/ui_panels.rs`) с разрешения автора.

use std::cell::RefCell;

use eframe::egui;

thread_local! {
    /// Переменные формул текущего редактора: (имена, значение, пояснение).
    static VARS: RefCell<Vec<(&'static [&'static str], f64, &'static str)>> = const { RefCell::new(Vec::new()) };
}

/// Переменные для формул на время `body` (редактор корпуса: ширина, высота…).
pub fn with_vars<R>(vars: Vec<(&'static [&'static str], f64, &'static str)>, body: impl FnOnce() -> R) -> R {
    VARS.with(|v| *v.borrow_mut() = vars);
    let r = body();
    VARS.with(|v| v.borrow_mut().clear());
    r
}

/// Подставить переменные и убрать «мм»; неизвестное слово — ошибка (а не тихий пропуск).
fn substitute(input: &str) -> Option<String> {
    let mut out = String::new();
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut String| -> Option<()> {
        if word.is_empty() {
            return Some(());
        }
        let w = word.to_lowercase();
        if !matches!(w.as_str(), "мм" | "mm") {
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

/// Подсказка к числовому полю: что можно написать.
fn formula_help() -> String {
    // Основное имя — английское, русское — в скобках как синоним: «W (Ш) — ширина (800)».
    let vars = VARS.with(|v| v.borrow().iter().map(|(names, x, what)| {
        let alias = if names.len() > 1 { format!(" ({})", names[1..].join(", ")) } else { String::new() };
        format!("{}{alias} — {} ({x})", names[0], crate::i18n::t(what))
    }).collect::<Vec<_>>());
    let mut s = crate::i18n::t("Щёлкните, чтобы ввести число или формулу: 600-4, (800-36)/2. Тяните влево-вправо, чтобы менять.");
    if !vars.is_empty() {
        s.push('\n');
        s.push_str(&crate::i18n::t("В формуле можно: "));
        s.push_str(&vars.join(", "));
    }
    s
}

/// Вычисляет математическое выражение в числовом поле ввода (поддерживает `+`, `-`, `*`, `/`, скобки `(...)`,
/// точку и запятую в качестве десятичного разделителя, а также пробелы и суффиксы единиц измерения).
/// Примеры: `"600+100"` -> `700.0`, `"600-4"` -> `596.0`, `"800-18-18"` -> `764.0`, `"(800-36)/2"` -> `382.0`.
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

/// `DragValue`, понимающий выражения.
pub fn drag<N: egui::emath::Numeric>(value: &mut N) -> egui::DragValue<'_> {
    // Не больше двух знаков после запятой: хвосты вычислений (118.00000000000786) не показываются, значение не меняется.
    egui::DragValue::new(value).custom_parser(eval_math_expression).max_decimals(2)
}

/// Числовое поле, которое можно тянуть мышью, даже если значение каждый кадр берётся заново из проекта:
/// пока тянут, промежуточное число хранится в памяти egui и подставляется вместо значения из проекта.
/// Без этого сдвиг мыши за кадр терялся и поле «не тянулось». Команду вызывающий шлёт, когда отпустили.
pub fn add<N: egui::emath::Numeric + Send + Sync + 'static>(ui: &mut egui::Ui, value: &mut N, style: impl FnOnce(egui::DragValue<'_>) -> egui::DragValue<'_>) -> egui::Response {
    let id = ui.next_auto_id().with("drag_in_progress");
    if let Some(v) = ui.data(|d| d.get_temp::<N>(id)) {
        *value = v;
    }
    // Поле пошире, чтобы набирать формулу было удобно.
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
        assert_eq!(eval_math_expression("600 мм + 100"), Some(700.0));
        assert_eq!(eval_math_expression("(800 - 36) / 2"), Some(382.0));
        assert_eq!(eval_math_expression("1,5*2"), Some(3.0));
        assert_eq!(eval_math_expression("10/0"), None);
        assert_eq!(eval_math_expression("abc"), None);
    }

    /// Значение каждый кадр берётся «из проекта» (10), а мышь тянет поле на 60 точек:
    /// после отпускания должно получиться 10 + 60, а не 10 + сдвиг последнего кадра.
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
        let v = committed.expect("перетаскивание закончено");
        assert!((v - 70.0).abs() < 2.0, "получилось {v}, ждали около 70");
    }

    /// Переменные редактора: Ш/2, В-100, «ш» строчной; неизвестное слово — ошибка.
    #[test]
    fn variables_in_formulas() {
        with_vars(vec![(&["W", "Ш"][..], 800.0, "ширина"), (&["T", "Т"][..], 16.0, "толщина")], || {
            assert_eq!(eval_math_expression("Ш/2"), Some(400.0));
            assert_eq!(eval_math_expression("ш-2*Т"), Some(768.0));
            assert_eq!(eval_math_expression("(W - T) / 2 мм"), Some(392.0));
            assert_eq!(eval_math_expression("Х+1"), None);
        });
        // Вне редактора букв нет.
        assert_eq!(eval_math_expression("Ш/2"), None);
        assert_eq!(eval_math_expression("600 мм + 100"), Some(700.0));
    }
}
