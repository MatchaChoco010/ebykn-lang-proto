//! `ebykn verify` の流れ（`build/verus/` に書き出した後）。`main.rs` と、Verus を使うテスト
//! （`tests/no_cheating.rs`）が同じ関数を使う。
//!
//! 1. Verus で検証する（`--no-cheating` なし。Ebykn は `admit()` と `external_body` を出すため）。
//! 2. 検証まで進んだら、`--no-cheating --no-verify` でもう一度走らせ、拒否の場所を Ebykn が出した前提と
//!    照らし合わせる（[`crate::no_cheating`]、#37）。
//! 3. 項目ごとの状態を決める。紛れ込んだ前提があれば、どの項目も verified にしない。

use std::path::Path;

use ebykn_lower::Output;

use crate::no_cheating::{self, Cheating};
use crate::verus::{self, ItemStatus, Mode, Run, Summary, Verus};

/// `verify` の結果。
pub struct Verified {
    /// 1 回目（検証）の実行。
    pub run: Run,
    pub summary: Summary,
    /// 全項目の状態。
    pub all: Vec<ItemStatus>,
    /// 2 回目（`--no-cheating`）の照合。
    pub cheating: Cheating,
    /// 2 回目の実行の診断（紛れ込んだ前提があれば出す）。
    pub cheating_text: String,
}

impl Verified {
    /// 項目ごとの状態を、別の書き下しの結果（依存クレートの名前を確定し直したもの。#49）で決め直す。
    /// 書き下した `.rs` は同じで、違うのは手続きマクロの前提の一覧だけ。
    pub fn restatus(&mut self, output: &Output) {
        self.all = verus::statuses(
            &output.files,
            output,
            &self.run.locations(),
            self.run.checked.as_ref(),
            self.run.failed.as_ref(),
            self.summary,
            self.cheating.taints(),
        );
    }

    /// `verify` が成功か（表示する項目に failed / unknown がなく、Verus が成功し、紛れ込んだ前提がない）。
    pub fn success(&self, shown: &[ItemStatus]) -> bool {
        self.run.success
            && self.cheating.clean()
            && !shown
                .iter()
                .any(|s| matches!(s.status, "failed" | "unknown"))
    }
}

/// 書き出した `build` に Verus をかけ、項目ごとの状態と照合の結果を返す。
pub fn verify_build(verus: &Verus, build: &Path, output: &Output) -> Result<Verified, String> {
    let run = verus::run(verus, build, Mode::Verify)?;
    let summary = run.summary();
    let (cheating, cheating_text) = match summary {
        Summary::Ran { .. } => {
            let second = verus::run(verus, build, Mode::NoCheating)?;
            (no_cheating::judge(output, &second), second.text)
        }
        Summary::Stopped { .. } => (Cheating::NotRun, String::new()),
    };
    let all = verus::statuses(
        &output.files,
        output,
        &run.locations(),
        run.checked.as_ref(),
        run.failed.as_ref(),
        summary,
        cheating.taints(),
    );
    Ok(Verified {
        run,
        summary,
        all,
        cheating,
        cheating_text,
    })
}
