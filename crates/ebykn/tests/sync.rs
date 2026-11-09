//! `ebykn sync`（仕様 §9）：`src/` の変更を `proof/` の `.eykp` に `git merge-file` で取り込む。
//! 一時ディレクトリにプロジェクトを作って確かめる（`git` が要る）。

use std::path::{Path, PathBuf};
use std::process::Command;

use ebykn::sync::{Outcome, Reason, merge_conflicts, sync, write_atomic};

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
    // 基準は今の src に進む（手で解いた hunk が次の sync でまた衝突しないように）。
    assert_eq!(p.read(".ebykn/sync/lib.eyk"), new);
    // 印が残っている間は、src が変わっていなくても失敗にし、何も変えない（#93 のレビュー）。
    assert_eq!(
        p.outcomes(),
        [("proof/lib.eykp".to_string(), Outcome::UnresolvedMarkers)]
    );
    p.write("src/lib.eyk", &new.replace("{ 1 }", "{ 100 }"));
    assert_eq!(
        p.outcomes(),
        [("proof/lib.eykp".to_string(), Outcome::UnresolvedMarkers)]
    );
    assert_eq!(p.read("proof/lib.eykp"), text);
    assert_eq!(p.read(".ebykn/sync/lib.eyk"), new);
    // 印を解けば、続きを取り込む。
    let resolved = PROOF.replace("{ 2 }", "{ 20 }");
    p.write("proof/lib.eykp", &resolved);
    assert_eq!(
        p.outcomes(),
        [("proof/lib.eykp".to_string(), Outcome::Merged)]
    );
    assert_eq!(
        p.read("proof/lib.eykp"),
        resolved.replace("{ 1 }", "{ 100 }")
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
         \"orphan_bases\":[],\"errors\":[]}"
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

/// 中身が二進（NUL を含む）の `.eykp` では `git merge-file` が 255 で終わる。衝突の数として読まず、
/// `.eykp` と基準をバイト単位で前のまま残す（#93 のレビュー）。
#[test]
fn binary_proof_fails_and_leaves_both_files_byte_for_byte() {
    let p = Proj::new("binary");
    p.write("src/lib.eyk", SRC);
    let proof = format!("{PROOF}\0\n");
    p.write("proof/lib.eykp", &proof);
    p.outcomes();
    p.write("src/lib.eyk", &SRC.replace("{ 1 }", "{ 10 }"));
    let r = sync(&p.0);
    assert!(!r.success());
    assert!(
        matches!(&r.files[0].outcome, Outcome::Failed(Reason::Git { .. })),
        "{:?}",
        r.files[0].outcome
    );
    assert_eq!(
        std::fs::read(p.0.join("proof/lib.eykp")).unwrap(),
        proof.as_bytes()
    );
    assert_eq!(p.read(".ebykn/sync/lib.eyk"), SRC);
}

#[test]
fn merge_exit_codes() {
    assert_eq!(merge_conflicts(Some(0)), Some(0));
    assert_eq!(merge_conflicts(Some(3)), Some(3));
    assert_eq!(merge_conflicts(Some(127)), Some(127));
    assert_eq!(merge_conflicts(Some(255)), None);
    assert_eq!(merge_conflicts(Some(-1)), None);
    assert_eq!(merge_conflicts(None), None);
}

/// 書き込みは一時ファイルに書いてから名前を変える。一時ファイルを作れなければ、元のファイルは
/// 前のまま残る（#93 のレビュー：`std::fs::write` は先にファイルを空にする）。
#[test]
fn atomic_write_keeps_the_old_content_on_failure() {
    let p = Proj::new("atomic");
    p.write("proof/lib.eykp", PROOF);
    let target = p.0.join("proof/lib.eykp");
    write_atomic(&target, b"new").unwrap();
    assert_eq!(p.read("proof/lib.eykp"), "new");
    assert!(!p.exists("proof/.lib.eykp.ebykn-sync-tmp"));
    // 一時ファイルの名前にディレクトリを置いて、書き込みを失敗させる。
    std::fs::create_dir(p.0.join("proof/.lib.eykp.ebykn-sync-tmp")).unwrap();
    assert!(write_atomic(&target, b"newer").is_err());
    assert_eq!(p.read("proof/lib.eykp"), "new");
}

/// 対応する `.eykp` のない基準は失敗として報告し、基準のない `.eykp` に今の src を基準として書かない
/// （名前を変えたモジュールの変更を落とさない。#93 のレビュー）。
#[test]
fn orphan_bases_are_reported_and_hold_new_bases() {
    let p = Proj::new("orphan");
    p.write("src/foo.eyk", SRC);
    p.write("proof/foo.eykp", PROOF);
    p.outcomes();
    // `git mv src/foo.eyk src/bar.eyk`、`git mv proof/foo.eykp proof/bar.eykp` をして編集した。
    std::fs::remove_file(p.0.join("src/foo.eyk")).unwrap();
    std::fs::remove_file(p.0.join("proof/foo.eykp")).unwrap();
    p.write("src/bar.eyk", &SRC.replace("{ 1 }", "{ 10 }"));
    p.write("proof/bar.eykp", PROOF);
    let r = sync(&p.0);
    assert!(!r.success());
    assert_eq!(r.orphan_bases, [".ebykn/sync/foo.eyk"]);
    assert_eq!(r.files[0].outcome, Outcome::BaseHeld);
    assert!(!p.exists(".ebykn/sync/bar.eyk"));
    // 古い基準を新しい名前に移せば、名前を変えたあとの変更を取り込む。
    std::fs::rename(
        p.0.join(".ebykn/sync/foo.eyk"),
        p.0.join(".ebykn/sync/bar.eyk"),
    )
    .unwrap();
    let r = sync(&p.0);
    assert!(r.success(), "{:?}", r.lines(ebykn_syntax::diag::Lang::En));
    assert_eq!(r.files[0].outcome, Outcome::Merged);
    assert_eq!(p.read("proof/bar.eykp"), PROOF.replace("{ 1 }", "{ 10 }"));
}

/// 失敗の理由も `--lang ja` で日本語にする（#93 のレビュー）。
#[test]
fn reasons_are_translated() {
    let p = Proj::new("ja");
    p.write("src/lib.eyk", SRC);
    p.write("proof/lib.eykp", &format!("{PROOF}\0\n"));
    p.outcomes();
    p.write("src/lib.eyk", &SRC.replace("{ 1 }", "{ 10 }"));
    let ja = sync(&p.0).lines(ebykn_syntax::diag::Lang::Ja).join("\n");
    assert!(ja.contains("`git merge-file` が失敗しました"), "{ja}");
    assert!(ja.contains("は変えていません"), "{ja}");
}

fn git(dir: &Path, args: &[&str]) {
    let st = Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        st.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&st.stderr)
    );
}

