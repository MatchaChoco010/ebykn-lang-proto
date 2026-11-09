//! 外部からの取得（`verus-install` と `slides-install` が使う）と、取得したものの sha256 の照合。
//!
//! 一時的なエラー（HTTP の 5xx・408・429、接続の失敗）だけを、回数を決めて再試行する。
//! それ以外（404 などの 4xx、書き込みの失敗）は再試行せずに失敗にする。
//! 取れなかったときは、何を・どこから取れなかったかを示して失敗する。
//!
//! CI の mdBook の取得（`.github/workflows/ci.yml` の `book`）は xtask を使わず curl の `--retry` で再試行する。
//! 時間の上限（`CURL_ARGS`）はそちらと同じにする（`fails_without_time_limits_on_curl` が確かめる）。

use std::path::Path;
use std::process::Command;
use std::time::Duration;

/// 試みる回数（最初の 1 回を含む）。
pub const ATTEMPTS: u32 = 5;
/// 再試行の前に待つ時間。
pub const DELAY: Duration = Duration::from_secs(10);

/// 1 回の curl の結果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Outcome {
    /// curl の終了コード。シグナルで止まったときは `None`。
    pub exit: Option<i32>,
    /// 最後の応答の HTTP ステータス。応答がなければ 0（curl の `%{http_code}` が `000`）。
    pub http: u16,
}

/// 1 回の結果を受けて次に何をするか。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Next {
    Done,
    Retry(String),
    Fail(String),
}

/// 一時的なエラーとみなす curl の終了コード：名前解決（5・6）、接続（7）、途中で切れた転送（18）、
/// 時間切れ（28）、TLS の接続（35）、空の応答（52）、送受信の失敗（55・56）、HTTP/2 の失敗（16・92）。
const TRANSIENT_EXIT: &[i32] = &[5, 6, 7, 16, 18, 28, 35, 52, 55, 56, 92];

/// `attempt` 回目（1 から数える）の結果から次を決める。
pub fn next(outcome: Outcome, attempt: u32) -> Next {
    let transient = match outcome {
        Outcome {
            exit: Some(0),
            http: 200..=299,
        } => return Next::Done,
        Outcome {
            exit: Some(0),
            http: http @ (408 | 429 | 500..=599),
        } => format!("HTTP {http}"),
        Outcome {
            exit: Some(0),
            http,
        } => return Next::Fail(format!("HTTP {http}（再試行しません）")),
        Outcome {
            exit: Some(code), ..
        } if TRANSIENT_EXIT.contains(&code) => format!("curl の終了コード {code}"),
        Outcome {
            exit: Some(code), ..
        } => {
            return Next::Fail(format!("curl の終了コード {code}（再試行しません）"));
        }
        Outcome { exit: None, .. } => {
            return Next::Fail("curl がシグナルで止まりました".to_string());
        }
    };
    if attempt >= ATTEMPTS {
        Next::Fail(format!(
            "{ATTEMPTS} 回試みて取れません（最後は {transient}）"
        ))
    } else {
        Next::Retry(transient)
    }
}

/// `attempt` を最大 `ATTEMPTS` 回呼び、`next` に従って再試行する。再試行の前に `wait(DELAY)` を呼ぶ。
fn retry(
    mut attempt: impl FnMut() -> Result<Outcome, String>,
    mut wait: impl FnMut(Duration),
) -> Result<(), String> {
    for n in 1.. {
        match next(attempt()?, n) {
            Next::Done => return Ok(()),
            Next::Fail(e) => return Err(e),
            Next::Retry(why) => {
                eprintln!(
                    "warning: {why}。{} 秒後に再試行します（{}/{ATTEMPTS}）",
                    DELAY.as_secs(),
                    n + 1
                );
                wait(DELAY);
            }
        }
    }
    unreachable!("next は ATTEMPTS 回目までに Done か Fail を返す")
}

/// `url` を `dest` に取る。`what` は失敗の報告に使う名前。
pub fn download(what: &str, url: &str, dest: &Path) -> Result<(), String> {
    retry(|| curl_once(url, dest), std::thread::sleep)
        .map_err(|e| format!("{what} を取得できません（{url}）: {e}"))
}

/// curl に渡す引数（取得先と保存先の前まで）。CI の mdBook の取得（`.github/workflows/ci.yml`）も同じ時間の上限を使う。
///
/// - `-f` は付けない（付けると HTTP のステータスが終了コード 22 にまとまり、4xx と 5xx を分けられない）。
/// - `--connect-timeout 30`：接続できないまま待ち続けない。
/// - `--speed-limit 1024 --speed-time 60`：60 秒のあいだ毎秒 1024 バイトを下回ったら止める（終了コード 28 になり、再試行する）。
///   これがないと、転送の途中で止まった接続をいつまでも待つ。
const CURL_ARGS: &[&str] = &[
    "-sSL",
    "--connect-timeout",
    "30",
    "--speed-limit",
    "1024",
    "--speed-time",
    "60",
    "-w",
    "%{http_code}",
    "-o",
];

