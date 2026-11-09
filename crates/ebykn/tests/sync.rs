//! `ebykn sync`（仕様 §9）：`src/` の変更を `proof/` の `.eykp` に `git merge-file` で取り込む。
//! 一時ディレクトリにプロジェクトを作って確かめる（`git` が要る）。

use std::path::{Path, PathBuf};
use std::process::Command;

use ebykn::sync::{Outcome, sync};

const MANIFEST: &str = "[package]\nname = \"s\"\nversion = \"0.1.0\"\n";

/// 一時ディレクトリのプロジェクト。落とすときに消す。
struct Proj(PathBuf);

impl Proj {
    fn new(name: &str) -> Proj {
        let dir = std::env::temp_dir().join(format!("ebykn-sync-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("Ebykn.toml"), MANIFEST).unwrap();
        Proj(dir)
    }

    fn write(&self, rel: &str, text: &str) {
        let p = self.0.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    fn read(&self, rel: &str) -> String {
        std::fs::read_to_string(self.0.join(rel)).unwrap()
    }

    fn exists(&self, rel: &str) -> bool {
        self.0.join(rel).exists()
    }

    fn outcomes(&self) -> Vec<(String, Outcome)> {
        sync(&self.0)
            .files
            .into_iter()
            .map(|f| (f.proof, f.outcome))
            .collect()
    }
}

impl Drop for Proj {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const SRC: &str = "pub fn a() -> u32 { 1 }\n\npub fn b() -> u32 { 2 }\n\npub fn c() -> u32 { 3 }\n";
/// `SRC` の写しに証明を足したもの（`c` の前に補題）。
const PROOF: &str = "pub fn a() -> u32 { 1 }\n\npub fn b() -> u32 { 2 }\n\nlemma fn l() ensures true { }\n\npub fn c() -> u32 { 3 }\n";

#[test]
fn first_sync_records_the_base_without_touching_the_proof() {
    let p = Proj::new("first");
    p.write("src/lib.eyk", SRC);
    p.write("proof/lib.eykp", PROOF);
    assert_eq!(
        p.outcomes(),
        [("proof/lib.eykp".to_string(), Outcome::BaseCreated)]
    );
    assert_eq!(p.read(".ebykn/sync/lib.eyk"), SRC);
    assert_eq!(p.read("proof/lib.eykp"), PROOF);
    // 2 回目は変わっていない。
    assert_eq!(
        p.outcomes(),
        [("proof/lib.eykp".to_string(), Outcome::Unchanged)]
    );
}

#[test]
fn source_changes_are_merged_into_the_proof() {
    let p = Proj::new("merge");
    p.write("src/lib.eyk", SRC);
    p.write("proof/lib.eykp", PROOF);
    p.outcomes();
    let new = SRC.replace("{ 1 }", "{ 10 }");
    p.write("src/lib.eyk", &new);
    assert_eq!(
        p.outcomes(),
        [("proof/lib.eykp".to_string(), Outcome::Merged)]
    );
    // src の変更と、証明の追加の両方が残る。
    assert_eq!(p.read("proof/lib.eykp"), PROOF.replace("{ 1 }", "{ 10 }"));
    assert_eq!(p.read(".ebykn/sync/lib.eyk"), new);
    assert_eq!(
        p.outcomes(),
        [("proof/lib.eykp".to_string(), Outcome::Unchanged)]
    );
}

#[test]
fn conflicts_are_left_in_the_file_and_the_base_moves_on() {
    let p = Proj::new("conflict");
    p.write("src/lib.eyk", SRC);
    p.write("proof/lib.eykp", &PROOF.replace("{ 2 }", "{ 2 + 0 }"));
    p.outcomes();
    let new = SRC.replace("{ 2 }", "{ 20 }");
    p.write("src/lib.eyk", &new);
    let r = sync(&p.0);
    assert!(!r.success());
    assert_eq!(r.files[0].outcome, Outcome::Conflict(1));
    let text = p.read("proof/lib.eykp");
    assert!(text.contains("<<<<<<< proof/lib.eykp"), "{text}");
    assert!(
        text.contains("{ 2 + 0 }") && text.contains("{ 20 }"),
        "{text}"
    );
    assert!(text.contains(">>>>>>> src/lib.eyk"), "{text}");
    // 証明の追加は残る。
    assert!(text.contains("lemma fn l()"), "{text}");
    // 基準は今の src に進む（次の sync で同じ変更を二度当てない）。
    assert_eq!(p.read(".ebykn/sync/lib.eyk"), new);
    assert_eq!(
        p.outcomes(),
        [("proof/lib.eykp".to_string(), Outcome::Unchanged)]
    );
}

#[test]
fn a_proof_without_its_source_is_an_error_and_left_alone() {
    let p = Proj::new("missing");
    p.write("src/lib.eyk", SRC);
    p.write("proof/lib.eykp", PROOF);
    p.write("proof/gone.eykp", "fn x() {}\n");
    let r = sync(&p.0);
    assert!(!r.success());
    let got: Vec<(String, Outcome)> = r.files.into_iter().map(|f| (f.proof, f.outcome)).collect();
    assert_eq!(
        got,
        [
            ("proof/gone.eykp".to_string(), Outcome::MissingSource),
            ("proof/lib.eykp".to_string(), Outcome::BaseCreated),
        ]
    );
    assert_eq!(p.read("proof/gone.eykp"), "fn x() {}\n");
    assert!(!p.exists(".ebykn/sync/gone.eyk"));
}

#[test]
fn nested_modules_map_path_by_path() {
    let p = Proj::new("nested");
    p.write("src/lib.eyk", "mod a;\n");
    p.write("src/a/mod.eyk", SRC);
    p.write("proof/a/mod.eykp", PROOF);
    let r = sync(&p.0);
    assert_eq!(r.files.len(), 1);
    let f = &r.files[0];
    assert_eq!(
        (f.proof.as_str(), f.source.as_str(), f.base.as_str()),
        ("proof/a/mod.eykp", "src/a/mod.eyk", ".ebykn/sync/a/mod.eyk")
    );
    assert_eq!(f.outcome, Outcome::BaseCreated);
    assert_eq!(p.read(".ebykn/sync/a/mod.eyk"), SRC);
    // `proof/` にない `src/lib.eyk` には基準を作らない（証明ファイルは無くてよい。仕様 §3.1）。
    assert!(!p.exists(".ebykn/sync/lib.eyk"));
}

#[test]
fn json_lists_every_file_and_the_result() {
    let p = Proj::new("json");
    p.write("src/lib.eyk", SRC);
    p.write("proof/lib.eykp", PROOF);
    p.write("proof/gone.eykp", "");
    let j = sync(&p.0).json();
    assert_eq!(
        j,
        "{\"$message_type\":\"ebykn-sync\",\"success\":false,\"files\":[\
         {\"proof\":\"proof/gone.eykp\",\"source\":\"src/gone.eyk\",\"base\":\".ebykn/sync/gone.eyk\",\"state\":\"missing-source\"},\
         {\"proof\":\"proof/lib.eykp\",\"source\":\"src/lib.eyk\",\"base\":\".ebykn/sync/lib.eyk\",\"state\":\"base-created\"}],\
         \"errors\":[]}"
    );
}

fn ebykn(dir: &Path, args: &[&str]) -> (i32, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_ebykn"))
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// 終了コード（仕様 §9）：衝突なく済めば 0、衝突があれば 1。
#[test]
fn cli_exit_codes() {
    let p = Proj::new("cli");
    p.write("src/lib.eyk", SRC);
    p.write("proof/lib.eykp", &PROOF.replace("{ 2 }", "{ 2 + 0 }"));
    let (code, err) = ebykn(&p.0, &["sync"]);
    assert_eq!(code, 0, "{err}");
    assert!(err.contains("base proof/lib.eykp"), "{err}");
    p.write("src/lib.eyk", &SRC.replace("{ 1 }", "{ 10 }"));
    let (code, err) = ebykn(&p.0, &["sync"]);
    assert_eq!(code, 0, "{err}");
    assert!(err.contains("merged proof/lib.eykp"), "{err}");
    p.write("src/lib.eyk", &SRC.replace("{ 2 }", "{ 20 }"));
    let (code, err) = ebykn(&p.0, &["--lang", "ja", "sync"]);
    assert_eq!(code, 1, "{err}");
    assert!(err.contains("衝突 1 か所をファイルに残しました"), "{err}");
    let (code, _) = ebykn(&p.0, &["sync", "extra"]);
    assert_eq!(code, 2);
}

/// `git` が動かなければ、そのファイルは変えずに失敗にする（黙って取り込んだことにしない）。
#[test]
fn without_git_the_proof_and_base_are_left_alone() {
    let p = Proj::new("nogit");
    p.write("src/lib.eyk", SRC);
    p.write("proof/lib.eykp", PROOF);
    p.outcomes();
    p.write("src/lib.eyk", &SRC.replace("{ 1 }", "{ 10 }"));
    let out = Command::new(env!("CARGO_BIN_EXE_ebykn"))
        .arg("sync")
        .current_dir(&p.0)
        .env("PATH", "")
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{err}");
    assert!(err.contains("cannot run `git merge-file`"), "{err}");
    assert_eq!(p.read("proof/lib.eykp"), PROOF);
    assert_eq!(p.read(".ebykn/sync/lib.eyk"), SRC);
}
