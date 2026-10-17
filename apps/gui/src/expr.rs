//! Числовые поля с вычислением выражений: можно набрать `600-4`, `720+18`, `(800-36)/2`, «600 мм + 100».
//! Вычислитель взят из QuadFurn (github.com/rbm13s/QuadFurn, `src/ui_panels.rs`) с разрешения автора.

use eframe::egui;

/// Вычисляет математическое выражение в числовом поле ввода (поддерживает `+`, `-`, `*`, `/`, скобки `(...)`,
/// точку и запятую в качестве десятичного разделителя, а также пробелы и суффиксы единиц измерения).
/// Примеры: `"600+100"` -> `700.0`, `"600-4"` -> `596.0`, `"800-18-18"` -> `764.0`, `"(800-36)/2"` -> `382.0`.
pub fn eval_math_expression(input: &str) -> Option<f64> {
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
}