/// curl の呼び出しはここでだけ作る（再試行の判定を通らない取得を作らない。テストが数える）。
fn curl_once(url: &str, dest: &Path) -> Result<Outcome, String> {
    let mut cmd = Command::new("curl");
    cmd.args(CURL_ARGS).arg(dest).arg(url);
    eprintln!("$ {cmd:?}");
    let out = cmd
        .output()
        .map_err(|e| format!("curl を起動できません: {e}"))?;
    eprint!("{}", String::from_utf8_lossy(&out.stderr));
    outcome(out.status.code(), &String::from_utf8_lossy(&out.stdout))
}

/// curl の終了コードと `%{http_code}` の出力から `Outcome` を作る。
///
/// 終了コードを先に見る。curl は失敗の種類によって `%{http_code}` に何も出さない（終了コード 3 など）ので、
/// 失敗したときに出力が読めなくても、応答がなかった（0）として終了コードで判定する。
/// 成功したのに出力が読めないときは、読めない入力として失敗にする。
fn outcome(exit: Option<i32>, stdout: &str) -> Result<Outcome, String> {
    let http = match (exit, stdout.trim().parse()) {
        (_, Ok(http)) => http,
        (Some(0), Err(_)) => {
            return Err(format!("curl の %{{http_code}} を読めません: {stdout:?}"));
        }
        (_, Err(_)) => 0,
    };
    Ok(Outcome { exit, http })
}

/// ファイルの sha256（`sha256sum` で求める）。
pub fn sha256_of(file: &Path) -> Result<String, String> {
    let out = Command::new("sha256sum")
        .arg(file)
        .output()
        .map_err(|e| format!("sha256sum を起動できません: {e}"))?;
    if !out.status.success() {
        return Err(format!("sha256sum が失敗しました: {}", file.display()));
    }
    String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .next()
        .map(str::to_string)
        .ok_or_else(|| "sha256sum の出力を読めません".to_string())
}

