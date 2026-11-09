//! 字句・構文・AST（仕様 §4、§5）と、構文の段階の診断（仕様 §10.1）。
//!
//! 入口は [`parse_eyk`]。`.eyk` を読んで [`ast::File`] と診断を返す。

// AST の列挙は節点の大きさがまちまちで、箱に入れると使う側が読みにくくなる。
#![allow(clippy::large_enum_variant)]

pub mod ast;
pub mod diag;
mod parse;
pub mod span;

pub use parse::{Parsed, parse_eyk};

/// `.eyk` で呼べるマクロ（#16 の既定）。std のマクロと、vstd の仕様用のマクロ。
/// 裸の名前か、std / core / alloc（vstd のマクロは vstd）から始まるパスで呼べる。
pub const ALLOWED_MACROS: &[&str] = &[
    // std
    "vec",
    "println",
    "print",
    "eprintln",
    "eprint",
    "format",
    "format_args",
    "write",
    "writeln",
    "panic",
    "assert",
    "assert_eq",
    "assert_ne",
    "debug_assert",
    "debug_assert_eq",
    "debug_assert_ne",
    "unreachable",
    "unimplemented",
    "todo",
    "matches",
    "concat",
    "stringify",
    "line",
    "column",
    "file",
    "module_path",
    "cfg",
    "env",
    "option_env",
    "include_str",
    "include_bytes",
    "compile_error",
    // vstd
    "seq",
    "set",
    "map",
];

/// [`ALLOWED_MACROS`] のうち vstd のもの。引数を Ebykn の式として読む。
pub const VSTD_MACROS: &[&str] = &["seq", "set", "map"];

/// 裸の名前で呼ぶと Verus が句・証明・モードの印として読む `verus_builtin` の関数（#16 の既定）。
/// `.eyk` では、パスなしの呼び出し `requires(..)` などを Verus の構文として拒否する。
///
/// 固定した Verus（`release/0.2026.11.02.8bf68ee`）の `verus_builtin` の `src/lib.rs` の公開関数のうち、
/// 契約の句（`requires` など）、証明（`assert_` など）、モードの変換（`tracked_exec` など）、
/// 書き下しの内部の印に当たるものをすべて挙げた。値を求めるだけの仕様の関数（`equal`、`imply`、
/// `add`、`spec_literal_int` など）と、Ebykn が使う `old` は入れない（`OPEN.md`）。
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