/// 取りやめの案内（`git checkout`）は、`.eykp` と基準の両方がコミットされたまま変更がないときだけ出す。
/// コミットしていない証明の作業があれば、戻すと消えることを伝える（#93 の再確認 SF1）。
#[test]
fn undo_hint_only_when_both_files_are_committed_as_is() {
    let p = Proj::new("undo");
    p.write("src/lib.eyk", SRC);
    p.write("proof/lib.eykp", &PROOF.replace("{ 2 }", "{ 2 + 0 }"));
    p.outcomes();
    git(&p.0, &["init", "-q"]);
    git(&p.0, &["add", "."]);
    git(&p.0, &["commit", "-q", "-m", "c"]);
    p.write("src/lib.eyk", &SRC.replace("{ 2 }", "{ 20 }"));
    let r = sync(&p.0);
    assert!(r.files[0].undoable);
    let en = r.lines(ebykn_syntax::diag::Lang::En).join("\n");
    let cmd = format!(
        "git -C {} checkout -- proof/lib.eykp .ebykn/sync/lib.eyk",
        p.0.display()
    );
    assert!(en.contains(&cmd), "{en}");
    // 案内どおりに戻すと、sync の前に戻る。
    let out = Command::new("sh").arg("-c").arg(&cmd).output().unwrap();
    assert!(out.status.success());
    assert_eq!(
        p.read("proof/lib.eykp"),
        PROOF.replace("{ 2 }", "{ 2 + 0 }")
    );
    assert_eq!(p.read(".ebykn/sync/lib.eyk"), SRC);

    // コミットしていない補題があるときは、取りやめの案内を出さない。
    let with_lemma = PROOF.replace("{ 2 }", "{ 2 + 0 }") + "lemma fn extra() ensures true { }\n";
    p.write("proof/lib.eykp", &with_lemma);
    let r = sync(&p.0);
    assert!(!r.files[0].undoable);
    let en = r.lines(ebykn_syntax::diag::Lang::En).join("\n");
    assert!(!en.contains("checkout --"), "{en}");
    assert!(en.contains("would lose uncommitted changes"), "{en}");
}

