use logos::Logos;
use std::borrow::Cow;

#[derive(Copy, Clone, Debug, PartialEq)]
enum Number {
    Int(i32),
    Float(f32),
}

enum Value<'a> {
    Null,
    Bool(bool),
    Number(Number),
    Str(Cow<'a, str>),
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

    #[token("=")]
    #[token(":=")]
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

    #[regex(r"(>=|<=|==|!=|&&|\|\|)", priority=3)]
    #[regex(r"[+\-*/%&|^!<>?:]")]
    Operator(&'a str),

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
    #[token("inf", |_| f32::INFINITY)]
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
    Str(Cow<'a, str>),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexes_computer_phn_without_errors() {
        let source = include_bytes!("examples/computer.phn");
        // try decoding utf8 then 1252
        let source = match std::str::from_utf8(source) {
            Ok(s) => s.to_owned(),
            Err(_) => {
                let (cow, _, _) = encoding_rs::WINDOWS_1252.decode(source);
                cow.into_owned()
            }
        };
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
}

