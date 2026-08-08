use chumsky::input::*;
use chumsky::pratt::*;
use chumsky::prelude::*;
use dumpster::{Trace, TraceWith, Visitor, unsync::Gc};
use logos::Logos;
use std::borrow::Cow;
use std::fmt;
use std::fmt::Display;
use std::fmt::Write;
use std::ops::Add;
use std::ops::Sub;
use std::rc::Rc;

use crate::Symbol;

#[derive(Logos, Clone, Debug, PartialEq)]
#[logos(skip r"[ \t\r\n\f]+")]
#[logos(skip(r"//[^\r\n]*", allow_greedy = true))]
#[logos(skip r"/\*([^*]|\*[^/])*\*/")]
pub enum Token<'a> {
    Error,

    #[token("false", |_| false)]
    #[token("true", |_| true)]
    Bool(bool),

    #[token("null")]
    Null,

    #[token("=>")]
    LambdaArrow,

    #[token("{")]
    BraceOpen,

    #[token("}")]
    BraceClose,

    #[token("[")]
    BracketOpen,

    #[token("]")]
    BracketClose,

    #[token("(")]
    ParenOpen,

    #[token(")")]
    ParenClose,

    #[token(",")]
    Comma,

    #[token(";")]
    Semicolon,

    #[token(".")]
    Dot,

    #[regex(r"(>=|<=|==|!=|&&|\|\||\+\+|\.\.|:=|->)", priority = 3)]
    #[regex(r"[+\-*/%&|^!<>?=:]")]
    Op(&'a str),

    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*")]
    Ident(&'a str),

    #[regex(r"[0-9]+", |lex| match lex.slice().parse() {
        Ok(num) => Some(Number::Int(num)),
        Err(_) => Some(Number::Float(lex.slice().parse().unwrap())),
    }, priority = 3)]
    // * because Algodoo allows simply `0x` or `0b`.
    #[regex(r"0x[0-9a-fA-F]*", |lex| Number::Int(i32::from_str_radix(&lex.slice()[lex.slice().len().saturating_sub(8).max(2)..], 16).unwrap_or(0)), priority = 4)]
    #[regex(r"0b[01]*", |lex| Number::Int(i32::from_str_radix(&lex.slice()[lex.slice().len().saturating_sub(8).max(2)..], 2).unwrap_or(0)), priority = 4)]
    Int(Number),

    #[token("-inf", |_| f32::NEG_INFINITY)]
    #[token("+inf", |_| f32::INFINITY)]
    #[token("∞", |_| f32::INFINITY)]
    #[token("NaN", |_| f32::NAN)]
    #[regex(r"(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?", |lex| lex.slice().parse::<f32>().expect("float parse error: unexpected!"))]
    Float(f32),

    #[regex(r#""(?:[^"\\]|\\.)*""#, |lex| {
        let slice = lex.slice();
        if !slice.contains('\\') {
            Cow::Borrowed(&slice[1..slice.len() - 1])
        } else {
            let mut result = String::new();
            let mut chars = slice[1..slice.len() - 1].chars();
            while let Some(c) = chars.next() {
                if c == '\\' {
                    if let Some(escaped) = chars.next() {
                        match escaped {
                            'n' => result.push('\n'),
                            't' => result.push('\t'),
                            '\\' => result.push('\\'),
                            '"' => result.push('"'),
                            _ => {}
                        }
                    }
                } else {
                    result.push(c);
                }
            }
            Cow::Owned(result)
        }
    })]
    // Verbatim string literal, e.g. `@"C:\path\to\file.txt"`
    #[regex(r#"@"(?:[^"]|"")*""#, |lex| {
        let slice = lex.slice();
        let mut result = String::new();
        let mut chars = slice[2..slice.len() - 1].chars(); // this removes the leading @" and trailing "
        while let Some(c) = chars.next() {
            // because of the regex there can never be a lone double quote
            if c == '"' {
                if !matches!(chars.next(), Some('"')) {
                    unreachable!("unexpected lone double quote in verbatim string literal");
                }
                result.push('"');
            } else {
                result.push(c);
            }
        }
        Cow::Owned(result)
    })]
    Str(Cow<'a, str>),
}

