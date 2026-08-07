use logos::Logos;
use std::borrow::Cow;
use std::fmt::Display;
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
    Void,
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
    Error,

    #[token("false", |_| false)]
    #[token("true", |_| true)]
    Bool(bool),

    #[token("null")]
    Null,

    #[token("=>")]
    LambdaArrow,

    /*#[token("->")]
    Arrow,

    #[token(":=")]
    Declare,

    #[token("=")]
    Assign,*/

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

    #[regex(r"(>=|<=|==|!=|&&|\|\|\+\+|\.\.|:=|->)", priority=3)]
    #[regex(r"[+\-*/%&|^!<>?=:]")]
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

    fn parse_source(source: &str) -> Spanned<Expr<'_>> {
        let lexer = Token::lexer(source).spanned().map(|(token, span)| {
            (token.unwrap_or(Token::Error), span.into())
        });
        let token_stream = Stream::from_iter(lexer).map(
            (0..source.len()).into(),
            |(token, span): (_, _)| (token, span),
        );

        block_parser()
            .then_ignore(end())
            .parse(token_stream)
            .into_result()
            .unwrap_or_else(|errors| panic!("parse errors: {errors:#?}"))
    }

    fn single_expr<'ast, 'src>(ast: &'ast Spanned<Expr<'src>>) -> &'ast Expr<'src> {
        match &ast.0 {
            Expr::Seq(exprs) if exprs.len() == 1 => &exprs[0].0,
            other => panic!("expected one expression, got {other:#?}"),
        }
    }

    fn assert_symbol(expr: &Spanned<Expr<'_>>, expected: &str) {
        match &expr.0 {
            Expr::Symbol(actual) => assert_eq!(*actual, expected),
            other => panic!("expected symbol {expected:?}, got {other:#?}"),
        }
    }

    #[test]
    fn lexes_computer_phn_without_errors() {
        let source = read_auto_encoding(include_bytes!("../examples/computer.phn"));
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
        let source = read_auto_encoding(include_bytes!("../examples/computer.phn"));
        parse_source(&source);
    }

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
    fn deeply_nested_functions_do_not_cause_exponential_backtracking() {
        let depth = 32;
        let source = format!("{}0{}", "{".repeat(depth), "}".repeat(depth));
        parse_source(&source);
    }
}

pub type Span = SimpleSpan;
pub type Spanned<T> = (T, Span);

// in increasing order of Algodoo precedence
#[derive(Copy, Clone, Debug)]
enum BinaryOp {
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
    /// a.b
    Member(Box<Spanned<Self>>, Spanned<&'src str>),
    /// f(a,b,c) or simply f a
    Call(Box<Spanned<Self>>, Spanned<Vec<Spanned<Self>>>),
    Binary(Box<Spanned<Self>>, BinaryOp, Box<Spanned<Self>>),
    Unary(UnaryOp, Box<Spanned<Self>>),
    // cond ? true_expr : false_expr
    Ternary(Box<Spanned<Self>>, Box<Spanned<Self>>, Box<Spanned<Self>>),
    /// (a,b,c)=>{...} or simply {...}
    Func(Vec<&'src str>, Span, Box<Spanned<Self>>),
    /// a; b (optional trailing ;)
    Seq(Vec<Box<Spanned<Self>>>),
}

/*
block_inner
    = expression
      { ";" expression }
      [ ";" ]
    ;
     */
fn block_parser<'tokens, 'src: 'tokens, I>(
) -> impl Parser<'tokens, I, Spanned<Expr<'src>>, extra::Err<Rich<'tokens, Token<'src>, Span>>> + Clone
where
    I: ValueInput<'tokens, Token = Token<'src>, Span = Span>,
{
    block_with_expr(expr_parser())
}

fn block_with_expr<'tokens, 'src: 'tokens, I, P>(
    expr: P,
) -> impl Parser<'tokens, I, Spanned<Expr<'src>>, extra::Err<Rich<'tokens, Token<'src>, Span>>> + Clone
where
    I: ValueInput<'tokens, Token = Token<'src>, Span = Span>,
    P: Parser<'tokens, I, Spanned<Expr<'src>>, extra::Err<Rich<'tokens, Token<'src>, Span>>> + Clone,
{
    expr
        .clone()
        .separated_by(just(Token::Semicolon))
        .allow_trailing()
        .collect::<Vec<_>>()
        .map_with(|stmts, e| (Expr::Seq(stmts.into_iter().map(Box::new).collect()), e.span()))
}

