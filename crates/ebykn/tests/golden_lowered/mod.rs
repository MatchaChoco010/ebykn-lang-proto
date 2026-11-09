//! golden の書き下しの期待（`tests/golden/<名前>/expected/verus/`）を、生成物と比べる・生成物から作り直す。
//!
//! 比べるのは `tests/golden.rs` の `lowered_verus_matches_expected`、作り直すのは `cargo xtask golden-bless`
//! （`examples/golden_bless.rs` からこのモジュールを呼ぶ）。比べ方と作り直し方を同じ場所に置き、作り直した
//! 期待が比較を通ることをテストで固定する。
//!
//! 期待には手で書いたコメントを置かない（ケースの説明は `README.md` に書く。`OPEN.md`）。比較はコメントを
//! 見ないので、置いても通るが、作り直すと消える。作り直しは、消えるコメントがあれば何も書かずに止まる。

#![allow(dead_code)] // 比べる側（テスト）と作り直す側（example）で使う関数が違う。

use std::path::Path;

use ebykn::check_project;

/// 期待の `Cargo.toml` に書く `ebykn_prelude` の `path`（`expected/verus/` から見たリポジトリの prelude）。
/// 比べるときは値を見ない（[`normalize_toml`]）。
pub const PRELUDE_PATH: &str = "../../../../../crates/ebykn_prelude";

/// トークン列（空白・コメント・記号の結合を除いた形）。
pub fn token_list(src: &str) -> Result<Vec<String>, String> {
    use proc_macro2::{TokenStream, TokenTree};
    fn flat(ts: TokenStream, out: &mut Vec<String>) {
        for t in ts {
            match t {
                TokenTree::Group(g) => {
                    out.push(format!("{:?}(", g.delimiter()));
                    flat(g.stream(), out);
                    out.push(")".into());
                }
                TokenTree::Punct(p) => out.push(p.as_char().to_string()),
                t => out.push(t.to_string()),
            }
        }
    }
    let ts: TokenStream = src.parse().map_err(|e| format!("{e}"))?;
    let mut out = Vec::new();
    flat(ts, &mut out);
    Ok(out)
}