/// `sync()` が `.eykp` と基準を書く所でも一時ファイルから置き換えることを、書き込みを失敗させて
/// 確かめる（#93 の再確認 SF2）。
#[test]
fn sync_writes_through_the_atomic_path() {
    // `.eykp` を書けない：`.eykp` も基準も前のまま。
    let p = Proj::new("atomic-proof");
    p.write("src/lib.eyk", SRC);
    p.write("proof/lib.eykp", PROOF);
    p.outcomes();
    p.write("src/lib.eyk", &SRC.replace("{ 1 }", "{ 10 }"));
    std::fs::create_dir(p.0.join("proof/.lib.eykp.ebykn-sync-tmp")).unwrap();
    let r = sync(&p.0);
    assert!(
        matches!(&r.files[0].outcome, Outcome::Failed(Reason::Write { path, .. }) if path == "proof/lib.eykp"),
        "{:?}",
        r.files[0].outcome
    );
    assert_eq!(p.read("proof/lib.eykp"), PROOF);
    assert_eq!(p.read(".ebykn/sync/lib.eyk"), SRC);

    // 基準を書けない：基準は前のまま、`.eykp` は取り込んだ中身。
    let p = Proj::new("atomic-base");
    p.write("src/lib.eyk", SRC);
    p.write("proof/lib.eykp", PROOF);
    p.outcomes();
    let new = SRC.replace("{ 1 }", "{ 10 }");
    p.write("src/lib.eyk", &new);
    std::fs::create_dir(p.0.join(".ebykn/sync/.lib.eyk.ebykn-sync-tmp")).unwrap();
    let r = sync(&p.0);
    assert!(
        matches!(
            &r.files[0].outcome,
            Outcome::BaseNotUpdated { conflicts: 0, .. }
        ),
        "{:?}",
        r.files[0].outcome
    );
    assert_eq!(p.read(".ebykn/sync/lib.eyk"), SRC);
    assert_eq!(p.read("proof/lib.eykp"), PROOF.replace("{ 1 }", "{ 10 }"));
}

#[cfg(unix)]
#[test]
fn mode_symlink_and_read_only_files() {
    use std::os::unix::fs::PermissionsExt as _;
    let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    // mode を引き継ぐ。
    let p = Proj::new("mode");
    p.write("src/lib.eyk", SRC);
    p.write("proof/lib.eykp", PROOF);
    p.outcomes();
    let proof = p.0.join("proof/lib.eykp");
    std::fs::set_permissions(&proof, std::fs::Permissions::from_mode(0o600)).unwrap();
    p.write("src/lib.eyk", &SRC.replace("{ 1 }", "{ 10 }"));
    assert_eq!(p.outcomes()[0].1, Outcome::Merged);
    assert_eq!(mode(&proof), 0o600);

    // 読み取り専用の `.eykp` は置き換えない。
    std::fs::set_permissions(&proof, std::fs::Permissions::from_mode(0o444)).unwrap();
    let before = p.read("proof/lib.eykp");
    p.write("src/lib.eyk", &SRC.replace("{ 1 }", "{ 11 }"));
    assert!(matches!(
        p.outcomes()[0].1,
        Outcome::Failed(Reason::Write { .. })
    ));
    assert_eq!(p.read("proof/lib.eykp"), before);
    std::fs::set_permissions(&proof, std::fs::Permissions::from_mode(0o644)).unwrap();

    // symlink の `.eykp` は、指す先を更新し、symlink のまま残す。
    let p = Proj::new("symlink");
    p.write("src/lib.eyk", SRC);
    p.write("real/lib.eykp", PROOF);
    std::fs::create_dir_all(p.0.join("proof")).unwrap();
    std::os::unix::fs::symlink(p.0.join("real/lib.eykp"), p.0.join("proof/lib.eykp")).unwrap();
    p.outcomes();
    p.write("src/lib.eyk", &SRC.replace("{ 1 }", "{ 10 }"));
    assert_eq!(p.outcomes()[0].1, Outcome::Merged);
    assert!(
        std::fs::symlink_metadata(p.0.join("proof/lib.eykp"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(p.read("real/lib.eykp"), PROOF.replace("{ 1 }", "{ 10 }"));
}

/// 衝突を数えたのに印のない出力は信じない（本物の git ではならないが、安全側に倒す。#93 の再確認）。
#[cfg(unix)]
#[test]
fn conflicts_without_markers_are_not_trusted() {
    use std::os::unix::fs::PermissionsExt as _;
    let p = Proj::new("fakegit");
    p.write("src/lib.eyk", SRC);
    p.write("proof/lib.eykp", PROOF);
    p.outcomes();
    p.write("src/lib.eyk", &SRC.replace("{ 1 }", "{ 10 }"));
    let bin = p.0.join("fakebin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(bin.join("git"), "#!/bin/sh\nexit 1\n").unwrap();
    std::fs::set_permissions(bin.join("git"), std::fs::Permissions::from_mode(0o755)).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_ebykn"))
        .arg("sync")
        .current_dir(&p.0)
        .env("PATH", &bin)
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{err}");
    assert!(err.contains("wrote no conflict markers"), "{err}");
    assert_eq!(p.read("proof/lib.eykp"), PROOF);
    assert_eq!(p.read(".ebykn/sync/lib.eyk"), SRC);
}
