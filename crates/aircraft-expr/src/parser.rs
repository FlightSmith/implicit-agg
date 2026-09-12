//! Recursive-descent parser for the expression grammar.

use crate::ast::{
    BinOp, CmpOp, Coordinate, Expr, Function, Ref, StationLeaf,
};
use crate::lexer::{lex, Spanned, Token};
use aircraft_model::{Code, Diagnostic};

const MAX_NESTING: u32 = 64;

/// Parse a field expression value: the full stored string including `=`.
pub fn parse_field_expression(source: &str) -> Result<Expr, Diagnostic> {
    let Some(formula) = source.trim().strip_prefix('=') else {
        return Err(syntax("an expression value must start with '='", 0));
    };
    parse_formula(formula)
}

/// Parse an already-stripped formula body.
pub fn parse_formula(formula: &str) -> Result<Expr, Diagnostic> {
    let tokens = lex(formula).map_err(|error| {
        syntax(&error.message, error.offset)
    })?;
    if tokens.is_empty() {
        return Err(syntax("an expression must not be empty", 0));
    }

    let mut parser = Parser {
        tokens,
        position: 0,
        depth: 0,
    };
    let expr = parser.comparison()?;
    if let Some(next) = parser.peek() {
        return Err(syntax(
            &format!("unexpected trailing input starting with {}", describe(&next.token)),
            next.offset,
        ));
    }
    Ok(expr)
}

fn syntax(message: &str, offset: usize) -> Diagnostic {
    Diagnostic::error(
        Code::ExpressionSyntax,
        format!("{message} (at offset {offset})"),
    )
}

fn describe(token: &Token) -> String {
    match token {
        Token::Number(value) => format!("the number {value}"),
        Token::Reference(text) => format!("the reference '@{text}'"),
        Token::Ident(name) => format!("the identifier '{name}'"),
        Token::Plus => "'+'".to_string(),
        Token::Minus => "'-'".to_string(),
        Token::Star => "'*'".to_string(),
        Token::Slash => "'/'".to_string(),
        Token::LeftParen => "'('".to_string(),
        Token::RightParen => "')'".to_string(),
        Token::Comma => "','".to_string(),
        Token::Equal => "'=='".to_string(),
        Token::NotEqual => "'!='".to_string(),
        Token::Less => "'<'".to_string(),
        Token::LessEqual => "'<='".to_string(),
        Token::Greater => "'>'".to_string(),
        Token::GreaterEqual => "'>='".to_string(),
    }
}

struct Parser {
    tokens: Vec<Spanned>,
    position: usize,
    depth: u32,
}

impl Parser {
    fn peek(&self) -> Option<&Spanned> {
        self.tokens.get(self.position)
    }

    fn advance(&mut self) -> Option<Spanned> {
        let next = self.tokens.get(self.position).cloned();
        if next.is_some() {
            self.position += 1;
        }
        next
    }

    fn expect(&mut self, token: Token, what: &str) -> Result<Spanned, Diagnostic> {
        match self.peek() {
            Some(spanned) if spanned.token == token => Ok(self.advance().expect("peeked")),
            Some(spanned) => Err(syntax(
                &format!("expected {what}, found {}", describe(&spanned.token)),
                spanned.offset,
            )),
            None => Err(syntax(&format!("expected {what}, found end of expression"), 0)),
        }
    }

    fn comparison(&mut self) -> Result<Expr, Diagnostic> {
        let lhs = self.additive()?;
        let op = match self.peek().map(|s| &s.token) {
            Some(Token::Less) => Some(CmpOp::Less),
            Some(Token::LessEqual) => Some(CmpOp::LessEqual),
            Some(Token::Greater) => Some(CmpOp::Greater),
            Some(Token::GreaterEqual) => Some(CmpOp::GreaterEqual),
            Some(Token::Equal) => Some(CmpOp::Equal),
            Some(Token::NotEqual) => Some(CmpOp::NotEqual),
            _ => None,
        };
        if let Some(op) = op {
            self.advance();
            let rhs = self.additive()?;
            return Ok(Expr::Compare(op, Box::new(lhs), Box::new(rhs)));
        }
        Ok(lhs)
    }

    fn additive(&mut self) -> Result<Expr, Diagnostic> {
        let mut lhs = self.multiplicative()?;
        loop {
            let op = match self.peek().map(|s| &s.token) {
                Some(Token::Plus) => BinOp::Add,
                Some(Token::Minus) => BinOp::Sub,
                _ => return Ok(lhs),
            };
            self.advance();
            let rhs = self.multiplicative()?;
            lhs = Expr::Binary(op, Box::new(lhs), Box::new(rhs));
        }
    }

    fn multiplicative(&mut self) -> Result<Expr, Diagnostic> {
        let mut lhs = self.unary()?;
        loop {
            let op = match self.peek().map(|s| &s.token) {
                Some(Token::Star) => BinOp::Mul,
                Some(Token::Slash) => BinOp::Div,
                _ => return Ok(lhs),
            };
            self.advance();
            let rhs = self.unary()?;
            lhs = Expr::Binary(op, Box::new(lhs), Box::new(rhs));
        }
    }