impl<'a> Display for Token<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Token::Error => write!(f, "<error>"),
            Token::Bool(b) => write!(f, "{}", b),
            Token::Null => write!(f, "null"),
            Token::LambdaArrow => write!(f, "=>"),
            /*Token::Arrow => write!(f, "->"),
            Token::Declare => write!(f, ":="),
            Token::Assign => write!(f, "="),*/
            Token::BraceOpen => write!(f, "{{"),
            Token::BraceClose => write!(f, "}}"),
            Token::BracketOpen => write!(f, "["),
            Token::BracketClose => write!(f, "]"),
            Token::ParenOpen => write!(f, "("),
            Token::ParenClose => write!(f, ")"),
            Token::Comma => write!(f, ","),
            Token::Semicolon => write!(f, ";"),
            Token::Dot => write!(f, "."),
            Token::Op(op) => write!(f, "{}", op),
            Token::Ident(ident) => write!(f, "{}", ident),
            Token::Int(num) => match num {
                Number::Int(n) => write!(f, "{}", n),
                Number::Float(n) => write!(f, "{}", n),
            },
            Token::Float(flt) => write!(f, "{}", flt),
            Token::Str(s) => write!(f, "\"{}\"", s), // todo: escapes
        }
    }
}

fn read_auto_encoding(source: &[u8]) -> Cow<'_, str> {
    // try decoding utf8 then 1252
    match std::str::from_utf8(source) {
        Ok(s) => Cow::Borrowed(s),
        Err(_) => {
            let (cow, _, _) = encoding_rs::WINDOWS_1252.decode(source);
            cow
        }
    }
}

pub type Span = SimpleSpan;
pub type Spanned<T> = (T, Span);

// in increasing order of Algodoo precedence
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum BinaryOp {
    // < ->
    Assign,
    Declare,
    // < 0,
    ClassAssign,
    // 2 - <ternary goes here>
    // 3 - or
    Or,
    // 4 - and
    And,
    // 5 - range (declared in thyme.cfg using `infix`)
    Range,
    // 8 - eq comp
    Eq,
    NotEq,
    // 9 - rel comp
    Less,
    LessEq,
    Greater,
    GreaterEq,
    // 10 - list concat
    ListConcat,
    // 11 - addition
    Add,
    Sub,
    // 12 - multiplication
    Mul,
    Div,
    Mod,
    // 13 - <unary>
    // 14 - exponentiation (right associative)
    Pow,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnaryOp {
    Not,
    Neg,
    Pos,
}

/// A Thyme number. Algodoo keeps integers and floats as distinct runtime types.
#[derive(Copy, Clone, Debug, PartialEq, Trace)]
pub enum Number {
    Int(i32),
    Float(f32),
}

impl Number {
    /// Returns the largest integer less than or equal to this number. Returns 0 for NaN and negative infinity, and i32::MAX for positive infinity.
    pub fn floor(self) -> i32 {
        match self {
            Number::Int(n) => n,
            Number::Float(f) => i32::try_from(f.floor() as i64).unwrap_or_else(|_| {
                if f.is_nan() || f.is_sign_negative() {
                    0
                } else {
                    i32::MAX
                }
            }),
        }
    }

    pub fn to_f32_lossy(self) -> f32 {
        match self {
            Number::Int(n) => n as f32,
            Number::Float(f) => f,
        }
    }

    /*pub fn thyme_display(&self) -> Cow<'_, str> {
        match self {
            Number::Int(n) => n.to_string().into(),
            Number::Float(f) => match f {
                f if f.is_nan() => "NaN".into(),
                f if f.is_infinite() && f.is_sign_positive() => "inf".into(),
                f if f.is_infinite() && f.is_sign_negative() => "-inf".into(),
                f => f.to_string().into(),
            }
        }
    }*/
}

impl Display for Number {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Number::Int(n) => write!(f, "{}", n),
            Number::Float(flt) => match flt {
                x if x.is_nan() => write!(f, "NaN"),
                x if x.is_infinite() && x.is_sign_positive() => write!(f, "inf"),
                x if x.is_infinite() && x.is_sign_negative() => write!(f, "-inf"),
                x => write!(f, "{}", x),
            }
        }
    }
}

