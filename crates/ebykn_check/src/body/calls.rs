//! 呼び出しの消去される引数（仕様 §6.2 の「消費」）：実行時の引数に付いて渡る権限
//! （`uses p@` / `&p@` / `&mut p@`）と、`uses` の後に並べて渡す領域（#19 の既定 P1。
//! `f(x, uses &mut r)`）。
//!
//! 領域の状態（`docs/current-spec.md` §5.2、`OPEN.md`）：
//! - 移動（`r`）で渡した領域変数は R から外し、同じブロックの中で後から使えば E0519。ブロックを
//!   出ると R はブロックの前に戻るので、ブロックの外で使った誤りは Rust の所有権の検査に任せる
//!   （Verus が拒む。黙って通ることはない）。`unchecked fn` の中では E0519 を出さず、本体は
//!   `external_body` で Verus が見ないので、移動した領域を使っても通る（`unchecked` の前提として出る）。
//! - 借用（`&r` / `&mut r`）は呼び出しの間だけで、状態を変えない。

use ebykn_syntax::ast::{self, Expr, PermMode, Region, RegionArg};
use ebykn_syntax::diag::{CheckMsg, Op};
use ebykn_syntax::span::Span;

use super::{Walker, ident_of};
use crate::facts::{ExtraArg, RegionRef};
use crate::index::{ErasedKind, FnInfo, UseInfo};

