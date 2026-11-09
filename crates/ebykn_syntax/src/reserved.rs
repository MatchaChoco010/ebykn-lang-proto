//! 名前で判定する検査の表（#16 の既定、PR #20 のレビュー）。
//!
//! Verus が名前で特別に読むもの（句・証明の組み込み関数、証明の構文の関数、Verus のマクロ）と、
//! Ebykn が名前で特別に読むもの（`impl Invariant` / `impl View`）を、この表にまとめる。
//! 判定はパスの**最後の名前**で行い、綴り（`crate::` / `self::` / `super::` / `::`、改名する
//! `use`、ターボフィッシュ、呼び出しでない値としての参照）によらない。
//!
//! - 裸の名前と、先頭が `::` のパスは、構文解析の段階で拒否する。
//! - それ以外のパス（`crate::x`、`m::x`、`use self::x as y;` など）は、構文解析では判定できない
//!   （クレートの中の定義を指すかもしれない）。[`DeferredName`] として検査（`ebykn_check`）に渡し、
//!   クレートの中の定義に解決するものだけを通す。
//!
//! 名前で判定する検査を足すときは、この表に足す。綴りの総当たりのテスト
//! （`crates/ebykn_check/tests/reserved.rs`）が、表のすべての名前に自動で効く。

/// 改名して付けられない名前（`.eyk` の `use .. as` / `extern crate .. as`。#16 の既定）。依存クレートの
/// 鍵にも使えない（`Ebykn.toml` の `[dependencies]`。#49）。
pub const RESERVED_CRATE_NAMES: &[&str] = &["std", "core", "alloc", "vstd", "ebykn_prelude"];

/// 依存クレートの鍵に使えない名前のうち、[`RESERVED_CRATE_NAMES`] のほかのもの（Verus が自分で
/// 使うクレート）。
pub const RESERVED_DEPENDENCY_NAMES: &[&str] = &["verus_builtin", "verus_builtin_macros"];

/// 表の名前の種類。種類ごとに、どの位置の名前を見るかと、診断の文言が決まる。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReservedKind {
    /// 裸の名前で呼ぶと Verus が句・証明・モードの印として読む `verus_builtin` の関数
    /// （[`VERUS_BUILTIN_CALLS`]）。式のパス、`use`、中を読まないトークン列で見る。
    VerusCall,
    /// 証明の構文の関数（`assert`、`reveal`、`reveal_with_fuel`、`hide`）。`.eykp` だけ。
    ProofCall,
    /// Verus の構文を処理するマクロ（[`VERUS_MACROS`]）。マクロのパスと `use` で見る。
    VerusMacro,
    /// Ebykn の型の性質のトレイト（`Invariant`、`View`）。`impl` のトレイトと `use` で見る。
    /// 受理する書き方は、裸の名前・ジェネリクスなしの `impl Invariant for T` だけ。
    EbyknTrait,
}

/// 証明の構文の関数（仕様 §5.7）。
pub const PROOF_CALLS: &[&str] = &["assert", "reveal", "reveal_with_fuel", "hide"];

/// Ebykn の型の性質のトレイト（仕様 §5.1、§6.4、§7.2）。
pub const EBYKN_TRAITS: &[&str] = &["Invariant", "View"];

/// Verus の構文を処理するマクロ（`verus!` と、Verus と vstd の証明用・内部用のマクロ）。
pub const VERUS_MACROS: &[&str] = &[
    "verus",
    "verus_",
    "proof",
    "proof_decl",
    "proof_with",
    "calc",
    "assert_by_contradiction",
    "assert_seqs_equal",
    "assert_sets_equal",
    "assert_maps_equal",
    "open_atomic_invariant",
    "open_local_invariant",
    "open_atomic_invariant_in_proof",
    "open_local_invariant_in_proof",
    "struct_with_invariants",
    "atomic_with_ghost",
    "state_machine",
    "tokenized_state_machine",
    "case_on_next",
    "case_on_init",
    "builtin_macros",
    "verus_proof_macro_exprs",
    "verus_exec_macro_exprs",
    "verus_proof_expr",
];

