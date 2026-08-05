use logos::Logos;
use std::borrow::Cow;
use chumsky::prelude::*;
use chumsky::input::*;

#[derive(Copy, Clone, Debug, PartialEq)]
enum Number {
    Int(i32),
    Float(f32),
}

#[derive(Debug)]
enum Value<'a> {
    Null,
    Bool(bool),
    Number(Number),
    Str(Cow<'a, str>),
    List(Vec<Self>),
}

#[derive(Logos, Clone, Debug, PartialEq)]
#[logos(skip r"[ \t\r\n\f]+")]
#[logos(skip (r"//[^\r\n]*", allow_greedy = true))]
#[logos(skip r"/\*([^*]|\*[^/])*\*/")]
enum Token<'a> {
    #[token("false", |_| false)]
    #[token("true", |_| true)]
    Bool(bool),

    #[token("null")]
    Null,

    #[token("=>")]
    LambdaArrow,

    #[token("->")]
    Arrow,

    #[token(":=")]
    Declare,

    #[token("=")]
    Assign,

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

    #[regex(r"(>=|<=|==|!=|&&|\|\|\+\+|\.\.)", priority=3)]
    #[regex(r"[+\-*/%&|^!<>?:]")]
    Op(&'a str),

    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*")]
    Ident(&'a str),

    #[regex(r"[0-9]+", |lex| match lex.slice().parse() {
        Ok(num) => Some(Number::Int(num)),
        Err(_) => Some(Number::Float(lex.slice().parse().unwrap())),
    }, priority = 3)]
    // * because Algodoo allows simply `0x` or `0b`.
    #[regex(r"0x[0-9a-fA-F]*", |lex| Number::Int(i32::from_str_radix(&lex.slice()[2..lex.slice().len().saturating_sub(8).max(2)], 16).unwrap_or(0)), priority = 4)]
    #[regex(r"0b[01]*", |lex| Number::Int(i32::from_str_radix(&lex.slice()[2..lex.slice().len().saturating_sub(8).max(2)], 2).unwrap_or(0)), priority = 4)]
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