impl From<i32> for Number {
    fn from(n: i32) -> Self {
        Number::Int(n)
    }
}

impl From<f32> for Number {
    fn from(n: f32) -> Self {
        Number::Float(n)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Literal {
    Null,
    Bool(bool),
    Number(Number),
    Str(Rc<str>),
}

#[derive(Clone, Debug)]
pub struct UserFunctionDef {
    pub params: Rc<[Symbol]>,
    pub body: Rc<Spanned<Expr>>,
}

#[derive(Debug)]
pub enum Expr {
    Error,
    // this is semantically useless but we need it to be able to pretty-print ASTs at runtime
    Parenthesized(Box<Spanned<Self>>),
    Value(Literal),
    List(Vec<Spanned<Self>>),
    Symbol(Symbol),
    /// a.b
    Member(Box<Spanned<Self>>, Spanned<Symbol>),
    /// f(a,b,c) or simply f a
    Call(Box<Spanned<Self>>, Spanned<Vec<Spanned<Self>>>),
    Binary(Box<Spanned<Self>>, BinaryOp, Box<Spanned<Self>>),
    Unary(UnaryOp, Box<Spanned<Self>>),
    // cond ? true_expr : false_expr
    Ternary(Box<Spanned<Self>>, Box<Spanned<Self>>, Box<Spanned<Self>>),
    /// (a,b,c)=>{...} or simply {...}
    Func(UserFunctionDef),
    /// a; b (optional trailing ;)
    Seq(Vec<Box<Spanned<Self>>>),
}

pub struct PrettyPrinter<W> {
    out: W,
    indent: usize,
}

impl<W: Write> PrettyPrinter<W> {
    pub fn new(out: W) -> Self {
        Self { out, indent: 0 }
    }

    fn write(&mut self, s: &str) -> fmt::Result {
        self.out.write_str(s)
    }

    fn write_char(&mut self, c: char) -> fmt::Result {
        self.out.write_char(c)
    }

    fn line(&mut self, s: &str) -> fmt::Result {
        writeln!(self.out, "{:indent$}{s}", "", indent = self.indent * 4)
    }

    fn indented(
        &mut self,
        f: impl FnOnce(&mut Self) -> fmt::Result,
    ) -> fmt::Result {
        self.indent += 1;
        let result = f(self);
        self.indent -= 1;
        result
    }
}

impl Expr {
    pub fn pretty_print(&self) -> String {
        let mut result = String::new();
        self.pretty(&mut PrettyPrinter::new(&mut result)).unwrap();
        result
    }

    pub fn pretty(&self, printer: &mut PrettyPrinter<impl Write>) -> fmt::Result {
        match self {
            Expr::Error => printer.write("<error>"),
            Expr::Parenthesized(expr) => {
                printer.write_char('(')?;
                expr.0.pretty(printer)?;
                printer.write_char(')')
            }
            Expr::Value(lit) => match lit {
                Literal::Null => printer.write("null"),
                Literal::Bool(b) => write!(printer.out, "{}", b),
                Literal::Number(n) => write!(printer.out, "{}", n),
                Literal::Str(s) => {
                    printer.write_char('"')?;
                    for c in s.chars() {
                        match c {
                            '\n' => printer.write("\\n")?,
                            '\t' => printer.write("\\t")?,
                            '\\' => printer.write("\\\\")?,
                            '"' => printer.write("\\\"")?,
                            _ => printer.write_char(c)?,
                        }
                    }
                    printer.write_char('"')
                }
            },
            Expr::List(items) => {
                printer.write_char('[')?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        printer.write(", ")?;
                    }
                    item.0.pretty(printer)?;
                }
                printer.write_char(']')
            }
            Expr::Symbol(sym) => printer.write(sym.as_str()),
            Expr::Member(object, member) => {
                object.0.pretty(printer)?;
                printer.write_char('.')?;
                printer.write(member.0.as_str())
            }
            Expr::Call(func, args) => {
                func.0.pretty(printer)?;
                printer.write_char('(')?;
                for (i, arg) in args.0.iter().enumerate() {
                    if i > 0 {
                        printer.write(", ")?;
                    }
                    arg.0.pretty(printer)?;
                }
                printer.write_char(')')
            }
            Expr::Binary(lhs, op, rhs) => {
                lhs.0.pretty(printer)?;
                printer.write_char(' ')?;
                use BinaryOp::*;
                printer.write(match op {
                    Assign => "=",
                    Declare => ":=",
                    ClassAssign => "->",
                    Or => "||",
                    And => "&&",
                    Range => "..",
                    Eq => "==",
                    NotEq => "!=",
                    Less => "<",
                    LessEq => "<=",
                    Greater => ">",
                    GreaterEq => ">=",
                    ListConcat => "++",
                    Add => "+",
                    Sub => "-",
                    Mul => "*",
                    Div => "/",
                    Mod => "%",
                    Pow => "^",
                })?;
                printer.write_char(' ')?;
                rhs.0.pretty(printer)
            }
            Expr::Unary(op, expr) => {
                use UnaryOp::*;
                printer.write_char(match op {
                    Not => '!',
                    Neg => '-',
                    Pos => '+',
                })?;
                expr.0.pretty(printer)
            }
            Expr::Ternary(cond, true_expr, false_expr) => {
                cond.0.pretty(printer)?;
                printer.write(" ? ")?;
                true_expr.0.pretty(printer)?;
                printer.write(" : ")?;
                false_expr.0.pretty(printer)
            }
            Expr::Func(func_def) => {
                func_def.pretty(printer)
            }
            Expr::Seq(stmts) => {
                for (i, stmt) in stmts.iter().enumerate() {
                    if i > 0 {
                        printer.write_char(';')?;
                        printer.line("")?;
                    }
                    stmt.0.pretty(printer)?;
                }
                Ok(())
            }
        }
    }
}

