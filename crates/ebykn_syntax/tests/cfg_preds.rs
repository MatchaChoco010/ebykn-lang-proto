//! cfg の述語の位置（#155）。構文木の属性の判定（`ast::has_cfg`、`denylist::attr_denied`）と、トークンの
//! 検査（`parse::tokens`）は、同じ走査（`denylist::for_each_cfg_pred`）で述語を決める。ここでは、構文木の
//! 属性から始めた走査と、属性のトークンから始めた走査が同じ述語を同じ順に渡すこと、cfg とみなす属性と
//! みなさない属性の両方を固定する。

mod support;
use ebykn_syntax::{ast, denylist};
use support::*;

/// cfg とみなす属性（述語の位置に cfg がある）。
const CFG: &[&str] = &[
    "cfg(test)",
    "r#cfg(test)",
    "::cfg(test)",
    "cfg",
    "cfg = \"x\"",
    "cfg::x(test)",
    "cfg_attr(test, cfg(any()))",
    "r#cfg_attr(test, r#cfg(any()))",
    "cfg_attr(test, allow(cfg), cfg(any()))",
    "cfg_attr(test, cfg_attr(unix, cfg(any())))",
    "cfg_attr(test, cfg)",
    "cfg_attr(test, ::cfg(any()))",
    "cfg_attr(all(cfg), allow(unused))",
    "cfg_attr(test, cfg_attr(any(r#cfg), allow(unused)))",
];

/// cfg とみなさない属性（述語の位置に cfg がない）。
const NOT_CFG: &[&str] = &[
    "allow(unused)",
    "allow(cfg)",
    "doc(cfg(test))",
    "cfg_attr(test, allow(cfg))",
    "cfg_attr(test, allow(r#cfg))",
    "cfg_attr(test, allow(unused, cfg))",
    "cfg_attr(test, doc(cfg(test)))",
    "cfg_attr(test, cfg_attr(unix, allow(cfg)))",
    "cfg_attr",
    "cfg_attr = \"cfg\"",
    "x::cfg(test)",
];

fn attr(body: &str) -> ast::Attr {
    let src = format!("#[{body}]\npub fn f() {{}}");
    let (_, p) = parse(&src);
    let item = p
        .file
        .items
        .first()
        .unwrap_or_else(|| panic!("項目がない: {src}"));
    item.attrs
        .first()
        .unwrap_or_else(|| panic!("属性がない: {src}"))
        .clone()
}

/// 構文木の属性から始めた走査が渡す述語。
fn from_ast(a: &ast::Attr) -> Vec<(String, bool)> {
    let mut v = Vec::new();
    denylist::for_each_attr_cfg_pred(a, &mut |p, c| v.push((p.to_string(), c)));
    v
}

/// 属性のトークンから始めた走査（トークンの検査と同じ入口）が渡す述語。
fn from_tokens(body: &str) -> Vec<(String, bool)> {
    let ts: proc_macro2::TokenStream = body.parse().expect("トークンに分けられる");
    let tts: Vec<proc_macro2::TokenTree> = ts.into_iter().collect();
    let (kind, rest) = denylist::attr_head(&tts);
    let mut v = Vec::new();
    denylist::for_each_cfg_pred(kind, rest, &mut |p, c| v.push((p.to_string(), c)));
    v
}

#[test]
fn the_ast_and_the_tokens_visit_the_same_predicates() {
    for body in CFG.iter().chain(NOT_CFG) {
        assert_eq!(from_ast(&attr(body)), from_tokens(body), "#[{body}]");
    }
}

#[test]
fn cfg_in_a_predicate_position_is_a_cfg() {
    for body in CFG {
        assert!(ast::has_cfg(&[attr(body)]), "cfg とみなすべき: #[{body}]");
    }
}

#[test]
fn cfg_elsewhere_is_not_a_cfg() {
    for body in NOT_CFG {
        let a = attr(body);
        assert!(
            !ast::has_cfg(std::slice::from_ref(&a)),
            "cfg ではない: #[{body}]"
        );
        assert!(from_ast(&a).iter().all(|(_, cond)| *cond), "#[{body}]");
    }
}
