//! EBYKN: Ebykn（https://github.com/MatchaChoco010/ebykn-lang-proto）が構文を足すための
//! 差し込み口。上流の verus_syn にはないファイル。
//!
//! 差し込み口が登録されていなければ、上流と同じに動く。登録した関数は、各位置で
//! Ebykn の構文に当たれば入力を読み進めて `Ok(true)` を返し、当たらなければ何も
//! 読まずに `Ok(false)` を返す。読んだトークン列は `Verbatim` の節として残り、
//! 構造は Ebykn の側で読み直す。

use crate::error::Result;
use crate::parse::ParseStream;

/// 差し込み口を呼ぶ位置。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EbyknSite {
    /// 文の先頭（属性の後）。読んだものは `Stmt::Item(Item::Verbatim(..))` になる。
    Stmt,
    /// 式の原子の先頭。読んだものは `Expr::Verbatim(..)` になる。`allow_struct` は、
    /// その位置で構造体リテラルを読めるか（`if` の条件などでは読めない）。
    Atom { allow_struct: bool },
    /// 後置の `@` の位置。読んだものは、それまでの式と合わせて `Expr::Verbatim(..)` になる。
    Trailer,
    /// 式の位置で、パスの直後の `{` の位置。読んだものはパスと合わせて `Expr::Verbatim(..)` になる。
    StructBody,
    /// パターンの位置で、パスの直後の `{` の位置。読んだものはパスと合わせて `Pat::Verbatim(..)` になる。
    PatStructBody,
}

/// 差し込み口の関数。
pub type EbyknHook = fn(EbyknSite, ParseStream) -> Result<bool>;

static HOOK: std::sync::OnceLock<EbyknHook> = std::sync::OnceLock::new();

/// 差し込み口を登録する。最初の登録だけが効く。
pub fn set_ebykn_hook(hook: EbyknHook) {
    let _ = HOOK.set(hook);
}

pub(crate) fn call(site: EbyknSite, input: ParseStream) -> Result<bool> {
    match HOOK.get() {
        Some(hook) => hook(site, input),
        None => Ok(false),
    }
}