/// `ebykn_prelude = { path = ".." }` の行の値を除く（置き場所は環境で変わる。`OPEN.md`）。
pub fn normalize_toml(s: &str) -> String {
    s.lines()
        .map(|l| {
            if l.starts_with("ebykn_prelude = ") {
                "ebykn_prelude = <path>".to_string()
            } else {
                l.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// 生成物から作る期待の一式。`(expected/verus/ からの相対パス, 中身)`。`src/` の `.rs` と `Cargo.toml`。
/// `Cargo.lock` は依存の版を固定するもので、生成物ではないので含めない。
pub fn generate(dir: &Path) -> Result<Vec<(String, String)>, String> {
    let name = case_name(dir);
    let report = check_project(dir);
    let output = ebykn::lower_report(&report)
        .ok_or_else(|| format!("{name}: 検査が通らないので書き下せません"))??;
    let mut out: Vec<(String, String)> =
        output.files.into_iter().map(|f| (f.rel, f.text)).collect();
    out.sort();
    let toml = ebykn_lower::cargo_toml(
        &report.project,
        &ebykn_lower::Deps {
            vstd: &ebykn::verus_pin().vstd,
            prelude_path: PRELUDE_PATH,
        },
    );
    out.push(("Cargo.toml".into(), toml));
    Ok(out)
}

fn case_name(dir: &Path) -> String {
    dir.file_name().unwrap().to_string_lossy().into_owned()
}

/// 書き下しを `expected/verus/` と比べる。違えば説明を返す。
pub fn compare_lowered(dir: &Path) -> Result<(), String> {
    let name = case_name(dir);
    let expected = dir.join("expected/verus");
    let generated = generate(dir)?;
    let mut want = expected_rs(&expected)?;
    want.sort();
    let got: Vec<String> = generated
        .iter()
        .map(|(rel, _)| rel.clone())
        .filter(|rel| rel != "Cargo.toml")
        .collect();
    if want != got {
        return Err(format!(
            "{name}: 生成したファイルが違います（期待 {want:?}、実際 {got:?}）"
        ));
    }
    for (rel, text) in &generated {
        let exp =
            std::fs::read_to_string(expected.join(rel)).map_err(|e| format!("{name}: {e}"))?;
        if rel == "Cargo.toml" {
            if normalize_toml(text) != normalize_toml(&exp) {
                return Err(format!(
                    "{name}: Cargo.toml が違います\n--- 期待\n{exp}\n--- 実際\n{text}"
                ));
            }
            continue;
        }
        let a = token_list(&exp).map_err(|e| format!("{name}: {rel}: {e}"))?;
        let b = token_list(text).map_err(|e| format!("{name}: 生成した {rel}: {e}"))?;
        if a != b {
            let at = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
            return Err(format!(
                "{name}: {rel} のトークン列が {at} 個目から違います\n--- 期待: {}\n--- 実際: {}\n--- 生成した全文\n{text}",
                a[at.min(a.len())..]
                    .iter()
                    .take(20)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(" "),
                b[at.min(b.len())..]
                    .iter()
                    .take(20)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(" "),
            ));
        }
    }
    Ok(())
}

/// `expected/verus/src/` の下の `.rs`（`src/..` の形）。ディレクトリがなければ空、読めなければ失敗にする。
fn expected_rs(expected: &Path) -> Result<Vec<String>, String> {
    fn walk(dir: &Path, rel: &str, out: &mut Vec<String>) -> Result<(), String> {
        let rd =
            std::fs::read_dir(dir).map_err(|e| format!("{} を読めません: {e}", dir.display()))?;
        for e in rd {
            let e = e.map_err(|e| format!("{} を読めません: {e}", dir.display()))?;
            let p = e.path();
            let n = e.file_name().to_string_lossy().into_owned();
            if p.is_dir() {
                walk(&p, &format!("{rel}/{n}"), out)?;
            } else if n.ends_with(".rs") {
                out.push(format!("{rel}/{n}"));
            }
        }
        Ok(())
    }
    let src = expected.join("src");
    let mut out = Vec::new();
    if src.is_dir() {
        walk(&src, "src", &mut out)?;
    }
    Ok(out)
}

/// コメントの印の数。`.rs` は `//` と `/*`、`Cargo.toml` は `#`。文字列の中のものも数える
/// （生成物の文字列にあるものは、作り直したあとにも同じ数だけ残るので、消えたことにはならない）。
fn comment_marks(rel: &str, text: &str) -> usize {
    if rel.ends_with(".toml") {
        text.matches('#').count()
    } else {
        text.matches("//").count() + text.matches("/*").count()
    }
}

/// 作り直しの結果。
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Blessed {
    /// 書いたファイル（中身が変わったもの、新しく作ったもの）。
    pub written: Vec<String>,
    /// 生成物にないので消した `.rs`。
    pub removed: Vec<String>,
}

/// `expected/verus/` を生成物から作り直す。`Cargo.lock` には触らない。
///
/// 今の期待に、生成物より多くのコメントの印があるファイル（作り直すと手で書いたコメントが消える）や、
/// 消すことになる `.rs` にコメントがあれば、何も書かずに失敗する。
pub fn bless(dir: &Path) -> Result<Blessed, String> {
    let name = case_name(dir);
    let expected = dir.join("expected/verus");
    let generated = generate(dir)?;
    let mut old = std::collections::BTreeMap::new();
    for rel in expected_rs(&expected)?
        .into_iter()
        .chain(["Cargo.toml".to_string()])
    {
        let p = expected.join(&rel);
        if p.exists() {
            let text = std::fs::read_to_string(&p)
                .map_err(|e| format!("{name}: {} を読めません: {e}", p.display()))?;
            old.insert(rel, text);
        }
    }
    let mut lost = Vec::new();
    for (rel, before) in &old {
        let after = generated
            .iter()
            .find(|(r, _)| r == rel)
            .map_or(0, |(r, t)| comment_marks(r, t));
        if comment_marks(rel, before) > after {
            lost.push(rel.clone());
        }
    }
    if !lost.is_empty() {
        return Err(format!(
            "{name}: 作り直すと expected/verus/ の {} のコメントが消えます。何も書いていません。\
             コメントはケースの README.md に移してから作り直してください",
            lost.join("、")
        ));
    }
    let mut done = Blessed::default();
    for (rel, text) in &generated {
        if old.get(rel) == Some(text) {
            continue;
        }
        let p = expected.join(rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("{name}: {} を作れません: {e}", parent.display()))?;
        }
        std::fs::write(&p, text)
            .map_err(|e| format!("{name}: {} を書けません: {e}", p.display()))?;
        done.written.push(rel.clone());
    }
    for rel in old.keys() {
        if !generated.iter().any(|(r, _)| r == rel) {
            let p = expected.join(rel);
            std::fs::remove_file(&p)
                .map_err(|e| format!("{name}: {} を消せません: {e}", p.display()))?;
            done.removed.push(rel.clone());
        }
    }
    Ok(done)
}