impl UserFunctionDef {
    pub fn pretty(&self, printer: &mut PrettyPrinter<impl Write>) -> fmt::Result {
        printer.write_char('(')?;
        for (i, param) in self.params.iter().enumerate() {
            if i > 0 {
                printer.write(", ")?;
            }
            printer.write(param.as_str())?;
        }
        printer.write(") => ")?;
        self.body.0.pretty(printer)
    }
}

/*
block_inner
    = expression
      { ";" expression }
      [ ";" ]
    ;
     */
fn block_parser<'tokens, 'src: 'tokens, I>()
-> impl Parser<'tokens, I, Spanned<Expr>, extra::Err<Rich<'tokens, Token<'src>, Span>>> + Clone
where
    I: ValueInput<'tokens, Token = Token<'src>, Span = Span>,
{
    block_with_expr(expr_parser())
}

fn block_with_expr<'tokens, 'src: 'tokens, I, P>(
    expr: P,
) -> impl Parser<'tokens, I, Spanned<Expr>, extra::Err<Rich<'tokens, Token<'src>, Span>>> + Clone
where
    I: ValueInput<'tokens, Token = Token<'src>, Span = Span>,
    P: Parser<'tokens, I, Spanned<Expr>, extra::Err<Rich<'tokens, Token<'src>, Span>>> + Clone,
{
    expr.clone()
        .separated_by(just(Token::Semicolon))
        .allow_trailing()
        .collect::<Vec<_>>()
        .map_with(|stmts, e| {
            (
                Expr::Seq(stmts.into_iter().map(Box::new).collect()),
                e.span(),
            )
        })
}