/// 取得したものの sha256 が固定値と同じか。
pub fn check_sha256(what: &str, got: &str, want: &str) -> Result<(), String> {
    if got == want {
        Ok(())
    } else {
        Err(format!(
            "{what} の sha256 が固定値と違います（期待 {want}、実際 {got}）"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(http: u16) -> Outcome {
        Outcome {
            exit: Some(0),
            http,
        }
    }

    fn exit(code: i32) -> Outcome {
        Outcome {
            exit: Some(code),
            http: 0,
        }
    }

    /// 決まった結果を順に返す偽の curl で `retry` を走らせ、呼ばれた回数と待った回数を返す。
    fn run(outcomes: &[Outcome]) -> (Result<(), String>, usize, usize) {
        let mut calls = 0;
        let mut waits = 0;
        let r = retry(
            || {
                calls += 1;
                Ok(outcomes[calls - 1])
            },
            |d| {
                assert_eq!(d, DELAY);
                waits += 1;
            },
        );
        (r, calls, waits)
    }

    #[test]
    fn retries_server_errors_and_connection_failures() {
        for o in [
            ok(500),
            ok(502),
            ok(503),
            ok(504),
            ok(408),
            ok(429),
            exit(6),
            exit(7),
            exit(28),
            exit(56),
        ] {
            assert!(matches!(next(o, 1), Next::Retry(_)), "{o:?}");
        }
        assert_eq!(next(ok(200), 1), Next::Done);
    }

    #[test]
    fn fails_unless_a_cut_transfer_is_retried() {
        // 途中で切れた転送は、HTTP が 200 でも終了コードが 0 ではない（手元のサーバーで exit 18 と 200）。
        for code in [18, 56] {
            let o = Outcome {
                exit: Some(code),
                http: 200,
            };
            assert!(matches!(next(o, 1), Next::Retry(_)), "{o:?}");
            assert!(matches!(next(o, ATTEMPTS), Next::Fail(_)), "{o:?}");
        }
    }

    #[test]
    fn succeeds_after_a_transient_error() {
        let (r, calls, waits) = run(&[ok(504), exit(7), ok(200)]);
        assert_eq!(r, Ok(()));
        assert_eq!((calls, waits), (3, 2));
    }

    #[test]
    fn fails_without_retrying_a_client_error() {
        for http in [400, 403, 404, 410] {
            let Next::Fail(e) = next(ok(http), 1) else {
                panic!("HTTP {http} を再試行した");
            };
            assert!(e.contains(&format!("HTTP {http}")), "{e}");
        }
        let (r, calls, waits) = run(&[ok(404), ok(200)]);
        assert!(r.unwrap_err().contains("HTTP 404"));
        assert_eq!((calls, waits), (1, 0));
    }

    #[test]
    fn fails_without_retrying_other_curl_errors() {
        // 23：書き込みの失敗。3：URL の形の誤り。どちらも繰り返しても直らない。
        for code in [3, 23] {
            assert!(matches!(next(exit(code), 1), Next::Fail(_)), "{code}");
        }
        assert!(matches!(
            next(
                Outcome {
                    exit: None,
                    http: 0
                },
                1
            ),
            Next::Fail(_)
        ));
    }

    #[test]
    fn fails_when_the_attempts_are_used_up() {
        assert!(matches!(next(ok(504), ATTEMPTS - 1), Next::Retry(_)));
        let Next::Fail(e) = next(ok(504), ATTEMPTS) else {
            panic!("{ATTEMPTS} 回目のあとも再試行した");
        };
        assert!(e.contains("HTTP 504"), "{e}");
        let all = vec![ok(504); ATTEMPTS as usize];
        let (r, calls, waits) = run(&all);
        assert!(r.unwrap_err().contains(&format!("{ATTEMPTS} 回")));
        assert_eq!((calls, waits), (ATTEMPTS as usize, ATTEMPTS as usize - 1));
    }

    #[test]
    fn fails_when_curl_output_is_unreadable() {
        for text in ["", "abc", "200 300"] {
            let e = outcome(Some(0), text).unwrap_err();
            assert!(e.contains("読めません"), "{text:?}: {e}");
        }
        assert_eq!(outcome(Some(0), "200\n"), Ok(ok(200)));
        let r = retry(|| outcome(Some(0), ""), |_| panic!("待った"));
        assert!(r.unwrap_err().contains("読めません"));
    }

    #[test]
    fn fails_on_the_exit_code_when_curl_prints_no_status() {
        // 終了コード 3（URL の形の誤り）では、curl は %{http_code} に何も出さない。
        assert_eq!(outcome(Some(3), ""), Ok(exit(3)));
        let Next::Fail(e) = next(outcome(Some(3), "").unwrap(), 1) else {
            panic!("終了コード 3 を再試行した");
        };
        assert!(e.contains("終了コード 3"), "{e}");
        // 接続の失敗は 000 を出す。
        assert_eq!(outcome(Some(7), "000"), Ok(exit(7)));
        assert_eq!(
            outcome(None, ""),
            Ok(Outcome {
                exit: None,
                http: 0
            })
        );
    }

    #[test]
    fn fails_without_time_limits_on_curl() {
        // 時間の上限がないと、途中で止まった転送を待ち続け、再試行にも来ない。
        let args = CURL_ARGS.join(" ");
        for want in [
            "--connect-timeout 30",
            "--speed-limit 1024",
            "--speed-time 60",
        ] {
            assert!(args.contains(want), "{want} がない: {args}");
        }
        assert!(
            !CURL_ARGS.contains(&"-f"),
            "-f は 4xx と 5xx を分けられなくする"
        );
        let ci = crate::testutil::real(".github/workflows/ci.yml");
        for want in [
            "--connect-timeout 30",
            "--speed-limit 1024",
            "--speed-time 60",
        ] {
            assert!(ci.contains(want), "ci.yml の mdBook の取得に {want} がない");
        }
    }

    #[test]
    fn fails_when_the_sha256_differs() {
        let want = "0".repeat(64);
        assert_eq!(check_sha256("x", &want, &want), Ok(()));
        let e = check_sha256("x.zip", &"1".repeat(64), &want).unwrap_err();
        assert!(e.contains("x.zip") && e.contains("sha256"), "{e}");
    }

    #[test]
    fn fails_when_curl_is_called_outside_curl_once() {
        // curl_once を通らない取得は再試行の判定をすり抜ける。
        let needle = ["Command::new(", "\"curl\")"].concat();
        let mut sources = Vec::new();
        let mut dirs = vec![std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src")];
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    dirs.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    sources.push(path);
                }
            }
        }
        assert!(
            sources.iter().any(|p| p.ends_with("verus.rs")),
            "{sources:?}"
        );
        let mut count = 0;
        for path in &sources {
            let src = std::fs::read_to_string(path).unwrap();
            let body = crate::testutil::without_tests(&src);
            let n = body.matches(needle.as_str()).count();
            assert!(
                n == 0 || path.ends_with("fetch.rs"),
                "{}: curl の呼び出しは fetch::curl_once だけにする",
                path.display()
            );
            count += n;
        }
        assert_eq!(count, 1, "fetch::curl_once に curl の呼び出しがない");
    }
}