fn read_auto_encoding(source: &[u8]) -> Cow<str> {
    // try decoding utf8 then 1252
    match std::str::from_utf8(source) {
        Ok(s) => Cow::Borrowed(s),
        Err(_) => {
            let (cow, _, _) = encoding_rs::WINDOWS_1252.decode(source);
            cow
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexes_computer_phn_without_errors() {
        let source = read_auto_encoding(include_bytes!("examples/computer.phn"));
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
    fn parses_test_file_without_errors() {
        use ariadne::{sources, Color, Label, Report, ReportKind};

        let test_file = read_auto_encoding(include_bytes!("examples/test.cfg"));
        let mut lexer = Token::lexer(&test_file);
        let (ast, errs) = expr_parser().parse_recovery(lexer.spanned());

        errs.into_iter()
        .map(|e| e.map_token(|c| c.to_string()))
        .chain(
            parse_errs
                .into_iter()
                .map(|e| e.map_token(|tok| tok.to_string())),
        )
        .for_each(|e| {
            Report::build(ReportKind::Error, (filename.clone(), e.span().into_range()))
                .with_config(ariadne::Config::new().with_index_type(ariadne::IndexType::Byte))
                .with_message(e.to_string())
                .with_label(
                    Label::new((filename.clone(), e.span().into_range()))
                        .with_message(e.reason().to_string())
                        .with_color(Color::Red),
                )
                .with_labels(e.contexts().map(|(label, span)| {
                    Label::new((filename.clone(), span.into_range()))
                        .with_message(format!("while parsing this {label}"))
                        .with_color(Color::Yellow)
                }))
                .finish()
                .print(sources([(filename.clone(), src.clone())]))
                .unwrap()
        });
    }
}

pub type Span = SimpleSpan;
pub type Spanned<T> = (T, Span);

#[derive(Copy, Clone, Debug)]
enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,

    ListConcat,

    And,
    Or,

    Eq,
    NotEq,

    Less,
    LessEq,
    Greater,
    GreaterEq,

    Range,

    Assign,
    Declare,
    ClassAssign,
}

#[derive(Clone, Debug)]
enum UnaryOp {
    Not,
    Neg,
    Pos
}

#[derive(Debug)]
enum Expr<'src> {
    Error,
    Value(Value<'src>),
    List(Vec<Spanned<Self>>),
    Symbol(&'src str),
    /// f(a,b,c) or simply f a
    Call(Box<Spanned<Self>>, Spanned<Vec<Spanned<Self>>>),
    Binary(Box<Spanned<Self>>, BinaryOp, Box<Spanned<Self>>),
    Unary(UnaryOp, Box<Spanned<Self>>),
    /// (a,b,c)=>{...} or simply {...}
    Func(Vec<&'src str>, Span, Box<Spanned<Self>>),
    /// a; b (optional trailing ;)
    Seq(Box<Spanned<Self>>, Box<Spanned<Self>>),
}

fn expr_parser<'tokens, 'src: 'tokens, I>(
) -> impl Parser<'tokens, I, Spanned<Expr<'src>>, extra::Err<Rich<'tokens, Token<'src>, Span>>> + Clone
where
    I: ValueInput<'tokens, Token = Token<'src>, Span = Span>,
{
    recursive(|expr| {
        let inline_expr = recursive(|inline_expr| {
            let val = select! {
                Token::Null => Expr::Value(Value::Null),
                Token::Bool(b) => Expr::Value(Value::Bool(b)),
                Token::Int(n) => Expr::Value(Value::Number(n)),
                Token::Float(f) => Expr::Value(Value::Number(Number::Float(f))),
                Token::Str(s) => Expr::Value(Value::Str(s)),
            }
            .labelled("value");

            let ident = select! { Token::Ident(ident) => ident }.labelled("identifier");

            let items = expr
                .clone()
                .separated_by(just(Token::Comma))
                .allow_trailing()
                .collect::<Vec<_>>();

            let list = items
                .clone()
                .map(Expr::List)
                .delimited_by(just(Token::BracketOpen), just(Token::BracketClose));

            let atom = val
                .or(ident.map(Expr::Symbol))
                .or(list)
                .map_with(|expr, e| (expr, e.span()))
                // Atoms can also just be normal expressions, but surrounded with parentheses
                .or(expr
                    .clone()
                    .delimited_by(just(Token::ParenOpen), just(Token::ParenClose)))
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

            let call = atom.foldl_with(
                items
                    .delimited_by(just(Token::ParenOpen), just(Token::ParenClose))
                    .map_with(|args, e| (args, e.span()))
                    .repeated(),
                |f, args, e| (Expr::Call(Box::new(f), args), e.span()),
            );

            let op = just(Token::Op("*"))
                .to(BinaryOp::Mul)
                .or(just(Token::Op("/")).to(BinaryOp::Div));
            let product = call
                .clone()
                .foldl_with(op.then(call).repeated(), |a, (op, b), e| {
                    (Expr::Binary(Box::new(a), op, Box::new(b)), e.span())
                });

            let op = just(Token::Op("+"))
                .to(BinaryOp::Add)
                .or(just(Token::Op("-")).to(BinaryOp::Sub));
            let sum = product
                .clone()
                .foldl_with(op.then(product).repeated(), |a, (op, b), e| {
                    (Expr::Binary(Box::new(a), op, Box::new(b)), e.span())
                });

            let op = just(Token::Op("=="))
                .to(BinaryOp::Eq)
                .or(just(Token::Op("!=")).to(BinaryOp::NotEq));
            let compare = sum
                .clone()
                .foldl_with(op.then(sum).repeated(), |a, (op, b), e| {
                    (Expr::Binary(Box::new(a), op, Box::new(b)), e.span())
                });

            compare.labelled("expression").as_context()
        });

        inline_expr
    })
}