/// 裸の名前で呼ぶと Verus が句・証明・モードの印として読む `verus_builtin` の関数。
///
/// 固定した Verus（`release/0.2026.11.02.8bf68ee`）の `verus_builtin` の `src/lib.rs` の公開関数のうち、
/// 契約の句（`requires` など）、証明（`assert_` など）、モードの変換（`tracked_exec` など）、
/// 書き下しの内部の印に当たるものをすべて挙げた。値を求めるだけの仕様の関数（`equal`、`imply`、
/// `add`、`spec_literal_int` など）と、Ebykn が使う `old` は入れない（`docs/current-spec.md`）。
/// `opens_invariants_none` / `opens_invariants_any` は Verus の句の名前で、関数ではないが入れる。
pub const VERUS_BUILTIN_CALLS: &[&str] = &[
    // 句
    "requires",
    "ensures",
    "returns",
    "recommends",
    "invariant",
    "invariant_except_break",
    "decreases",
    "decreases_when",
    "decreases_by",
    "recommends_by",
    "extra_dependency",
    "opens_invariant_mask",
    "opens_invariants_none",
    "opens_invariants_any",
    "no_unwind",
    "no_unwind_when",
    "atomic_spec",
    "atomic_call_loop",
    "default_ensures",
    "no_method_body",
    "with_triggers",
    "inv_mask_none",
    "inv_mask_any",
    "inv_mask_list",
    "inv_mask_list_compl",
    "inv_mask_set",
    // 証明
    "admit",
    "assume",
    "assume_",
    "assert_",
    "assert_by",
    "assert_by_compute",
    "assert_by_compute_only",
    "assert_nonlinear_by",
    "assert_bitvector_by",
    "assert_forall_by",
    "assert_bit_vector",
    "use_type_invariant",
    "reveal_hide_",
    "reveal_hide_internal_path_",
    "reveal_strlit",
    "reveal_byteslit",
    "proof_with_callee",
    "inline_air_stmt",
    // モードの変換と書き下しの内部
    "ghost_exec",
    "tracked_exec",
    "tracked_exec_borrow",
    "closure_to_fn_spec",
    "closure_to_fn_proof",
    "constrain_type",
    "get_future_output_type",
    "erased_ghost_value",
    "shadow_ghost_value",
    "verus_erasure_get_first",
    "dummy_capture_new",
    "dummy_capture_consume",
    "mutable_reference_tie",
    "two_phase_mutable_reference_tie",
    "has_resolved",
    "has_resolved_unsized",
    "mut_ref_current",
    "mut_ref_future",
    "mut_ref_tracked",
    "final_",
    "after_borrow",
    "shr_ref_struct_wrap",
];

/// 表のすべての名前と種類。
pub fn all() -> impl Iterator<Item = (&'static str, ReservedKind)> {
    let k = |xs: &'static [&'static str], kind| xs.iter().map(move |n| (*n, kind));
    k(VERUS_BUILTIN_CALLS, ReservedKind::VerusCall)
        .chain(k(PROOF_CALLS, ReservedKind::ProofCall))
        .chain(k(VERUS_MACROS, ReservedKind::VerusMacro))
        .chain(k(EBYKN_TRAITS, ReservedKind::EbyknTrait))
}

/// 名前（`r#` を外したもの）が表にあれば、その種類。
pub fn kind_of(name: &str) -> Option<ReservedKind> {
    all().find(|(n, _)| *n == name).map(|(_, k)| k)
}

/// 式のパス・`use` の最後の名前として見る種類か（関数として呼べるもの）。
pub fn is_call(kind: ReservedKind) -> bool {
    matches!(kind, ReservedKind::VerusCall | ReservedKind::ProofCall)
}

/// 構文解析で判定できなかった、表の名前を最後に持つパス。検査が名前解決して決める。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeferredName {
    /// パスの名前の列（`r#` を外したもの。ジェネリクスは除く）。
    pub segments: Vec<String>,
    pub kind: ReservedKind,
    /// 診断に出す書き方（`crate::requires(..)`、`use self::requires as rq` など）。
    pub written: String,
}
