//! トークン列を読める形の文字列にする簡単な整形器（`verusfmt` の代わり。仕様 §7.5 は SHOULD）。
//!
//! 意味を変えないことだけを保証する：結合している記号（`==>` など）の間に空白を入れず、
//! 識別子とリテラルの間には必ず空白を置く。改行は `;` と、文を含む `{ }` の中に入れる。

use proc_macro2::{Delimiter, Spacing, TokenStream, TokenTree};

const INDENT: &str = "    ";

/// 句のキーワード（関数の署名の後で改行する）。
const CLAUSES: &[&str] = &[
    "requires",
    "ensures",
    "returns",
    "recommends",
    "decreases",
    "no_unwind",
    "invariant",
];

pub fn print(ts: &TokenStream, indent: usize) -> String {
    let mut p = Printer {
        out: String::new(),
        indent,
        line_start: true,
    };
    p.stream(ts, true);
    let mut s = p.out.trim_end().to_string();
    s.push('\n');
    s
}

struct Printer {
    out: String,
    indent: usize,
    line_start: bool,
}

/// 前のトークンの種類（空白の入れ方を決める）。
#[derive(Clone, Copy, PartialEq, Eq)]
enum Prev {
    None,
    Word,
    /// 結合する記号（`Spacing::Joint`）。
    JointPunct(char),
    Punct(char),
    /// 二文字以上の演算子の終わり（`==`、`&&`、`==>`）。後ろに空白を置く。
    Op,
    /// `::`。
    PathSep,
    /// キーワード（`let`、`if` など）。後ろの括弧の前に空白を置く。
    Keyword,
    Close,
}

/// 後ろの `(` / `[` を呼び出しと読まない語。
const KEYWORDS: &[&str] = &[
    "let",
    "if",
    "while",
    "match",
    "return",
    "in",
    "for",
    "move",
    "mut",
    "else",
    "where",
    "as",
    "impl",
    "dyn",
    "requires",
    "ensures",
    "returns",
    "decreases",
    "invariant",
    "recommends",
    "break",
    "tracked",
    "ghost",
    "loop",
    "unsafe",
];

impl Printer {
    fn newline(&mut self) {
        while self.out.ends_with(' ') {
            self.out.pop();
        }
        if !self.out.is_empty() && !self.out.ends_with('\n') {
            self.out.push('\n');
        }
        self.line_start = true;
    }

    fn write(&mut self, s: &str) {
        if self.line_start {
            for _ in 0..self.indent {
                self.out.push_str(INDENT);
            }
            self.line_start = false;
        }
        self.out.push_str(s);
    }

    fn space(&mut self) {
        if !self.line_start && !self.out.ends_with(' ') {
            self.out.push(' ');
        }
    }

