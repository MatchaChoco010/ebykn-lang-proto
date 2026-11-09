//! ゲートのテスト用：一時ディレクトリに入力を作り、実リポジトリのファイルを壊して置く。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::gates::{Repo, Report};

pub struct TempRepo {
    root: PathBuf,
    files: Vec<String>,
}

impl TempRepo {
    pub fn new() -> Self {
        static N: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "ebykn-xtask-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        TempRepo {
            root,
            files: Vec::new(),
        }
    }

    pub fn write(&mut self, rel: &str, content: &str) -> &mut Self {
        let path = self.root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
        self.files.push(rel.to_string());
        self
    }

    pub fn run(&self, gate: fn(&Repo) -> Report) -> Report {
        self.run_ref(gate)
    }

    /// 閉包でゲートを走らせる（偽の外部コマンドを渡すときなど）。
    pub fn run_ref(&self, gate: impl FnOnce(&Repo) -> Report) -> Report {
        gate(&Repo {
            root: self.root.clone(),
            files: self.files.clone(),
        })
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// 実リポジトリのファイルを読む。
pub fn real(rel: &str) -> String {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::fs::read_to_string(root.join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

/// `from` を `to` に置き換える。壊す操作が当たったこと（`from` があり、結果が変わったこと）を先に確かめる。
pub fn mutate(src: &str, from: &str, to: &str) -> String {
    assert!(src.contains(from), "壊す操作が当たらない: {from:?}");
    let out = src.replacen(from, to, 1);
    assert_ne!(out, src, "壊す操作で内容が変わらない");
    out
}

/// ゲートが失敗し、`needle` を含むエラーを出したことを確かめる。
pub fn assert_fails(report: &Report, needle: &str) {
    assert!(
        report.errors.iter().any(|e| e.contains(needle)),
        "{needle:?} を含むエラーがない: {:?}",
        report.errors
    );
}

pub fn assert_passes(report: &Report) {
    assert!(report.errors.is_empty(), "{:?}", report.errors);
}

/// ソースのうちテストのモジュール（`#[cfg(test)]` の直後の `mod tests`）より前の部分。
/// `#[cfg(test)] mod testutil;` のような宣言では切らない。`mod tests` がなければ全体を返す。
pub fn without_tests(src: &str) -> &str {
    &src[..src.find("#[cfg(test)]\nmod tests").unwrap_or(src.len())]
}
