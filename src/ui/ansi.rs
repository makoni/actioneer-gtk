#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnsiColor {
    Basic(u8),
    Bright(u8),
}

impl AnsiColor {
    pub fn to_css(self) -> &'static str {
        match self {
            AnsiColor::Basic(0) => "#000000",
            AnsiColor::Basic(1) => "#cc0000",
            AnsiColor::Basic(2) => "#4e9a06",
            AnsiColor::Basic(3) => "#c4a000",
            AnsiColor::Basic(4) => "#3465a4",
            AnsiColor::Basic(5) => "#75507b",
            AnsiColor::Basic(6) => "#06989a",
            AnsiColor::Basic(7) => "#d3d7cf",
            AnsiColor::Bright(0) => "#555753",
            AnsiColor::Bright(1) => "#ef2929",
            AnsiColor::Bright(2) => "#8ae234",
            AnsiColor::Bright(3) => "#fce94f",
            AnsiColor::Bright(4) => "#729fcf",
            AnsiColor::Bright(5) => "#ad7fa8",
            AnsiColor::Bright(6) => "#34e2e2",
            AnsiColor::Bright(7) => "#eeeeec",
            _ => "#ffffff",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct AnsiStyle {
    pub bold: bool,
    pub underline: bool,
    pub fg: Option<AnsiColor>,
    pub bg: Option<AnsiColor>,
}

impl AnsiStyle {
    pub fn is_default(&self) -> bool {
        !self.bold && !self.underline && self.fg.is_none() && self.bg.is_none()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnsiSpan {
    pub text: String,
    pub style: AnsiStyle,
}

pub fn parse_ansi(input: &str) -> Vec<AnsiSpan> {
    let mut spans = Vec::new();
    let mut style = AnsiStyle::default();
    let mut current = String::new();

    let bytes = input.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
            if let Some(offset) = bytes[i + 2..].iter().position(|&b| b == b'm') {
                if !current.is_empty() {
                    spans.push(AnsiSpan {
                        text: std::mem::take(&mut current),
                        style: style.clone(),
                    });
                }

                let seq = &input[i + 2..i + 2 + offset];
                apply_sgr_sequence(seq, &mut style);

                i = i + 2 + offset + 1;
                continue;
            }
        }

        let ch = input[i..].chars().next().unwrap();
        current.push(ch);
        i += ch.len_utf8();
    }

    if !current.is_empty() {
        spans.push(AnsiSpan {
            text: current,
            style,
        });
    }

    spans
}

fn apply_sgr_sequence(seq: &str, style: &mut AnsiStyle) {
    if seq.is_empty() {
        *style = AnsiStyle::default();
        return;
    }

    let mut any = false;
    for part in seq.split(';') {
        let code = if part.is_empty() {
            0
        } else if let Ok(value) = part.parse::<u16>() {
            value
        } else {
            continue;
        };
        any = true;
        apply_sgr_code(code, style);
    }

    if !any {
        *style = AnsiStyle::default();
    }
}

fn apply_sgr_code(code: u16, style: &mut AnsiStyle) {
    match code {
        0 => *style = AnsiStyle::default(),
        1 => style.bold = true,
        4 => style.underline = true,
        22 => style.bold = false,
        24 => style.underline = false,
        30..=37 => style.fg = Some(AnsiColor::Basic((code - 30) as u8)),
        90..=97 => style.fg = Some(AnsiColor::Bright((code - 90) as u8)),
        39 => style.fg = None,
        40..=47 => style.bg = Some(AnsiColor::Basic((code - 40) as u8)),
        100..=107 => style.bg = Some(AnsiColor::Bright((code - 100) as u8)),
        49 => style.bg = None,
        _ => {
            // Ignore unsupported codes
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_text_as_single_span() {
        let spans = parse_ansi("hello world");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].text, "hello world");
        assert!(spans[0].style.is_default());
    }

    #[test]
    fn parses_basic_color_and_reset() {
        let spans = parse_ansi("hi \u{1b}[31mred\u{1b}[0m ok");
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].text, "hi ");
        assert!(spans[0].style.is_default());
        assert_eq!(spans[1].text, "red");
        assert_eq!(spans[1].style.fg, Some(AnsiColor::Basic(1)));
        assert_eq!(spans[2].text, " ok");
        assert!(spans[2].style.is_default());
    }

    #[test]
    fn parses_bold_underline_and_color_resets() {
        let spans = parse_ansi("A\u{1b}[1;4;32mB\u{1b}[22;24;39mC");
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].text, "A");
        assert!(spans[0].style.is_default());
        assert_eq!(spans[1].text, "B");
        assert!(spans[1].style.bold);
        assert!(spans[1].style.underline);
        assert_eq!(spans[1].style.fg, Some(AnsiColor::Basic(2)));
        assert_eq!(spans[2].text, "C");
        assert!(spans[2].style.is_default());
    }

    
}