    /// `block`：文を並べる位置（ファイルの直下、複数行の `{ }` の中）か。
    fn stream(&mut self, ts: &TokenStream, block: bool) {
        let tts: Vec<TokenTree> = ts.clone().into_iter().collect();
        let mut prev = Prev::None;
        for (i, tt) in tts.iter().enumerate() {
            let next = tts.get(i + 1);
            match tt {
                TokenTree::Ident(id) => {
                    let s = id.to_string();
                    let after_fn = i > 0 && matches!(&tts[i - 1], TokenTree::Ident(x) if x == "fn");
                    let clause = block
                        && CLAUSES.contains(&s.as_str())
                        && i > 0
                        && !after_fn
                        && !matches!(prev, Prev::Punct('.') | Prev::PathSep);
                    if clause {
                        self.newline();
                    } else if needs_space(prev, false) {
                        self.space();
                    }
                    self.write(&s);
                    prev = if KEYWORDS.contains(&s.as_str())
                        && !after_fn
                        && !matches!(prev, Prev::Punct('.') | Prev::PathSep)
                    {
                        Prev::Keyword
                    } else {
                        Prev::Word
                    };
                }
                TokenTree::Literal(l) => {
                    if needs_space(prev, false) {
                        self.space();
                    }
                    self.write(&l.to_string());
                    prev = Prev::Word;
                }
                TokenTree::Punct(p) => {
                    let c = p.as_char();
                    let tight_before = matches!(c, '.' | ',' | ';' | '?')
                        || (c == ':'
                            && (matches!(prev, Prev::JointPunct(':'))
                                || p.spacing() == Spacing::Joint))
                        || (c == '@' && matches!(prev, Prev::Word | Prev::Close))
                        || (c == '!' && matches!(prev, Prev::Word) && is_group(next))
                        || (c == ':' && matches!(prev, Prev::Word | Prev::Close));
                    // 離れていた二つの記号はつなげない（`a & &b` を `a &&b` にしない）。
                    let apart = matches!(prev, Prev::Punct(_) | Prev::Op);
                    if !tight_before && (needs_space(prev, true) || apart) {
                        self.space();
                    }
                    self.write(&c.to_string());
                    prev = match (p.spacing(), prev) {
                        (Spacing::Joint, _) => Prev::JointPunct(c),
                        (Spacing::Alone, Prev::JointPunct(':')) if c == ':' => Prev::PathSep,
                        (Spacing::Alone, Prev::JointPunct('-' | '=')) if c == '>' => Prev::Op,
                        (Spacing::Alone, Prev::JointPunct(a)) if is_op(a, c) => Prev::Op,
                        (Spacing::Alone, _) => Prev::Punct(c),
                    };
                    if block && c == ';' {
                        self.newline();
                        prev = Prev::None;
                    }
                }
                TokenTree::Group(g) => {
                    let (open, close) = match g.delimiter() {
                        Delimiter::Parenthesis => ("(", ")"),
                        Delimiter::Bracket => ("[", "]"),
                        Delimiter::Brace => ("{", "}"),
                        Delimiter::None => ("", ""),
                    };
                    let call = matches!(prev, Prev::Word | Prev::Close | Prev::PathSep)
                        && g.delimiter() != Delimiter::Brace
                        || matches!(prev, Prev::Punct('!') | Prev::JointPunct('!'));
                    let after_dot_or_colon = matches!(
                        prev,
                        Prev::Punct('.') | Prev::JointPunct(':') | Prev::Punct(':')
                    );
                    if !call && !after_dot_or_colon && needs_space(prev, false) {
                        self.space();
                    }
                    if g.delimiter() == Delimiter::Brace && multiline(&g.stream()) {
                        self.write(open);
                        self.newline();
                        self.indent += 1;
                        self.stream(&g.stream(), true);
                        self.newline();
                        self.indent -= 1;
                        self.write(close);
                        // `}` の後：文や項目の終わりなら改行する。
                        let continues = matches!(next, Some(TokenTree::Punct(p)) if matches!(p.as_char(), ',' | ';' | '.' | ')' | '?'))
                            || matches!(next, Some(TokenTree::Ident(i)) if i == "else");
                        if block && !continues {
                            self.newline();
                            prev = Prev::None;
                            continue;
                        }
                    } else {
                        self.write(open);
                        let inner = g.stream();
                        if g.delimiter() == Delimiter::Brace && !inner.is_empty() {
                            self.write(" ");
                        }
                        self.stream(&inner, false);
                        if g.delimiter() == Delimiter::Brace && !inner.is_empty() {
                            self.write(" ");
                        }
                        self.write(close);
                    }
                    prev = Prev::Close;
                }
            }
        }
    }
}

/// 二文字の演算子の終わりか（`a` は前の結合した記号、`c` は今の記号）。
fn is_op(a: char, c: char) -> bool {
    matches!(
        (a, c),
        ('=', '=' | '>')
            | (
                '!' | '<' | '>' | '+' | '-' | '*' | '/' | '%' | '^' | '&' | '|',
                '='
            )
            | ('&', '&')
            | ('|', '|')
            | ('-', '>')
            | ('<', '<')
            | ('>', '>')
            | ('.', '.' | '=')
    )
}