    fn unary(&mut self) -> Result<Expr, Diagnostic> {
        if matches!(self.peek().map(|s| &s.token), Some(Token::Minus)) {
            self.advance();
            let inner = self.unary()?;
            return Ok(Expr::Negate(Box::new(inner)));
        }
        self.primary()
    }

    fn primary(&mut self) -> Result<Expr, Diagnostic> {
        let Some(spanned) = self.peek().cloned() else {
            return Err(syntax("unexpected end of expression", 0));
        };
        match spanned.token {
            Token::Number(value) => {
                self.advance();
                Ok(Expr::Literal(value))
            }
            Token::Reference(text) => {
                self.advance();
                Ok(Expr::Reference(parse_reference(&text, spanned.offset)?))
            }
            Token::Ident(name) => {
                self.advance();
                let function = Function::from_name(&name).ok_or_else(|| {
                    syntax(&format!("unknown function '{name}'"), spanned.offset)
                })?;
                self.expect(Token::LeftParen, "'(' after a function name")?;
                let mut args = Vec::new();
                if !matches!(self.peek().map(|s| &s.token), Some(Token::RightParen)) {
                    loop {
                        args.push(self.comparison()?);
                        if matches!(self.peek().map(|s| &s.token), Some(Token::Comma)) {
                            self.advance();
                        } else {
                            break;
                        }
                    }
                }
                self.expect(Token::RightParen, "')' to close the argument list")?;
                let (minimum, maximum) = function.arity();
                if args.len() < minimum || args.len() > maximum {
                    return Err(syntax(
                        &format!(
                            "function {} expects {} argument(s), got {}",
                            function.name(),
                            arity_description(minimum, maximum),
                            args.len()
                        ),
                        spanned.offset,
                    ));
                }
                Ok(Expr::Call(function, args))
            }
            Token::LeftParen => {
                self.depth += 1;
                if self.depth > MAX_NESTING {
                    return Err(syntax("expression is nested too deeply", spanned.offset));
                }
                self.advance();
                let inner = self.comparison()?;
                self.expect(Token::RightParen, "')'")?;
                self.depth -= 1;
                Ok(inner)
            }
            other => Err(syntax(
                &format!("unexpected {}", describe(&other)),
                spanned.offset,
            )),
        }
    }
}

fn arity_description(minimum: usize, maximum: usize) -> String {
    if maximum == usize::MAX {
        format!("at least {minimum}")
    } else if minimum == maximum {
        minimum.to_string()
    } else {
        format!("{minimum} to {maximum}")
    }
}

fn parse_reference(text: &str, offset: usize) -> Result<Ref, Diagnostic> {
    if let Some(rest) = text.strip_prefix("param.") {
        if rest.is_empty() {
            return Err(syntax("a @param reference must name a parameter", offset));
        }
        return Ok(Ref::Parameter(rest.to_string()));
    }

    if let Some(rest) = text.strip_prefix("station.") {
        let parts: Vec<&str> = rest.split('.').collect();
        let invalid = || {
            syntax(
                "supported forms: @station.<id>.position.[x|y|z], @station.<id>.chord, \
                 @station.<id>.twist",
                offset,
            )
        };
        return match parts.as_slice() {
            [station, "position", coordinate] => {
                let leaf = match *coordinate {
                    "x" => StationLeaf::PositionX,
                    "y" => StationLeaf::PositionY,
                    "z" => StationLeaf::PositionZ,
                    _ => return Err(invalid()),
                };
                Ok(Ref::Station {
                    station: (*station).to_string(),
                    leaf,
                })
            }
            [station, "chord"] => Ok(Ref::Station {
                station: (*station).to_string(),
                leaf: StationLeaf::Chord,
            }),
            [station, "twist"] => Ok(Ref::Station {
                station: (*station).to_string(),
                leaf: StationLeaf::Twist,
            }),
            _ => Err(invalid()),
        };
    }

    if let Some(rest) = text.strip_prefix("component.") {
        let parts: Vec<&str> = rest.split('.').collect();
        let invalid = || {
            syntax(
                "supported form: @component.<id>.interface.<name>.origin.[x|y|z]",
                offset,
            )
        };
        return match parts.as_slice() {
            [component, "interface", interface, "origin", coordinate] => {
                let coordinate = match *coordinate {
                    "x" => Coordinate::X,
                    "y" => Coordinate::Y,
                    "z" => Coordinate::Z,
                    _ => return Err(invalid()),
                };
                Ok(Ref::InterfaceOrigin {
                    component: (*component).to_string(),
                    interface: (*interface).to_string(),
                    coordinate,
                })
            }
            _ => Err(invalid()),
        };
    }

    Err(syntax(
        "unknown reference prefix; references start with @param, @station, or @component",
        offset,
    ))
}