fn expr_parser<'tokens, 'src: 'tokens, I>()
-> impl Parser<'tokens, I, Spanned<Expr>, extra::Err<Rich<'tokens, Token<'src>, Span>>> + Clone
where
    I: ValueInput<'tokens, Token = Token<'src>, Span = Span>,
{
    recursive(|expr| {
        let val = select! {
            Token::Null => Expr::Value(Literal::Null),
            Token::Bool(b) => Expr::Value(Literal::Bool(b)),
            Token::Int(n) => Expr::Value(Literal::Number(n)),
            Token::Float(f) => Expr::Value(Literal::Number(Number::Float(f))),
            Token::Str(s) => Expr::Value(Literal::Str(Rc::from(s.as_ref()))),
        }
        .labelled("value");

        let ident = select! { Token::Ident(ident) => Symbol::from(ident) }.labelled("identifier");

        let items = expr
            .clone()
            .separated_by(just(Token::Comma).or(just(Token::Semicolon)))
            .allow_trailing()
            .collect::<Vec<_>>();

        let list_brackets = items
            .clone()
            .map(Expr::List)
            .delimited_by(just(Token::BracketOpen), just(Token::BracketClose));

        let list_parentheses = items
            .clone()
            .map(Expr::List)
            .delimited_by(just(Token::ParenOpen), just(Token::ParenClose));

        let param_list = ident
            .clone()
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .collect::<Vec<_>>();

        /*
        zero_arg_function = "{" [ block_inner ] "}"
        arg_list = expression [ "," arg_list ]
        general_function = [ "(" [ arg_list ] ")" "=>" ]? zero_arg_function

         */
        let zero_arg_function = block_with_expr(expr.clone())
            .delimited_by(just(Token::BraceOpen), just(Token::BraceClose));

        let general_function = param_list
            .clone()
            .delimited_by(just(Token::ParenOpen), just(Token::ParenClose))
            .then_ignore(just(Token::LambdaArrow))
            .or_not()
            .then(zero_arg_function)
            .map(|(params, body)| {
                Expr::Func(UserFunctionDef {
                    params: Rc::from(params.unwrap_or_default()),
                    body: Rc::new(body),
                })
            });

        let atom = val
            .or(ident.map(Expr::Symbol))
            .or(list_brackets)
            .or(general_function)
            .or(expr
                .clone()
                .delimited_by(just(Token::ParenOpen), just(Token::ParenClose))
                .map_with(|expr, _e| Expr::Parenthesized(Box::new(expr))))
            .or(list_parentheses)
            .map_with(|expr, e| (expr, e.span()))
            // Attempt to recover anything that looks like a parenthesised expression but contains errors
            .recover_with(via_parser(nested_delimiters(
                Token::ParenOpen,
                Token::ParenClose,
                [
                    (Token::BracketOpen, Token::BracketClose),
                    (Token::BraceOpen, Token::BraceClose),
                ],
                |span| (Expr::Error, span),
            )))
            // Attempt to recover anything that looks like a list but contains errors
            .recover_with(via_parser(nested_delimiters(
                Token::BracketOpen,
                Token::BracketClose,
                [
                    (Token::ParenOpen, Token::ParenClose),
                    (Token::BraceOpen, Token::BraceClose),
                ],
                |span| (Expr::Error, span),
            )))
            .boxed();

        // a.b
        let member_access = atom.clone().foldl_with(
            just(Token::Dot)
                .ignore_then(ident.clone())
                .map_with(|member, e| (member, e.span()))
                .repeated(),
            |object, member, e| (Expr::Member(Box::new(object), member), e.span()),
        );

        // f(x, y, z)
        let call_parenthesized = member_access.clone().foldl_with(
            items
                .delimited_by(just(Token::ParenOpen), just(Token::ParenClose))
                .map_with(|args, e| (args, e.span()))
                .repeated(),
            |f, args, e| (Expr::Call(Box::new(f), args), e.span()),
        );

        // f x; application binds more tightly than all infix operators.
        let call_unparenthesized = call_parenthesized.foldl_with(
            member_access
                .map_with(|arg, e| (vec![arg], e.span()))
                .repeated(),
            |f, args, e| (Expr::Call(Box::new(f), args), e.span()),
        );

        // These binding powers come directly from the table in thyme.md.
        let operator_expr = call_unparenthesized
            .pratt((
                infix(
                    right(14),
                    just(Token::Op("^")).to(BinaryOp::Pow),
                    |lhs, op, rhs, e| (Expr::Binary(Box::new(lhs), op, Box::new(rhs)), e.span()),
                ),
                prefix(
                    13,
                    select! {
                        Token::Op("!") => UnaryOp::Not,
                        Token::Op("-") => UnaryOp::Neg,
                        Token::Op("+") => UnaryOp::Pos,
                    },
                    |op, rhs, e| (Expr::Unary(op, Box::new(rhs)), e.span()),
                ),
                infix(
                    left(12),
                    select! {
                        Token::Op("*") => BinaryOp::Mul,
                        Token::Op("/") => BinaryOp::Div,
                        Token::Op("%") => BinaryOp::Mod,
                    },
                    |lhs, op, rhs, e| (Expr::Binary(Box::new(lhs), op, Box::new(rhs)), e.span()),
                ),
                infix(
                    left(11),
                    select! {
                        Token::Op("+") => BinaryOp::Add,
                        Token::Op("-") => BinaryOp::Sub,
                    },
                    |lhs, op, rhs, e| (Expr::Binary(Box::new(lhs), op, Box::new(rhs)), e.span()),
                ),
                infix(
                    left(10),
                    just(Token::Op("++")).to(BinaryOp::ListConcat),
                    |lhs, op, rhs, e| (Expr::Binary(Box::new(lhs), op, Box::new(rhs)), e.span()),
                ),
                infix(
                    left(9),
                    select! {
                        Token::Op("<") => BinaryOp::Less,
                        Token::Op("<=") => BinaryOp::LessEq,
                        Token::Op(">") => BinaryOp::Greater,
                        Token::Op(">=") => BinaryOp::GreaterEq,
                    },
                    |lhs, op, rhs, e| (Expr::Binary(Box::new(lhs), op, Box::new(rhs)), e.span()),
                ),
                infix(
                    left(8),
                    select! {
                        Token::Op("==") => BinaryOp::Eq,
                        Token::Op("!=") => BinaryOp::NotEq,
                    },
                    |lhs, op, rhs, e| (Expr::Binary(Box::new(lhs), op, Box::new(rhs)), e.span()),
                ),
                infix(
                    left(5),
                    just(Token::Op("..")).to(BinaryOp::Range),
                    |lhs, op, rhs, e| (Expr::Binary(Box::new(lhs), op, Box::new(rhs)), e.span()),
                ),
                infix(
                    left(4),
                    just(Token::Op("&&")).to(BinaryOp::And),
                    |lhs, op, rhs, e| (Expr::Binary(Box::new(lhs), op, Box::new(rhs)), e.span()),
                ),
                infix(
                    left(3),
                    just(Token::Op("||")).to(BinaryOp::Or),
                    |lhs, op, rhs, e| (Expr::Binary(Box::new(lhs), op, Box::new(rhs)), e.span()),
                ),
            ))
            .boxed();

        // ternary: a ? b : c
        // right associative: (true ? false : true ? 1 : 2) is parsed as (true ? false : (true ? 1 : 2))
        let ternary = operator_expr
            .then(
                just(Token::Op("?"))
                    .ignore_then(expr.clone())
                    .then_ignore(just(Token::Op(":")))
                    .then(expr.clone())
                    .or_not(),
            )
            .map_with(|(cond, branches), e| match branches {
                Some((true_expr, false_expr)) => (
                    Expr::Ternary(Box::new(cond), Box::new(true_expr), Box::new(false_expr)),
                    e.span(),
                ),
                None => cond,
            })
            .boxed();

        // class object assign: obj -> { foo = 5; bar = 6; }
        // right associative but Algodoo rejects it if there's more than one token on the left size (e.g. 1 + a->{2} breaks the parser)
        let class_assign = ternary
            .clone()
            .then(just(Token::Op("->")).ignore_then(expr.clone()).or_not())
            .map_with(|(obj, class_expr), e| match class_expr {
                Some(class_expr) => (
                    Expr::Binary(Box::new(obj), BinaryOp::ClassAssign, Box::new(class_expr)),
                    e.span(),
                ),
                None => obj,
            })
            .boxed();

        // assignment: a = b or a := b (right associative)
        let op = just(Token::Op("="))
            .to(BinaryOp::Assign)
            .or(just(Token::Op(":=")).to(BinaryOp::Declare));
        let assignment = class_assign
            .then(op.then(expr.clone()).or_not())
            .map_with(|(lhs, assignment), e| match assignment {
                Some((op, rhs)) => (Expr::Binary(Box::new(lhs), op, Box::new(rhs)), e.span()),
                None => lhs,
            })
            .boxed();

        assignment.labelled("expression").as_context()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn right_associative_operators_keep_their_ast_shape() {
        let assignment = parse_source("a = b := c");
        let Expr::Binary(a, BinaryOp::Assign, declaration) = single_expr(&assignment) else {
            panic!("expected assignment, got {:#?}", single_expr(&assignment));
        };
        assert_symbol(a, "a");
        let Expr::Binary(b, BinaryOp::Declare, c) = &declaration.0 else {
            panic!("expected declaration, got {:#?}", declaration.0);
        };
        assert_symbol(b, "b");
        assert_symbol(c, "c");

        let exponent = parse_source("a ^ b ^ c");
        let Expr::Binary(a, BinaryOp::Pow, rhs) = single_expr(&exponent) else {
            panic!("expected exponent, got {:#?}", single_expr(&exponent));
        };
        assert_symbol(a, "a");
        let Expr::Binary(b, BinaryOp::Pow, c) = &rhs.0 else {
            panic!("expected exponent, got {:#?}", rhs.0);
        };
        assert_symbol(b, "b");
        assert_symbol(c, "c");

        let ternary = parse_source("a ? b : c ? d : e");
        let Expr::Ternary(a, b, rhs) = single_expr(&ternary) else {
            panic!("expected ternary, got {:#?}", single_expr(&ternary));
        };
        assert_symbol(a, "a");
        assert_symbol(b, "b");
        let Expr::Ternary(c, d, e) = &rhs.0 else {
            panic!("expected ternary, got {:#?}", rhs.0);
        };
        assert_symbol(c, "c");
        assert_symbol(d, "d");
        assert_symbol(e, "e");
    }

    #[test]
    fn pratt_parser_uses_the_thyme_precedence_table() {
        let ast = parse_source("a || b && c .. d == e < f ++ g + h * -i ^ j");

        let (a, and) = assert_binary(single_expr(&ast), BinaryOp::Or);
        assert_symbol(a, "a");
        let (b, range) = assert_binary(&and.0, BinaryOp::And);
        assert_symbol(b, "b");
        let (c, equality) = assert_binary(&range.0, BinaryOp::Range);
        assert_symbol(c, "c");
        let (d, relation) = assert_binary(&equality.0, BinaryOp::Eq);
        assert_symbol(d, "d");
        let (e, concat) = assert_binary(&relation.0, BinaryOp::Less);
        assert_symbol(e, "e");
        let (f, sum) = assert_binary(&concat.0, BinaryOp::ListConcat);
        assert_symbol(f, "f");
        let (g, product) = assert_binary(&sum.0, BinaryOp::Add);
        assert_symbol(g, "g");
        let (h, negation) = assert_binary(&product.0, BinaryOp::Mul);
        assert_symbol(h, "h");
        let power = assert_unary(&negation.0, UnaryOp::Neg);
        let (i, j) = assert_binary(&power.0, BinaryOp::Pow);
        assert_symbol(i, "i");
        assert_symbol(j, "j");

        let subtraction = parse_source("a - b - c");
        let (lhs, c) = assert_binary(single_expr(&subtraction), BinaryOp::Sub);
        let (a, b) = assert_binary(&lhs.0, BinaryOp::Sub);
        assert_symbol(a, "a");
        assert_symbol(b, "b");
        assert_symbol(c, "c");
    }

    #[test]
    fn deeply_nested_functions_do_not_cause_exponential_backtracking() {
        let depth = 32;
        let source = format!("{}0{}", "{".repeat(depth), "}".repeat(depth));
        parse_source(&source);
    }
    fn parse_source(source: &str) -> Spanned<Expr> {
        let lexer = Token::lexer(source)
            .spanned()
            .map(|(token, span)| (token.unwrap_or(Token::Error), span.into()));
        let token_stream = Stream::from_iter(lexer)
            .map((0..source.len()).into(), |(token, span): (_, _)| {
                (token, span)
            });

        block_parser()
            .then_ignore(end())
            .parse(token_stream)
            .into_result()
            .unwrap_or_else(|errors| panic!("parse errors: {errors:#?}"))
    }

    fn single_expr(ast: &Spanned<Expr>) -> &Expr {
        match &ast.0 {
            Expr::Seq(exprs) if exprs.len() == 1 => &exprs[0].0,
            other => panic!("expected one expression, got {other:#?}"),
        }
    }

    fn assert_symbol(expr: &Spanned<Expr>, expected: &str) {
        match &expr.0 {
            Expr::Symbol(actual) => assert_eq!(actual.as_str(), expected),
            other => panic!("expected symbol {expected:?}, got {other:#?}"),
        }
    }

    fn assert_binary<'ast>(
        expr: &'ast Expr,
        expected: BinaryOp,
    ) -> (&'ast Spanned<Expr>, &'ast Spanned<Expr>) {
        match expr {
            Expr::Binary(lhs, actual, rhs) => {
                assert_eq!(*actual, expected);
                (lhs, rhs)
            }
            other => panic!("expected {expected:?}, got {other:#?}"),
        }
    }

    fn assert_unary(expr: &Expr, expected: UnaryOp) -> &Spanned<Expr> {
        match expr {
            Expr::Unary(actual, rhs) => {
                assert_eq!(*actual, expected);
                rhs
            }
            other => panic!("expected {expected:?}, got {other:#?}"),
        }
    }

    #[test]
    fn lexes_computer_phn_without_errors() {
        let source = read_auto_encoding(include_bytes!("../snippets/computer.phn"));
        let mut lexer = Token::lexer(&source);

        while let Some(token) = lexer.next() {
            assert!(
                token.is_ok(),
                "lexer error at {:?}: {:?}",
                lexer.span(),
                lexer.slice()
            );
        }
    }

    #[test]
    fn parses_computer_phn_without_errors() {
        let source = read_auto_encoding(include_bytes!("../snippets/computer.phn"));
        parse_source(&source);
    }

    fn parsed_number(source: &str) -> Number {
        let ast = parse_source(source);
        match single_expr(&ast) {
            Expr::Value(Literal::Number(number)) => *number,
            other => panic!("expected a number literal, got {other:#?}"),
        }
    }

    #[test]
    fn parsed_numbers_preserve_algodoo_integer_and_float_types() {
        assert_eq!(parsed_number("123"), Number::Int(123));
        assert!(matches!(parsed_number("2147483648"), Number::Float(value) if value.is_finite()));
        assert_eq!(parsed_number("0xff"), Number::Int(255));
        assert_eq!(parsed_number("0b101101"), Number::Int(45));
        assert_eq!(parsed_number("1.25"), Number::Float(1.25));
        assert_eq!(parsed_number("+inf"), Number::Float(f32::INFINITY));
        assert_eq!(parsed_number("-inf"), Number::Float(f32::NEG_INFINITY));
        assert!(matches!(parsed_number("NaN"), Number::Float(value) if value.is_nan()));
    }

    #[test]
    fn parsed_names_strings_and_parameters_are_owned() {
        let symbol = parse_source("some_name");
        match single_expr(&symbol) {
            Expr::Symbol(value) => assert_eq!(value.as_str(), "some_name"),
            other => panic!("expected symbol, got {other:#?}"),
        }

        let string = parse_source(r#""hello\nworld""#);
        match single_expr(&string) {
            Expr::Value(Literal::Str(value)) => assert_eq!(&**value, "hello\nworld"),
            other => panic!("expected string literal, got {other:#?}"),
        }

        let function = parse_source("(first, second) => { first }");
        match single_expr(&function) {
            Expr::Func(definition) => {
                assert_eq!(definition.params[0].as_str(), "first");
                assert_eq!(definition.params[1].as_str(), "second");
                let Expr::Seq(body) = &definition.body.0 else {
                    panic!(
                        "expected function body sequence, got {:#?}",
                        definition.body.0
                    );
                };
                assert_symbol(&body[0], "first");
            }
            other => panic!("expected function, got {other:#?}"),
        }
    }
}

pub fn parse_thyme<'src, 'tok>(source: &'src str) -> ParseResult<Spanned<Expr>, chumsky::error::Rich<'tok, Token<'src>>> {
    let lexer = Token::lexer(source)
        .spanned()
        .map(|(token, span)| (token.unwrap_or(Token::Error), span.into()));
    let token_stream = Stream::from_iter(lexer)
        .map((0..source.len()).into(), |(token, span): (_, _)| {
            (token, span)
        });
    block_parser().parse(token_stream)
}