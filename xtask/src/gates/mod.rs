//! ゲート。各モジュールは `run(&Repo) -> Report` を持ち、入力を壊すと失敗することを
//! `fails_` で始まるテストで示す（`check-gate-tests` が検査する）。

pub mod book;
pub mod file_size;
pub mod gate_tests;
pub mod md_links;
pub mod verus_pin;
pub mod verus_probes;

use std::collections::HashSet;
use std::path::PathBuf;

pub struct Gate {
    pub name: &'static str,
    pub run: fn(&Repo) -> Report,
}

pub const ALL: &[Gate] = &[
    Gate {
        name: "check-file-size",
        run: file_size::run,
    },
    Gate {
        name: "check-md-links",
        run: md_links::run,
    },
    Gate {
        name: "check-book",
        run: book::run,
    },
    Gate {
        name: "check-gate-tests",
        run: gate_tests::run,
    },
    Gate {
        name: "check-verus-pin",
        run: verus_pin::run,
    },
];

/// 固定した Verus が要るゲート。`gates` には入れず、CI の Verus のジョブで走らせる。
pub const VERUS: &[Gate] = &[Gate {
    name: "verus-probes",
    run: verus_probes::run,
}];

/// 検査の対象。`files` はリポジトリ直下からの相対パス（区切りは `/`）。
pub struct Repo {
    pub root: PathBuf,
    pub files: Vec<String>,
}

impl Repo {
    pub fn read_bytes(&self, rel: &str) -> Result<Vec<u8>, String> {
        std::fs::read(self.root.join(rel)).map_err(|e| format!("{rel}: 読めません: {e}"))
    }

    pub fn read(&self, rel: &str) -> Result<String, String> {
        String::from_utf8(self.read_bytes(rel)?).map_err(|_| format!("{rel}: UTF-8 ではありません"))
    }

    pub fn has(&self, rel: &str) -> bool {
        self.files.iter().any(|f| f == rel)
    }

    /// ファイルと、ファイルを含むディレクトリの集合。
    pub fn paths(&self) -> HashSet<String> {
        let mut set = HashSet::new();
        for f in &self.files {
            set.insert(f.clone());
            let mut p = f.as_str();
            while let Some(i) = p.rfind('/') {
                p = &p[..i];
                set.insert(p.to_string());
            }
        }
        set
    }
}

#[derive(Default)]
pub struct Report {
    pub errors: Vec<String>,
    /// 通ったときにも出す情報（対象外にしたもの、実行していないものなど）。
    pub notes: Vec<String>,
}

/// コードフェンスの開き（```` ``` ```` または `~~~`）。フェンス文字・長さ・情報文字列を返す。
pub fn fence_open(line: &str) -> Option<(char, usize, &str)> {
    let t = line.trim_start();
    if line.len() - t.len() > 3 {
        return None;
    }
    let c = t.chars().next()?;
    if c != '`' && c != '~' {
        return None;
    }
    let n = t.chars().take_while(|&x| x == c).count();
    (n >= 3).then(|| (c, n, t[n..].trim()))
}

/// `open` で開いたフェンスを閉じる行か。
pub fn fence_close(line: &str, c: char, n: usize) -> bool {
    let t = line.trim();
    t.chars().take_while(|&x| x == c).count() >= n && t.chars().all(|x| x == c)
}