fn is_group(t: Option<&TokenTree>) -> bool {
    matches!(t, Some(TokenTree::Group(_)))
}

/// 前のトークンの後に空白を入れるか。
fn needs_space(prev: Prev, _punct: bool) -> bool {
    match prev {
        Prev::None => false,
        // 結合した記号の続きには空白を入れない（`==>`、`::`、`->`）。
        Prev::JointPunct(_) => false,
        Prev::Punct('.') => false,
        // 前置の `&`・`*`・`!`、属性の `#`、`@` の後。
        Prev::Punct('&' | '*' | '!' | '#' | '@') => false,
        Prev::Punct(_) | Prev::Op | Prev::Keyword => true,
        Prev::PathSep => false,
        Prev::Word | Prev::Close => true,
    }
}

/// `{ }` の中を複数行にするか（文がある、または長い）。
fn multiline(ts: &TokenStream) -> bool {
    let tts: Vec<TokenTree> = ts.clone().into_iter().collect();
    if tts
        .iter()
        .any(|t| matches!(t, TokenTree::Punct(p) if p.as_char() == ';'))
    {
        return true;
    }
    if tts
        .iter()
        .any(|t| matches!(t, TokenTree::Ident(i) if CLAUSES.contains(&i.to_string().as_str())))
    {
        return true;
    }
    let flat = print_flat(ts);
    flat.len() > 70
        || tts
            .iter()
            .any(|t| matches!(t, TokenTree::Group(g) if g.delimiter() == Delimiter::Brace && multiline(&g.stream())))
}

fn print_flat(ts: &TokenStream) -> String {
    ts.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn round(src: &str) -> String {
        print(&TokenStream::from_str(src).unwrap(), 0)
    }

    /// 整形の前後でトークン列が変わらず、結合していた記号（`==>` など）は結合したまま。
    fn same_tokens(src: &str) {
        let out = round(src);
        let a = flat(&TokenStream::from_str(src).unwrap());
        let b = flat(&TokenStream::from_str(&out).unwrap());
        let chars = |v: &[(String, bool)]| v.iter().map(|x| x.0.clone()).collect::<Vec<_>>();
        assert_eq!(chars(&a), chars(&b), "{out}");
        for (x, y) in a.iter().zip(&b) {
            // 結合を切らず、離れていた記号を結合させない（どちらも字句の意味が変わる）。
            assert_eq!(x.1, y.1, "記号の結合が変わった: {} in {out}", x.0);
        }
    }

    /// (トークン, 結合した記号か)。
    fn flat(ts: &TokenStream) -> Vec<(String, bool)> {
        let mut out = Vec::new();
        for t in ts.clone() {
            match t {
                TokenTree::Group(g) => {
                    out.push((format!("{:?}", g.delimiter()), false));
                    out.extend(flat(&g.stream()));
                    out.push(("end".into(), false));
                }
                TokenTree::Punct(p) => {
                    out.push((p.as_char().to_string(), p.spacing() == Spacing::Joint))
                }
                t => out.push((t.to_string(), false)),
            }
        }
        out
    }

    #[test]
    fn keeps_tokens_and_joint_punctuation() {
        same_tokens("let y = a & &b; let z = !!c; let w = - -d; f(&&e);");
        same_tokens(
            "fn f(x: u64) -> (r: u64) requires x > 0, ensures r ==> x <==> y, { let a = x; a }",
        );
        same_tokens(
            "proof { self.nodes.borrow_mut().insert(__eyk_perm_n); } let x = self.order@.first_opt(); a - -b; & &x; x::<Vec<u8>>()",
        );
        let out = round("let x = a==>b;");
        assert!(out.contains("a ==> b"), "{out}");
        let out = round("let x = self.order@.len();");
        assert!(out.contains("self.order@.len()"), "{out}");
    }
}
