//! `cargo xtask slides-selftest`：`slides-build` の検査が、本物のテンプレートと見本で失敗できることを確かめる。
//!
//! `slides.rs` の単体テストは、検査の記録の文字列を壊して `judge` を確かめるだけで、テンプレートの側
//! （記録を残す Typst のコード）を通らない。テンプレートの数え方が壊れても、単体テストは緑のまま残る。
//! そこで、スキルの見本（`example/slides.typ`）を 1 か所ずつ壊して実際に組み、それぞれが決まった文言で
//! 失敗することを確かめる。固定した Typst とフォントが要るので（`slides-install`）、CI の slides ジョブで走らせる。

use std::path::Path;

/// 見本の置き場（リポジトリの直下から）。
pub const EXAMPLE: &str = ".claude/skills/ebykn-report-slides/example/slides.typ";
/// 壊した見本を組む置き場。Typst の root（リポジトリの直下）の中に置く。
const WORK: &str = "target/reports/selftest";

/// 壊し方：名前、置き換える文字列、置き換えた後の文字列、失敗の文言に含まれるべき語。
pub const MUTATIONS: &[(&str, &str, &str, &str)] = &[
    (
        "bullets",
        "  - 権限を二重に解放するプログラムを",
        "  - 一\n  - 二\n  - 三\n  - 四\n  - 五\n  - 権限を二重に解放するプログラムを",
        "箇条書きが 6 項目",
    ),
    // 番号付きの並び（`+`）も数える（テンプレートの `show enum`）。
    (
        "numbered",
        "  - 権限を二重に解放するプログラムを",
        "  + 一\n  + 二\n  + 三\n  + 四\n  + 五\n  + 権限を二重に解放するプログラムを",
        "箇条書きが 6 項目",
    ),
    (
        "long-boxes",
        "ebykn: 検査が 2 件のエラーで失敗しました\n```)\n]",
        "ebykn: 検査が 2 件のエラーで失敗しました\n```)\n  #code-box(```text\n1\n2\n3\n4\n5\n6\n7\n```)\n  #code-box(```text\n1\n2\n3\n4\n5\n6\n7\n```)\n]",
        "6 行を超えるコードの箱が 2 つ",
    ),
    (
        "code-in-cols",
        "  #code-box(title: [`src/lib.eyk`（一部）], ```eyk",
        "  #cols([説明], code-box(title: [`src/lib.eyk`（一部）], ```eyk",
        "段組み",
    ),
    // 上の壊し方で開いた括弧を閉じる。2 つで 1 組の壊し方にする（`code-in-cols` と同じ名前）。
    (
        "code-in-cols",
        "    let b = p.free();\n}\n```)\n]",
        "    let b = p.free();\n}\n```))\n]",
        "段組み",
    ),
    (
        "overflow",
        "  二重の解放は E0504",
        "  二重の解放は E0504\n  #v(12cm)",
        "はみ出しています",
    ),
    (
        "wrap",
        "ebykn: 検査が 2 件のエラーで失敗しました",
        "ebykn: 検査が 2 件のエラーで失敗しました。この行は箱の幅に収まらないほど長く書き足して、折り返しの検査が失敗することを確かめるためのものです",
        "折り返されています",
    ),
    (
        "codename",
        "heading: [権限の検査], caption: [実例の入力]",
        "heading: [M0：権限の検査], caption: [実例の入力]",
        "コードネーム M0",
    ),
    (
        "raw-link",
        "#commit(\"f9cf01f\")",
        "#link(\"https://github.com/MatchaChoco010/ebykn-lang-proto/commit/f9cf01f\")[f9cf01f]",
        "PDF の中のリンク",
    ),
    (
        "warning",
        "date: \"2026-11-09\")\n",
        "date: \"2026-11-09\")\n#set text(font: \"Arial\")\n",
        "警告",
    ),
    (
        "per-page",
        "#overview((",
        "#overview(per-page: 20, (",
        "per-page",
    ),
    (
        "package",
        "#import \"/.claude/",
        "#import \"@preview/cetz:0.3.0\"\n#import \"/.claude/",
        "パッケージは使えません",
    ),
];

/// `from` を `to` に置き換える。当たらなければ（見本が変わって壊し方が古くなった）失敗にする。
pub fn mutate(src: &str, from: &str, to: &str) -> Result<String, String> {
    if !src.contains(from) {
        return Err(format!("壊す操作が見本に当たりません: {from:?}"));
    }
    Ok(src.replacen(from, to, 1))
}

/// 名前ごとに壊し方をまとめて当てた見本を返す。
pub fn mutated(src: &str, name: &str) -> Result<(String, &'static str), String> {
    let mut out = src.to_string();
    let mut needle = "";
    for (n, from, to, expect) in MUTATIONS.iter().filter(|m| m.0 == name) {
        out = mutate(&out, from, to).map_err(|e| format!("{n}: {e}"))?;
        needle = expect;
    }
    Ok((out, needle))
}

fn names() -> Vec<&'static str> {
    let mut v: Vec<&str> = MUTATIONS.iter().map(|m| m.0).collect();
    v.dedup();
    v
}

fn build_one(root: &Path, name: &str, text: &str) -> Result<(), Vec<String>> {
    let dir = root.join(WORK).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| vec![format!("{}: {e}", dir.display())])?;
    std::fs::write(dir.join("slides.typ"), text).map_err(|e| vec![format!("{e}")])?;
    super::try_build(root, &dir).map(|_| ())
}

pub fn run(root: &Path) -> bool {
    let src = match std::fs::read_to_string(root.join(EXAMPLE)) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: slides-selftest: {EXAMPLE}: {e}");
            return false;
        }
    };
    let mut ok = true;
    // 壊していない見本は通る（通らなければ、下の失敗は何も示さない）。
    match build_one(root, "original", &src) {
        Ok(()) => eprintln!("ok: slides-selftest: 壊していない見本は通る"),
        Err(errors) => {
            eprintln!("error: slides-selftest: 壊していない見本が通りません: {errors:?}");
            return false;
        }
    }
    for name in names() {
        let (text, needle) = match mutated(&src, name) {
            Ok(x) => x,
            Err(e) => {
                eprintln!("error: slides-selftest: {e}");
                ok = false;
                continue;
            }
        };
        match build_one(root, name, &text) {
            Ok(()) => {
                eprintln!("error: slides-selftest: {name}：壊した見本が通ってしまいました");
                ok = false;
            }
            Err(errors) if errors.iter().any(|e| e.contains(needle)) => {
                eprintln!("ok: slides-selftest: {name}：「{needle}」で失敗した");
            }
            Err(errors) => {
                eprintln!(
                    "error: slides-selftest: {name}：失敗したが「{needle}」の文言がありません: {errors:?}"
                );
                ok = false;
            }
        }
    }
    ok
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::real;

    #[test]
    fn fails_unless_every_mutation_hits_the_example() {
        // 見本が変わって壊し方が当たらなくなると、selftest は何も確かめずに失敗する。単体テストで先に捕まえる。
        let src = real(EXAMPLE);
        for name in names() {
            let (out, needle) = mutated(&src, name).unwrap_or_else(|e| panic!("{e}"));
            assert_ne!(out, src, "{name}");
            assert!(!needle.is_empty(), "{name}");
        }
        assert!(mutate("abc", "x", "y").is_err());
    }
}