fn expr_parser<'tokens, 'src: 'tokens, I>(
) -> impl Parser<'tokens, I, Spanned<Expr<'src>>, extra::Err<Rich<'tokens, Token<'src>, Span>>> + Clone
where
    I: ValueInput<'tokens, Token = Token<'src>, Span = Span>,
{
    recursive(|expr| {
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
            .map_with(|(params, body), e| Expr::Func(params.unwrap_or_default(), e.span(), Box::new(body)));

        let atom = val
            .or(ident.map(Expr::Symbol))
            .or(list_brackets)
            .or(general_function)
            .or(expr
                .clone()
                .delimited_by(just(Token::ParenOpen), just(Token::ParenClose))
                .map_with(|expr, e| expr.0))
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

        // ^ (right associative)
        let op = just(Token::Op("^")).to(BinaryOp::Pow);
        let exponent = recursive(|exponent| {
            call_unparenthesized
                .clone()
                .then(op.then(exponent).or_not())
                .map_with(|(lhs, exponent), e| match exponent {
                    Some((op, rhs)) => (
                        Expr::Binary(Box::new(lhs), op, Box::new(rhs)),
                        e.span(),
                    ),
                    None => lhs,
                })
        })
            .boxed();

        // unary: ! - + (prefix)
        let unary_op = just(Token::Op("!")).to(UnaryOp::Not)
            .or(just(Token::Op("-")).to(UnaryOp::Neg))
            .or(just(Token::Op("+")).to(UnaryOp::Pos));
        let unary = unary_op.then(exponent.clone())
            .map_with(|(op, expr), e| (Expr::Unary(op, Box::new(expr)), e.span()))
            .or(exponent.clone())
            .boxed();

        // * / %
        let op = just(Token::Op("*")).to(BinaryOp::Mul)
            .or(just(Token::Op("/")).to(BinaryOp::Div))
            .or(just(Token::Op("%")).to(BinaryOp::Mod));
        let product = unary
            .clone()
            .foldl_with(op.then(unary).repeated(), |a, (op, b), e| {
                (Expr::Binary(Box::new(a), op, Box::new(b)), e.span())
            })
            .boxed();

        // + -
        let op = just(Token::Op("+")).to(BinaryOp::Add)
            .or(just(Token::Op("-")).to(BinaryOp::Sub));
        let sum = product
            .clone()
            .foldl_with(op.then(product).repeated(), |a, (op, b), e| {
                (Expr::Binary(Box::new(a), op, Box::new(b)), e.span())
            })
            .boxed();

        // ++
        let op = just(Token::Op("++")).to(BinaryOp::ListConcat);
        let list_concat = sum
            .clone()
            .foldl_with(op.then(sum).repeated(), |a, (op, b), e| {
                (Expr::Binary(Box::new(a), op, Box::new(b)), e.span())
            }).boxed();

        // < <= > >=
        let op = just(Token::Op("<")).to(BinaryOp::Less)
            .or(just(Token::Op("<=")).to(BinaryOp::LessEq))
            .or(just(Token::Op(">")).to(BinaryOp::Greater))
            .or(just(Token::Op(">=")).to(BinaryOp::GreaterEq));
        let rel_comp = list_concat
            .clone()
            .foldl_with(op.then(list_concat).repeated(), |a, (op, b), e| {
                (Expr::Binary(Box::new(a), op, Box::new(b)), e.span())
            })
            .boxed();

        // == !=
        let op = just(Token::Op("==")).to(BinaryOp::Eq)
            .or(just(Token::Op("!=")).to(BinaryOp::NotEq));
        let compare = rel_comp
            .clone()
            .foldl_with(op.then(rel_comp).repeated(), |a, (op, b), e| {
                (Expr::Binary(Box::new(a), op, Box::new(b)), e.span())
            })
            .boxed();

        // ..
        let op = just(Token::Op("..")).to(BinaryOp::Range);
        let range = compare
            .clone()
            .foldl_with(op.then(compare).repeated(), |a, (op, b), e| {
                (Expr::Binary(Box::new(a), op, Box::new(b)), e.span())
            })
            .boxed();

        // &&
        let op = just(Token::Op("&&")).to(BinaryOp::And);
        let and = range
            .clone()
            .foldl_with(op.then(range).repeated(), |a, (op, b), e| {
                (Expr::Binary(Box::new(a), op, Box::new(b)), e.span())
            })
            .boxed();

        // ||
        let op = just(Token::Op("||")).to(BinaryOp::Or);
        let or = and
            .clone()
            .foldl_with(op.then(and).repeated(), |a, (op, b), e| {
                (Expr::Binary(Box::new(a), op, Box::new(b)), e.span())
            })
            .boxed();

        // ternary: a ? b : c 
        // right associative: (true ? false : true ? 1 : 2) is parsed as (true ? false : (true ? 1 : 2))
        let ternary = or
            .then(
                just(Token::Op("?"))
                    .ignore_then(expr.clone())
                    .then_ignore(just(Token::Op(":")))
                    .then(expr.clone())
                    .or_not(),
            )
            .map_with(|(cond, branches), e| match branches {
                Some((true_expr, false_expr)) => (
                    Expr::Ternary(
                        Box::new(cond),
                        Box::new(true_expr),
                        Box::new(false_expr),
                    ),
                    e.span(),
                ),
                None => cond,
            })
            .boxed();

        // class object assign: obj -> { foo = 5; bar = 6; }
        // right associative but Algodoo rejects it if there's more than one token on the left size (e.g. 1 + a->{2} breaks the parser)
        let class_assign = ternary
            .clone()
            .then(
                just(Token::Op("->"))
                    .ignore_then(expr.clone())
                    .or_not(),
            )
            .map_with(|(obj, class_expr), e| {
                match class_expr {
                    Some(class_expr) => (
                        Expr::Binary(
                            Box::new(obj),
                            BinaryOp::ClassAssign,
                            Box::new(class_expr),
                        ),
                        e.span(),
                    ),
                    None => obj,
                }
            })
            .boxed();

        // assignment: a = b or a := b (right associative)
        let op = just(Token::Op("=")).to(BinaryOp::Assign)
            .or(just(Token::Op(":=")).to(BinaryOp::Declare));
        let assignment = class_assign
            .then(op.then(expr.clone()).or_not())
            .map_with(|(lhs, assignment), e| match assignment {
                Some((op, rhs)) => (
                    Expr::Binary(Box::new(lhs), op, Box::new(rhs)),
                    e.span(),
                ),
                None => lhs,
            })
            .boxed();

        assignment.labelled("expression").as_context()
    })
}
