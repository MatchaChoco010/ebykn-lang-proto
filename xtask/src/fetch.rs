//! 外部からの取得（`verus-install` が使う）。
//!
//! 一時的なエラー（HTTP の 5xx・408・429、接続の失敗）だけを、回数を決めて再試行する。
//! それ以外（404 などの 4xx、書き込みの失敗）は再試行せずに失敗にする。
//! 取れなかったときは、何を・どこから取れなかったかを示して失敗する。
//!
//! CI の mdBook の取得（`.github/workflows/ci.yml` の `book`）も同じ回数と待ち時間にそろえる。

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

/// curl の呼び出しはここでだけ作る（再試行の判定を通らない取得を作らない。テストが数える）。
/// `-f` は付けない（付けると HTTP のステータスが終了コード 22 にまとまり、4xx と 5xx を分けられない）。
fn curl_once(url: &str, dest: &Path) -> Result<Outcome, String> {
    let mut cmd = Command::new("curl");
    cmd.args(["-sSL", "-w", "%{http_code}", "-o"])
        .arg(dest)
        .arg(url);
    eprintln!("$ {cmd:?}");
    let out = cmd
        .output()
        .map_err(|e| format!("curl を起動できません: {e}"))?;
    eprint!("{}", String::from_utf8_lossy(&out.stderr));
    let text = String::from_utf8_lossy(&out.stdout);
    let http = text
        .trim()
        .parse()
        .map_err(|_| format!("curl の %{{http_code}} を読めません: {text:?}"))?;
    Ok(Outcome {
        exit: out.status.code(),
        http,
    })
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
        let r = retry(
            || Err("curl の %{http_code} を読めません".to_string()),
            |_| {},
        );
        assert!(r.unwrap_err().contains("読めません"));
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
            let body = &src[..src.find("#[cfg(test)]").unwrap_or(src.len())];
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