impl Walker<'_> {
    /// 呼び出しの消去される引数を当てる。`info` は解決できた呼ばれる側（解決できなければ `None`）。
    /// `args` は受け手を除いた実行時の引数。
    pub(super) fn apply_uses(
        &mut self,
        span: Span,
        info: Option<&FnInfo>,
        args: &[Expr],
        uses: &[RegionArg],
    ) {
        let regions: Vec<Option<RegionRef>> = uses.iter().map(|u| self.region_arg(u)).collect();
        self.check_region_aliases(uses, &regions);
        let mut extra = Vec::new();
        match info {
            Some(info) => {
                self.check_region_decl(span, info, uses);
                let mut next = 0;
                for u in &info.uses {
                    match u {
                        UseInfo::Perm {
                            name, param, mode, ..
                        } => {
                            if let Some(a) = self.perm_arg(span, name, *param, *mode, args) {
                                extra.push(a);
                            }
                        }
                        UseInfo::Named { .. } => {
                            if next < uses.len() {
                                extra.push(ExtraArg::Region(next));
                                next += 1;
                            }
                        }
                    }
                }
                // 宣言より多く渡した分も書き下しに残す（E0518 で止まる）。
                extra.extend((next..uses.len()).map(ExtraArg::Region));
            }
            // 解決できない呼び出し（外のクレートの関数、クロージャなど）は書いた順に渡す。
            // 受け取らない関数なら Verus が型の誤りとして拒む。
            None => extra.extend((0..uses.len()).map(ExtraArg::Region)),
        }
        for (u, r) in uses.iter().zip(&regions) {
            if let Some(r) = r {
                self.pass_region(u, r);
            }
        }
        if !extra.is_empty() {
            self.facts.calls.insert(span.into(), extra);
        }
    }

    /// 実行時の引数に付いて渡る権限（`uses p@` / `&p@` / `&mut p@`）。
    fn perm_arg(
        &mut self,
        span: Span,
        name: &str,
        param: usize,
        mode: PermMode,
        args: &[Expr],
    ) -> Option<ExtraArg> {
        let Some(x) = args.get(param).and_then(ident_of) else {
            let at = args.get(param).map_or(span, |a| a.span);
            self.push(
                at,
                CheckMsg::UsesArgNotVar {
                    param: name.to_string(),
                },
            );
            return None;
        };
        // 証明の中の移動と `&mut` の貸しは、論理の文脈でも E0520 の操作（[`Walker::proof_op`]）。
        if mode != PermMode::Shared {
            self.proof_op(&format!("{}@", x.name));
        }
        if !self.logic {
            match mode {
                PermMode::Move => self.consume(&x.name, Op::Move, x.span),
                PermMode::Mut => self.lend(&x.name, true, x.span),
                PermMode::Shared => self.lend(&x.name, false, x.span),
            }
        }
        Some(ExtraArg::Perm {
            var: x.name.clone(),
            mode,
        })
    }

    /// 呼ばれる側の `uses` の領域の宣言と、数と渡し方を比べる（E0518）。
    fn check_region_decl(&mut self, span: Span, info: &FnInfo, uses: &[RegionArg]) {
        let decl: Vec<(&String, PermMode)> = info
            .uses
            .iter()
            .filter_map(|u| match u {
                UseInfo::Named { name, mode } => Some((name, *mode)),
                UseInfo::Perm { .. } => None,
            })
            .collect();
        if decl.len() != uses.len() {
            self.push(
                span,
                CheckMsg::RegionArgCount {
                    callee: info.name.clone(),
                    expected: decl
                        .iter()
                        .map(|(n, m)| format!("{}{n}", mode_prefix(*m)))
                        .collect(),
                    found: uses.len(),
                },
            );
            return;
        }
        for ((name, mode), u) in decl.iter().zip(uses) {
            if *mode != u.mode {
                self.push(
                    u.span,
                    CheckMsg::RegionArgMode {
                        callee: info.name.clone(),
                        param: (*name).clone(),
                        expected: *mode,
                        found: u.mode,
                    },
                );
            }
        }
    }

    /// 呼び出しで渡す領域を決める（仕様 §5.4 の `RegionExpr`）。
    fn region_arg(&mut self, u: &RegionArg) -> Option<RegionRef> {
        match &u.region {
            Region::SelfField { self_span, field } => {
                if !self.cx.has_self {
                    self.push(*self_span, CheckMsg::SelfErasedWithoutSelf);
                    return None;
                }
                if self.closure_use_region_self(field, u.span) {
                    return None;
                }
                match self.self_struct().and_then(|s| s.erased_kind(&field.name)) {
                    Some(ErasedKind::Owns) => Some(RegionRef::SelfField(field.name.clone())),
                    Some(ErasedKind::Spec) => {
                        self.push(
                            u.span,
                            CheckMsg::SpecFieldAsRegion {
                                field: field.name.clone(),
                            },
                        );
                        None
                    }
                    None => {
                        self.push(
                            field.span,
                            CheckMsg::NotARegionArg {
                                name: field.name.clone(),
                            },
                        );
                        None
                    }
                }
            }
            Region::Var(v) => {
                if self.closure_use(&v.name, u.span) {
                    return None;
                }
                if self.regions.contains(&v.name) {
                    return Some(RegionRef::Var(v.name.clone()));
                }
                if !self.region_moved(v) {
                    self.push(
                        v.span,
                        CheckMsg::NotARegionArg {
                            name: v.name.clone(),
                        },
                    );
                }
                None
            }
        }
    }

    /// `v` が移動した領域なら E0519 を出して `true`。
    pub(super) fn region_moved(&mut self, v: &ast::Ident) -> bool {
        if !self.moved_regions.contains(&v.name) {
            return false;
        }
        self.push(
            v.span,
            CheckMsg::RegionMoved {
                name: v.name.clone(),
            },
        );
        true
    }

    /// 同じ呼び出しで同じ領域を二度渡し、一方が `&mut` か移動なら E0519。
    fn check_region_aliases(&mut self, uses: &[RegionArg], regions: &[Option<RegionRef>]) {
        for (i, (u, r)) in uses.iter().zip(regions).enumerate() {
            let Some(r) = r else { continue };
            let clash = uses[..i].iter().zip(regions).any(|(w, q)| {
                q.as_ref() == Some(r) && (u.mode != PermMode::Shared || w.mode != PermMode::Shared)
            });
            if clash {
                self.push(
                    u.span,
                    CheckMsg::RegionPassedTwice {
                        region: region_text(r),
                    },
                );
            }
        }
    }

    /// 渡した領域の状態を変える（移動なら R から外す）。借りた領域の移動と、共有で借りた領域の
    /// `&mut` は E0519。
    fn pass_region(&mut self, u: &RegionArg, r: &RegionRef) {
        // 証明の中で領域を移動や `&mut` で渡すのは、論理の文脈でも E0520 の操作。
        if u.mode != PermMode::Shared {
            self.proof_op(&region_text(r));
        }
        let held = match r {
            RegionRef::Var(v) => self.region_modes.get(v).copied().unwrap_or(PermMode::Move),
            // `self@x` は `self` を通して借りている。
            _ => PermMode::Mut,
        };
        if !self.logic {
            match (u.mode, held) {
                (PermMode::Move, PermMode::Shared | PermMode::Mut) => {
                    self.push(
                        u.span,
                        CheckMsg::RegionNotMovable {
                            region: region_text(r),
                        },
                    );
                    return;
                }
                (PermMode::Mut, PermMode::Shared) => {
                    self.push(
                        u.span,
                        CheckMsg::RegionSharedAsMut {
                            name: region_text(r),
                        },
                    );
                    return;
                }
                _ => {}
            }
        }
        if let RegionRef::Var(v) = r {
            if held != PermMode::Move {
                self.facts.region_arg_refs.insert(u.span.into());
            }
            if u.mode == PermMode::Move && !self.logic {
                self.regions.remove(v);
                self.moved_regions.insert(v.clone());
            }
        }
    }
}

fn mode_prefix(m: PermMode) -> &'static str {
    match m {
        PermMode::Move => "",
        PermMode::Shared => "&",
        PermMode::Mut => "&mut ",
    }
}

fn region_text(r: &RegionRef) -> String {
    match r {
        RegionRef::SelfField(f) => format!("self@{f}"),
        RegionRef::Var(v) => v.clone(),
        RegionRef::At { region, .. } => region_text(region),
    }
}
