//! Tokenizer for the expression language.
//!
//! References (`@...`) are lexed as a single token using the
//! longest-identifier rule: whitespace or an operator ends a reference.

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Number(f64),
    /// A `@`-reference without the `@`, e.g. `param.wing.rootChord`.
    Reference(String),
    /// A bare identifier, only valid as a function name.
    Ident(String),
    Plus,
    Minus,
    Star,
    Slash,
    LeftParen,
    RightParen,
    Comma,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}

#[derive(Debug, Clone)]
pub struct Spanned {
    pub token: Token,
    pub offset: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LexError {
    pub offset: usize,
    pub message: String,
}

fn is_reference_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-'
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

pub fn lex(source: &str) -> Result<Vec<Spanned>, LexError> {
    let mut tokens = Vec::new();
    let chars: Vec<(usize, char)> = source.char_indices().collect();
    let mut index = 0;

    while index < chars.len() {
        let (offset, c) = chars[index];
        if c.is_whitespace() {
            index += 1;
            continue;
        }

        match c {
            '+' => tokens.push(Spanned { token: Token::Plus, offset }),
            '-' => tokens.push(Spanned { token: Token::Minus, offset }),
            '*' => tokens.push(Spanned { token: Token::Star, offset }),
            '/' => tokens.push(Spanned { token: Token::Slash, offset }),
            '(' => tokens.push(Spanned { token: Token::LeftParen, offset }),
            ')' => tokens.push(Spanned { token: Token::RightParen, offset }),
            ',' => tokens.push(Spanned { token: Token::Comma, offset }),
            '@' => {
                let start = index + 1;
                let mut end = start;
                while end < chars.len() && is_reference_char(chars[end].1) {
                    end += 1;
                }
                if end == start {
                    return Err(LexError {
                        offset,
                        message: "a reference must name a parameter, station, or component".to_string(),
                    });
                }
                let text: String = chars[start..end].iter().map(|(_, c)| c).collect();
                tokens.push(Spanned {
                    token: Token::Reference(text),
                    offset,
                });
                index = end;
                continue;
            }
            '0'..='9' => {
                let mut end = index;
                while end < chars.len() && (chars[end].1.is_ascii_digit() || chars[end].1 == '.') {
                    end += 1;
                }
                // Optional exponent.
                if end < chars.len()
                    && (chars[end].1 == 'e' || chars[end].1 == 'E')
                    && end + 1 < chars.len()
                {
                    let mut exponent_end = end + 1;
                    if chars[exponent_end].1 == '+' || chars[exponent_end].1 == '-' {
                        exponent_end += 1;
                    }
                    if exponent_end < chars.len() && chars[exponent_end].1.is_ascii_digit() {
                        end = exponent_end;
                        while end < chars.len() && chars[end].1.is_ascii_digit() {
                            end += 1;
                        }
                    }
                }
                let text: String = chars[index..end].iter().map(|(_, c)| c).collect();
                let value: f64 = text.parse().map_err(|_| LexError {
                    offset,
                    message: format!("invalid number {text:?}"),
                })?;
                tokens.push(Spanned {
                    token: Token::Number(value),
                    offset,
                });
                index = end;
                continue;
            }
            _ if is_ident_start(c) => {
                let mut end = index;
                while end < chars.len() && (chars[end].1.is_ascii_alphanumeric() || chars[end].1 == '_')
                {
                    end += 1;
                }
                let text: String = chars[index..end].iter().map(|(_, c)| c).collect();
                tokens.push(Spanned {
                    token: Token::Ident(text),
                    offset,
                });
                index = end;
                continue;
            }
            '=' => {
                if index + 1 < chars.len() && chars[index + 1].1 == '=' {
                    tokens.push(Spanned { token: Token::Equal, offset });
                    index += 2;
                    continue;
                }
                return Err(LexError {
                    offset,
                    message: "single '=' is not an operator; use '==' to compare".to_string(),
                });
            }
            '!' => {
                if index + 1 < chars.len() && chars[index + 1].1 == '=' {
                    tokens.push(Spanned { token: Token::NotEqual, offset });
                    index += 2;
                    continue;
                }
                return Err(LexError {
                    offset,
                    message: "'!' alone is not an operator; use '!='".to_string(),
                });
            }
            '<' => {
                if index + 1 < chars.len() && chars[index + 1].1 == '=' {
                    tokens.push(Spanned { token: Token::LessEqual, offset });
                    index += 2;
                    continue;
                }
                tokens.push(Spanned { token: Token::Less, offset });
            }
            '>' => {
                if index + 1 < chars.len() && chars[index + 1].1 == '=' {
                    tokens.push(Spanned { token: Token::GreaterEqual, offset });
                    index += 2;
                    continue;
                }
                tokens.push(Spanned { token: Token::Greater, offset });
            }
            _ => {
                return Err(LexError {
                    offset,
                    message: format!("unexpected character {c:?}"),
                });
            }
        }
        index += 1;
    }

    Ok(tokens)
}
