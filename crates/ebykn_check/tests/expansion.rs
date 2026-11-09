//! 絞り込みの署名展開（仕様 §6.4）と、書き下しに渡す事実（`Facts`）。

mod support;
use ebykn_check::facts::RegionRef;
use ebykn_check::sig::{FnExpansion, Refine, Subject, Wrap};
use support::*;

const STACK: &str = include_str!("../../../tests/golden/stack/src/lib.eyk");

fn inv(ty: &str, subject: Subject, wrap: Wrap) -> Refine {
    Refine {
        ty: vec![ty.to_string()],
        subject,
        wrap,
    }
}

/// 関数名の位置の順に並べた展開。
fn expansions(src: &str) -> Vec<FnExpansion> {
    let (c, lines) = run(src);
    assert!(
        lines.iter().all(|l| l.contains("W0601")),
        "{}",
        lines.join("\n")
    );
    c.facts.expansions.values().cloned().collect()
}

#[test]
fn stack_signatures_expand_as_in_the_hand_written_target() {
    let exps = expansions(STACK);
    // new() -> Self、push(&mut self)、pop(&mut self) の順。
    assert_eq!(
        exps,
        [
            FnExpansion {
                requires: vec![],
                ensures: vec![inv("Stack", Subject::Var("__eyk_r".into()), Wrap::None)],
                name_ret: true,
            },
            FnExpansion {
                requires: vec![inv("Stack", Subject::Old("self".into()), Wrap::None)],
                ensures: vec![inv("Stack", Subject::Final("self".into()), Wrap::None)],
                name_ret: false,
            },
            FnExpansion {
                requires: vec![inv("Stack", Subject::Old("self".into()), Wrap::None)],
                ensures: vec![inv("Stack", Subject::Final("self".into()), Wrap::None)],
                name_ret: false,
            },
        ]
    );
}

const S: &str = "pub struct S { a: u64 }\nimpl Invariant for S { closed spec fn invariant(&self) -> bool = self.a > 0; }\n";

#[test]
fn positions_and_wrappers() {
    let src = format!(
        "{S}pub fn f(x: S, y: &S, z: &mut S, o: Option<S>, r: Result<S, u64>) -> (k: Option<S>) {{ o }}\n"
    );
    let e = &expansions(&src)[0];
    assert_eq!(
        e.requires,
        [
            inv("S", Subject::Var("x".into()), Wrap::None),
            inv("S", Subject::Var("y".into()), Wrap::None),
            inv("S", Subject::Old("z".into()), Wrap::None),
            inv("S", Subject::Var("o".into()), Wrap::Option),
            inv("S", Subject::Var("r".into()), Wrap::Result),
        ]
    );
    assert_eq!(
        e.ensures,
        [
            inv("S", Subject::Final("z".into()), Wrap::None),
            inv("S", Subject::Var("k".into()), Wrap::Option),
        ]
    );
    assert!(!e.name_ret);
}

#[test]
fn non_pub_functions_of_the_own_impl_are_not_expanded() {
    let src = format!(
        "{S}impl S {{ fn helper(&mut self) {{ }} pub fn api(&self) {{ }} }}\npub struct U {{ s: u64 }}\nimpl U {{ fn g(&self, s: &S) {{ }} }}\n"
    );
    let exps = expansions(&src);
    // helper は足さない。api と、U の非 pub の g（S の不変条件）は足す。
    assert_eq!(exps.len(), 2, "{exps:?}");
    assert_eq!(
        exps[0].requires,
        [inv("S", Subject::Var("self".into()), Wrap::None)]
    );
    assert_eq!(
        exps[1].requires,
        [inv("S", Subject::Var("s".into()), Wrap::None)]
    );
}

#[test]
fn w0601_for_positions_that_are_not_expanded() {
    let src = format!("{S}pub fn f(v: Vec<S>) {{ }}\n");
    let (c, lines) = run(&src);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].contains(": W0601: "), "{}", lines[0]);
    assert!(c.facts.expansions.is_empty());
    // 警告は失敗ではない。
    assert!(c.diags.iter().all(|d| d.code().is_warning()));
}

#[test]
fn nested_invariants() {
    let src = format!(
        "{S}pub struct T {{ s: S, o: Option<S>, n: u64 }}\nimpl Invariant for T {{ spec fn invariant(&self) -> bool = true; }}\n"
    );
    let (c, _) = run(&src);
    assert_eq!(
        c.facts.nested.get(&vec!["T".to_string()]).cloned(),
        Some(vec![
            ("s".into(), vec!["S".into()], Wrap::None),
            ("o".into(), vec!["S".into()], Wrap::Option),
        ])
    );
}

#[test]
fn stack_facts_for_lowering() {
    let (c, lines) = run(STACK);
    assert!(lines.is_empty(), "{lines:?}");
    let f = &c.facts;
    // `let n = Ptr::new(..)` と `take self@nodes[h]` / `put self@nodes[n]`。
    assert_eq!(f.perm_lets.values().collect::<Vec<_>>(), ["n"]);
    assert_eq!(
        f.region_stmts.values().cloned().collect::<Vec<_>>(),
        [
            RegionRef::SelfField("nodes".into()),
            RegionRef::SelfField("nodes".into())
        ]
    );
    // `h.free()`。
    assert_eq!(f.frees.values().collect::<Vec<_>>(), ["h"]);
    // 不変条件の `self@nodes[self@order[i]]` は論理の領域添字、`self@order[i]` は普通の添字。
    assert_eq!(
        f.region_index.values().cloned().collect::<Vec<_>>(),
        [RegionRef::SelfField("nodes".into())]
    );
    // リテラルの `@nodes`（owns）と `@order`（spec）。
    assert_eq!(f.erased.len(), 2);
}

#[test]
fn perm_derefs_and_extra_call_arguments() {
    let src = format!(
        "{PRELUDE}fn set(p: Ptr<Node>, v: u64) uses &mut p@ ensures final(*p).val == v {{ (*p).val = v; }}\nfn f(q: u64) -> u64 {{ let p = Ptr::new(Node {{ val: 1 }}); set(p, 2); let n = p.free(); *q2(q) }}\nfn q2(x: u64) -> &'static u64 {{ &0 }}\n"
    );
    let (c, lines) = run(&src);
    assert!(lines.is_empty(), "{lines:?}");
    // ensures の `final(*p)` の `*p`、本体の `(*p).val = v` の `*p`。f の `*q2(q)` は Rust の `*`。
    assert_eq!(c.facts.derefs.values().collect::<Vec<_>>(), ["p", "p"]);
    let calls: Vec<_> = c.facts.calls.values().cloned().collect();
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0][0],
        ebykn_check::facts::ExtraArg::Perm {
            var: "p".into(),
            mode: ebykn_syntax::ast::PermMode::Mut
        }
    );
}

#[test]
fn region_indexes_inside_spec_closures() {
    // 付録 A の `view`：論理文脈のクロージャの中の `self@nodes[p]` も領域添字。
    let src = format!(
        "{PRELUDE}impl Pool {{ pub closed spec fn vals(&self) -> Seq<u64> = self@order.map(|i: int, p: Ptr<Node>| self@nodes[p].val); }}"
    );
    let (c, lines) = run(&src);
    assert!(lines.is_empty(), "{lines:?}");
    assert_eq!(c.facts.region_index.len(), 1);
}
