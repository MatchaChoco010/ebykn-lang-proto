//! 仕様の付録の例が AST に読めること。付録 B の `stack` は M2 の golden の入力。

mod support;
use ebykn_syntax::ast::*;
use support::*;

const STACK: &str = include_str!("../../../tests/golden/stack/src/lib.eyk");
const DLIST: &str = include_str!("fixtures/dlist.eyk");

fn impl_items(f: &File, n: usize) -> Vec<String> {
    let ItemKind::Impl(i) = &f.items[n].kind else {
        panic!("impl");
    };
    i.items.iter().map(item_head).collect()
}

fn method_body(f: &File, n: usize, name: &str) -> Vec<String> {
    let ItemKind::Impl(i) = &f.items[n].kind else {
        panic!("impl");
    };
    for it in &i.items {
        if let ItemKind::Fn(m) = &it.kind
            && m.sig.ident.name == name
        {
            let Some(FnBody::Block(b)) = &m.body else {
                panic!("body");
            };
            return b.stmts.iter().map(stmt).collect();
        }
    }
    panic!("{name} がない");
}

/// 付録 B を書き写したものであること（仕様と食い違ったら golden の入力が古い）。
#[test]
fn stack_fixture_is_the_appendix() {
    let spec = include_str!("../../../docs/ebykn-spec.md");
    let b = &spec[spec.find("## 付録 B").expect("付録 B")..];
    let code = &b[b.find("```\n").unwrap() + 4..];
    let code = &code[..code.find("```").unwrap()];
    assert_eq!(STACK, code);
    let a = &spec[spec.find("### A.1").expect("付録 A.1")..];
    let code = &a[a.find("```\n").unwrap() + 4..];
    let code = &code[..code.find("```").unwrap()];
    assert_eq!(DLIST, code);
}

#[test]
fn stack_reads() {
    let f = ok(STACK);
    assert_eq!(
        f.items.iter().map(item_head).collect::<Vec<_>>(),
        [
            "pub struct Node",
            "pub struct Stack owns { nodes: Region<Ptr<Node<T>>> } spec { order: Seq<Ptr<Node<T>>> }",
            "impl Invariant for Stack<T> (1 items)",
            "impl Stack<T> (3 items)",
        ]
    );
    assert_eq!(
        impl_items(&f, 2),
        ["closed spec fn invariant(&self) -> bool = (and \
          (forall [p in (@ self order)] (.contains (@ self nodes) p)) \
          (== (. self head) (.first (@ self order))) \
          (forall [i in (.. 0 (.len (@ self order)))] (== (. (index (@ self nodes) (index (@ self order) i)) next) (.get (@ self order) (+ i 1)))))"]
    );
    assert_eq!(
        method_body(&f, 3, "new"),
        ["(struct Stack head:None @nodes:(call Region::empty) @order:(call Seq::empty))"]
    );
    assert_eq!(
        method_body(&f, 3, "push"),
        [
            "(let n (call Ptr::new (struct Node val:v next:(. self head))))",
            "(= (. self head) (call Some n));",
            "(put self@nodes[n])",
            "(spec= (@ self order) (+ seq!(n) (@ self order)))",
        ]
    );
    assert_eq!(
        method_body(&f, 3, "pop"),
        [
            "(let h (? (. self head)))",
            "(take self@nodes[h])",
            "(let node (.free h))",
            "(= (. self head) (. node next));",
            "(spec= (@ self order) (.drop_first (@ self order)))",
            "(call Some (. node val))",
        ]
    );
}

#[test]
fn dlist_reads() {
    let f = ok(DLIST);
    assert_eq!(
        f.items.iter().map(item_head).collect::<Vec<_>>(),
        [
            "pub struct Node",
            "pub struct DList owns { nodes: Region<Ptr<Node<T>>> } spec { order: Seq<Ptr<Node<T>>> }",
            "impl View for DList<T> (2 items)",
            "impl Invariant for DList<T> (1 items)",
            "impl DList<T> (5 items)",
        ]
    );
    assert_eq!(
        impl_items(&f, 4),
        [
            "pub closed spec fn has(&self, h: Ptr<Node<T>>) -> bool = (.contains (@ self order) h)",
            "pub fn new() -> Self {..}",
            "pub fn push_back(&mut self, v: T) -> (h: Ptr<Node<T>>) (ensures (== (.view self) (.push (.view (call old self)) v)) (.has self h)) {..}",
            "pub fn peek_front(&self) -> Option<&T> {..}",
            "pub fn remove(&mut self, h: Ptr<Node<T>>) -> (v: T) (requires (.has self h)) (ensures (== (.view self) (.remove (.view (call old self)) (.index_of (@ (call old self) order) h)))) {..}",
        ]
    );
    assert_eq!(
        method_body(&f, 4, "push_back"),
        [
            "(let n (call Ptr::new (struct Node val:v prev:(. self tail) next:None)))",
            "(match (. self tail) (Some(t) => (borrow mut self@nodes[t] {(= (. (paren (* t)) next) (call Some n));})) (None => (= (. self head) (call Some n))))",
            "(= (. self tail) (call Some n));",
            "(put self@nodes[n])",
            "(spec= (@ self order) (.push (@ self order) n))",
            "n",
        ]
    );
    assert_eq!(
        method_body(&f, 4, "peek_front"),
        [
            "(let h (? (. self head)))",
            "(borrow self@nodes[h] {(call Some (& (. (paren (* h)) val)))})",
        ]
    );
}
