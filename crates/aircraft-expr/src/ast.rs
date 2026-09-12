//! Abstract syntax tree of the expression language and its printer.

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Literal(f64),
    Reference(Ref),
    Negate(Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    Compare(CmpOp, Box<Expr>, Box<Expr>),
    Call(Function, Vec<Expr>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Ref {
    Parameter(String),
    Station {
        station: String,
        leaf: StationLeaf,
    },
    InterfaceOrigin {
        component: String,
        interface: String,
        coordinate: Coordinate,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StationLeaf {
    PositionX,
    PositionY,
    PositionZ,
    Chord,
    Twist,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Coordinate {
    X,
    Y,
    Z,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Equal,
    NotEqual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Function {
    Min,
    Max,
    Clamp,
    Abs,
    Sqrt,
    Pow,
    Sin,
    Cos,
    Tan,
    Lerp,
}

impl Function {
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "min" => Function::Min,
            "max" => Function::Max,
            "clamp" => Function::Clamp,
            "abs" => Function::Abs,
            "sqrt" => Function::Sqrt,
            "pow" => Function::Pow,
            "sin" => Function::Sin,
            "cos" => Function::Cos,
            "tan" => Function::Tan,
            "lerp" => Function::Lerp,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Function::Min => "min",
            Function::Max => "max",
            Function::Clamp => "clamp",
            Function::Abs => "abs",
            Function::Sqrt => "sqrt",
            Function::Pow => "pow",
            Function::Sin => "sin",
            Function::Cos => "cos",
            Function::Tan => "tan",
            Function::Lerp => "lerp",
        }
    }

    /// (minimum, maximum) argument count; `usize::MAX` means unbounded.
    pub fn arity(self) -> (usize, usize) {
        match self {
            Function::Min | Function::Max => (1, usize::MAX),
            Function::Clamp | Function::Lerp => (3, 3),
            Function::Abs | Function::Sqrt | Function::Sin | Function::Cos | Function::Tan => {
                (1, 1)
            }
            Function::Pow => (2, 2),
        }
    }
}

impl std::fmt::Display for Ref {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Ref::Parameter(id) => write!(f, "@param.{id}"),
            Ref::Station { station, leaf } => {
                let leaf = match leaf {
                    StationLeaf::PositionX => "position.x",
                    StationLeaf::PositionY => "position.y",
                    StationLeaf::PositionZ => "position.z",
                    StationLeaf::Chord => "chord",
                    StationLeaf::Twist => "twist",
                };
                write!(f, "@station.{station}.{leaf}")
            }
            Ref::InterfaceOrigin {
                component,
                interface,
                coordinate,
            } => {
                let coordinate = match coordinate {
                    Coordinate::X => "x",
                    Coordinate::Y => "y",
                    Coordinate::Z => "z",
                };
                write!(f, "@component.{component}.interface.{interface}.origin.{coordinate}")
            }
        }
    }
}

/// Render an expression back to formula source (without the leading `=`).
/// Parenthesization is conservative and preserves structure exactly.
pub fn print(expr: &Expr) -> String {
    match expr {
        Expr::Literal(value) => {
            let mut rendered = format!("{value}");
            if rendered.contains('e') || rendered.contains('E') {
                return rendered;
            }
            if !rendered.contains('.') {
                rendered.push_str(".0");
            }
            rendered
        }
        Expr::Reference(reference) => reference.to_string(),
        Expr::Negate(inner) => format!("-{}", print(inner)),
        Expr::Binary(op, lhs, rhs) => {
            format!("({} {} {})", print(lhs), op_symbol(*op), print(rhs))
        }
        Expr::Compare(op, lhs, rhs) => {
            format!("({} {} {})", print(lhs), cmp_symbol(*op), print(rhs))
        }
        Expr::Call(function, args) => {
            let args = args
                .iter()
                .map(print)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}({})", function.name(), args)
        }
    }
}

fn op_symbol(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
    }
}

fn cmp_symbol(op: CmpOp) -> &'static str {
    match op {
        CmpOp::Less => "<",
        CmpOp::LessEqual => "<=",
        CmpOp::Greater => ">",
        CmpOp::GreaterEqual => ">=",
        CmpOp::Equal => "==",
        CmpOp::NotEqual => "!=",
    }
}
