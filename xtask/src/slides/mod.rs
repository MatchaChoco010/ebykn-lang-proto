//! 報告スライドの道具（`slides-install` / `slides-build`）。スキル `.claude/skills/ebykn-report-slides` が使う。
//!
//! Typst の単体のバイナリと、日本語のフォント（Noto Sans JP、Noto Sans Mono CJK JP）を版と sha256 で固定し、
//! リポジトリの外のキャッシュ（`$XDG_CACHE_HOME/ebykn/`、無ければ `~/.cache/ebykn/`）に入れる。
//! 組むときはシステムのフォントも Typst に同梱のフォントも使わない。手元でも CI でも同じ PDF になる。
//!
//! `slides-build <ディレクトリ>` は `<ディレクトリ>/slides.typ` を `<ディレクトリ>/slides.pdf` に組み、
//! テンプレートが残す検査の記録（`<ebykn-check>`）を読んで、はみ出し・空のページ・リンクの抜けを失敗にする。

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::fetch;

mod pin;
pub mod selftest;
use pin::{FONTS, FONTS_COMMIT, TYPST_ASSET, TYPST_SHA256, TYPST_UNPACKED, TYPST_VERSION};

/// 展開が終わったことを示す印。中身は確かめた sha256。
const MARKER: &str = ".sha256";

/// 原稿の中で使ってよい最小の文字の大きさ（pt）。テンプレートの `size-min` と同じ。
const MIN_SIZE_PT: f64 = 10.0;

/// 1 枚の密度の上限（作者の意見で決めた。SKILL.md の「質の基準」と同じ）。
/// 箇条書きの項目の数（入れ子を含む）。
const MAX_BULLETS: usize = 5;
/// これより多い行のコードの箱を「長い」とみなす。長い箱は 1 枚に 1 つまで。
const LONG_CODE_LINES: usize = 6;
/// 1 枚のコードの行の合計。
const MAX_CODE_LINES: usize = 16;

fn cache_base() -> Result<PathBuf, String> {
    let base = match std::env::var_os("XDG_CACHE_HOME") {
        Some(d) if !d.is_empty() => PathBuf::from(d),
        _ => PathBuf::from(std::env::var_os("HOME").ok_or("HOME も XDG_CACHE_HOME もありません")?)
            .join(".cache"),
    };
    Ok(base.join("ebykn"))
}

fn typst_dir() -> Result<PathBuf, String> {
    Ok(cache_base()?.join("typst").join(TYPST_VERSION))
}

fn fonts_dir() -> Result<PathBuf, String> {
    Ok(cache_base()?.join("fonts").join(FONTS_COMMIT))
}

fn font_file(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// `url` を `download` で `dest` に取り、sha256 を固定値と比べる。合わなければ `dest` を消して失敗する。
fn fetch_pinned(
    what: &str,
    url: &str,
    dest: &Path,
    sha256: &str,
    download: impl FnOnce(&str, &Path) -> Result<(), String>,
) -> Result<(), String> {
    download(url, dest)?;
    let got = fetch::sha256_of(dest)?;
    fetch::check_sha256(what, &got, sha256).inspect_err(|_| {
        let _ = std::fs::remove_file(dest);
    })
}

/// 導入済みの Typst。印の sha256 と `typst --version` が固定値と合わなければ失敗にする。
fn installed_typst() -> Result<PathBuf, String> {
    let dir = typst_dir()?;
    let marker = std::fs::read_to_string(dir.join(MARKER)).unwrap_or_default();
    let bin = dir.join(TYPST_UNPACKED).join("typst");
    if marker.trim() != TYPST_SHA256 || !bin.is_file() {
        return Err(format!(
            "固定した Typst {TYPST_VERSION} が {} にありません。`cargo xtask slides-install` で導入してください",
            dir.display()
        ));
    }
    let out = Command::new(&bin)
        .arg("--version")
        .output()
        .map_err(|e| format!("typst を起動できません: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout);
    if !text.starts_with(&format!("typst {TYPST_VERSION} ")) {
        return Err(format!(
            "typst --version が固定値 {TYPST_VERSION} ではありません。`cargo xtask slides-install` で入れ直してください: {text}"
        ));
    }
    Ok(bin)
}

/// 導入済みのフォントのディレクトリ。各ファイルの sha256 を確かめる。
fn installed_fonts() -> Result<PathBuf, String> {
    let dir = fonts_dir()?;
    for (path, sha) in FONTS {
        let file = dir.join(font_file(path));
        if !file.is_file() {
            return Err(format!(
                "フォント {} が {} にありません。`cargo xtask slides-install` で導入してください",
                font_file(path),
                dir.display()
            ));
        }
        fetch::check_sha256(font_file(path), &fetch::sha256_of(&file)?, sha)?;
    }
    Ok(dir)
}

pub fn install() -> bool {
    match try_install() {
        Ok((typst, fonts)) => {
            eprintln!(
                "ok: slides-install: {} と {}",
                typst.display(),
                fonts.display()
            );
            true
        }
        Err(e) => {
            eprintln!("error: slides-install: {e}");
            false
        }
    }
}

fn try_install() -> Result<(PathBuf, PathBuf), String> {
    if installed_typst().is_err() {
        let dir = typst_dir()?;
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let tar = dir.join(TYPST_ASSET);
        let url = format!(
            "https://github.com/typst/typst/releases/download/v{TYPST_VERSION}/{TYPST_ASSET}"
        );
        let what = format!("Typst {TYPST_VERSION}");
        if let Err(e) = fetch_pinned(&what, &url, &tar, TYPST_SHA256, |url, dest| {
            fetch::download(&what, url, dest)
        }) {
            let _ = std::fs::remove_dir_all(&dir);
            return Err(e);
        }
        run(Command::new("tar")
            .arg("-xJf")
            .arg(&tar)
            .arg("-C")
            .arg(&dir))?;
        std::fs::remove_file(&tar).map_err(|e| format!("{}: {e}", tar.display()))?;
        std::fs::write(dir.join(MARKER), format!("{TYPST_SHA256}\n"))
            .map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let typst = installed_typst()?;
    let dir = fonts_dir()?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for (path, sha) in FONTS {
        let file = dir.join(font_file(path));
        let ok = file.is_file() && fetch::sha256_of(&file).is_ok_and(|got| got.as_str() == *sha);
        if ok {
            continue;
        }
        let url =
            format!("https://raw.githubusercontent.com/notofonts/noto-cjk/{FONTS_COMMIT}/{path}");
        let what = font_file(path);
        fetch_pinned(what, &url, &file, sha, |url, dest| {
            fetch::download(what, url, dest)
        })?;
    }
    Ok((typst, installed_fonts()?))
}

/// Typst を走らせる。失敗したときに加え、警告（`warning:`）を出したときも失敗にする。
/// 警告には、知らないフォント（`unknown font family`）のように、黙って見た目が崩れるものがある。
/// `label` は表示する呼び出し（`typst eval` の長い式を出さないため）。
fn run_typst(cmd: &mut Command, label: &str) -> Result<std::process::Output, String> {
    eprintln!("$ {label}");
    let out = cmd
        .output()
        .map_err(|e| format!("typst を起動できません: {e}"))?;
    let stderr = String::from_utf8_lossy(&out.stderr);
    eprint!("{stderr}");
    if !out.status.success() {
        return Err(format!("typst が失敗しました（{}）:\n{stderr}", out.status));
    }
    if stderr.contains("warning:") {
        return Err(format!(
            "typst が警告を出しました。警告がなくなるように原稿を直してください:\n{stderr}"
        ));
    }
    Ok(out)
}

fn run(cmd: &mut Command) -> Result<(), String> {
    eprintln!("$ {cmd:?}");
    let status = cmd
        .status()
        .map_err(|e| format!("{:?} を起動できません: {e}", cmd.get_program()))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "{:?} が失敗しました（{status}）",
            cmd.get_program()
        ))
    }
}

/// 原稿の文字の大きさの指定のうち、使えないもの（行番号と値）。`size: <数>pt` と、位置の引数の
/// `text(<数>pt)` を見る。最小の大きさより小さい pt と、大きさを確かめられない `em` の指定を返す。
pub fn small_sizes(src: &str) -> Vec<(usize, String)> {
    let mut found = Vec::new();
    for (i, line) in src.lines().enumerate() {
        for key in ["size:", "text("] {
            let mut rest = line;
            while let Some(at) = rest.find(key) {
                rest = &rest[at + key.len()..];
                let t = rest.trim_start();
                let value: String = t
                    .chars()
                    .take_while(|c| c.is_ascii_digit() || *c == '.')
                    .collect();
                if value.is_empty() {
                    continue;
                }
                let unit: String = t[value.len()..]
                    .chars()
                    .take_while(|c| c.is_ascii_alphabetic())
                    .collect();
                let too_small =
                    matches!(value.parse::<f64>(), Ok(n) if unit == "pt" && n < MIN_SIZE_PT);
                if too_small || unit == "em" {
                    found.push((i + 1, format!("{value}{unit}")));
                }
            }
        }
    }
    found
}

/// テンプレートが残す検査の記録（`<ebykn-check>`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub slides: usize,
    pub ends: usize,
    pub pages: usize,
    pub links: usize,
    /// はみ出したスライド（題、始まりのページ、終わりのページ）。
    pub overflow: Vec<(String, usize, usize)>,
    /// 箱の幅に収まらず折り返されたコードの行（ページ、行）。
    pub wraps: Vec<(usize, String)>,
    /// スライドごとの密度。
    pub density: Vec<Density>,
    /// PR / Issue ごとのスライド（ページ、変更の鍵、見出し）。
    pub changes: Vec<(usize, String, String)>,
    /// 一覧の行（変更の鍵、見出し）。
    pub entries: Vec<(String, String)>,
}

/// 1 枚のスライドの密度。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Density {
    pub page: usize,
    pub title: String,
    /// 箇条書きの項目の数（入れ子を含む）。
    pub bullets: usize,
    /// 段組みの中に置いたコードの箱の数。
    pub narrow: usize,
    /// コードの箱ごとの行数。
    pub codes: Vec<usize>,
}

/// `typst eval` に渡す式。検査の記録を 1 行 1 項目の文字列にする（JSON の入れ子を読まずに済ませる）。
const CHECK_EXPR: &str = r#"query(<ebykn-check>).map(it => {
  let v = it.value
  let head = ("slides " + str(v.slides), "ends " + str(v.ends), "pages " + str(v.pages), "links " + str(v.links))
  let over = v.overflow.map(o => "overflow " + str(o.from) + " " + str(o.to) + " " + o.title)
  let wraps = v.wraps.map(w => "wrap " + str(w.page) + " " + w.line)
  let density = v.density.map(d => "slide " + str(d.page) + " " + str(d.bullets) + " " + str(d.narrow) + " "
    + (if d.codes.len() == 0 { "-" } else { d.codes.map(str).join(",") }) + " " + d.title)
  let changes = v.changes.map(c => "change " + str(c.page) + " " + c.key + " " + c.heading)
  let entries = v.entries.map(e => "entry " + e.key + " " + e.heading)
  (head + over + wraps + density + changes + entries).join("\n")
}).join("\n---\n")"#;

/// `typst eval --format json` の出力（JSON の文字列 1 つ）を読む。
fn json_string(text: &str) -> Result<String, String> {
    let t = text.trim();
    // テンプレートを使っていない原稿では記録がなく、`null` になる。空として read_check に渡す。
    if t == "null" {
        return Ok(String::new());
    }
    let inner = t
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .ok_or_else(|| format!("typst eval の出力が JSON の文字列ではありません: {t}"))?;
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some('/') => out.push('/'),
            Some('u') => {
                let hex: String = chars.by_ref().take(4).collect();
                let n = u32::from_str_radix(&hex, 16)
                    .map_err(|_| format!("typst eval の出力の \\u{hex} を読めません"))?;
                out.push(char::from_u32(n).unwrap_or('\u{fffd}'));
            }
            other => return Err(format!("typst eval の出力の \\{other:?} を読めません")),
        }
    }
    Ok(out)
}

/// 検査の記録を読む。記録がちょうど 1 つでなければ（テンプレートを使っていない原稿など）失敗にする。
pub fn read_check(text: &str) -> Result<Check, String> {
    if text.is_empty() {
        return Err("検査の記録 <ebykn-check> がありません。原稿が `#show: deck.with(..)` を使っているか確かめてください".to_string());
    }
    if text.contains("\n---\n") {
        return Err("検査の記録 <ebykn-check> が 2 つ以上あります".to_string());
    }
    let mut check = Check {
        slides: 0,
        ends: 0,
        pages: 0,
        links: 0,
        overflow: Vec::new(),
        wraps: Vec::new(),
        density: Vec::new(),
        changes: Vec::new(),
        entries: Vec::new(),
    };
    let mut seen = Vec::new();
    for line in text.lines() {
        let (key, rest) = line
            .split_once(' ')
            .ok_or_else(|| format!("検査の記録の行を読めません: {line}"))?;
        let num = |s: &str| {
            s.parse::<usize>()
                .map_err(|_| format!("検査の記録の行を読めません: {line}"))
        };
        match key {
            "slides" | "ends" | "pages" | "links" => {
                if seen.contains(&key) {
                    return Err(format!("検査の記録の {key} が重複しています"));
                }
                seen.push(key);
                let n = num(rest)?;
                match key {
                    "slides" => check.slides = n,
                    "ends" => check.ends = n,
                    "pages" => check.pages = n,
                    _ => check.links = n,
                }
            }
            "overflow" => {
                let mut parts = rest.splitn(3, ' ');
                let from = num(parts.next().unwrap_or_default())?;
                let to = num(parts.next().unwrap_or_default())?;
                let title = parts.next().unwrap_or_default().to_string();
                check.overflow.push((title, from, to));
            }
            "slide" => {
                let mut parts = rest.splitn(5, ' ');
                let mut next = || parts.next().unwrap_or_default();
                let page = num(next())?;
                let bullets = num(next())?;
                let narrow = num(next())?;
                let codes = match next() {
                    "-" => Vec::new(),
                    list => list.split(',').map(num).collect::<Result<_, _>>()?,
                };
                let title = next().to_string();
                check.density.push(Density {
                    page,
                    title,
                    bullets,
                    narrow,
                    codes,
                });
            }
            "change" => {
                let mut parts = rest.splitn(3, ' ');
                let page = num(parts.next().unwrap_or_default())?;
                let key = parts.next().unwrap_or_default().to_string();
                let heading = parts.next().unwrap_or_default().to_string();
                check.changes.push((page, key, heading));
            }
            "entry" => {
                let (key, heading) = rest.split_once(' ').unwrap_or((rest, ""));
                check.entries.push((key.to_string(), heading.to_string()));
            }
            "wrap" => {
                let (page, line) = rest.split_once(' ').unwrap_or((rest, ""));
                check.wraps.push((num(page)?, line.to_string()));
            }
            _ => return Err(format!("検査の記録に未知の項目 {key}")),
        }
    }
    for key in ["slides", "ends", "pages", "links"] {
        if !seen.contains(&key) {
            return Err(format!("検査の記録に {key} がありません"));
        }
    }
    Ok(check)
}

/// 検査の記録と、PDF の中のリンクの数から、問題を並べる。空なら通る。
pub fn judge(check: &Check, pdf_links: usize) -> Vec<String> {
    let mut errors = Vec::new();
    for (title, from, to) in &check.overflow {
        errors.push(format!(
            "スライド「{title}」が {from} ページ目から {to} ページ目にはみ出しています。中身を減らすか、スライドを分けてください"
        ));
    }
    for (page, line) in &check.wraps {
        errors.push(format!(
            "{page} ページ目のコードの行が箱に収まらず折り返されています。行を短くするか、箱を広くしてください: {line}"
        ));
    }
    for d in &check.density {
        let at = format!("{} ページ目「{}」", d.page, d.title);
        if d.narrow > 0 {
            errors.push(format!(
                "{at}：コードの箱を段組みの中に置いています。コードは横幅いっぱいに置き、長ければスライドを分けてください"
            ));
        }
        if d.bullets > MAX_BULLETS {
            errors.push(format!(
                "{at}：箇条書きが {} 項目あります（{MAX_BULLETS} 項目まで）。スライドを分けてください",
                d.bullets
            ));
        }
        let long = d.codes.iter().filter(|&&n| n > LONG_CODE_LINES).count();
        if long > 1 {
            errors.push(format!(
                "{at}：{LONG_CODE_LINES} 行を超えるコードの箱が {long} つあります（1 枚に 1 つまで）。スライドを分けてください"
            ));
        }
        let total: usize = d.codes.iter().sum();
        if total > MAX_CODE_LINES {
            errors.push(format!(
                "{at}：コードが合わせて {total} 行あります（{MAX_CODE_LINES} 行まで）。要点の行に絞るか、スライドを分けてください"
            ));
        }
    }
    errors.extend(judge_headings(&check.changes, &check.entries));
    if check.slides != check.density.len() {
        errors.push(format!(
            "密度の記録（{}）がスライドの数（{}）と合いません",
            check.density.len(),
            check.slides
        ));
    }
    if check.slides == 0 {
        errors.push("スライドが 1 枚もありません".to_string());
    }
    if check.slides != check.ends {
        errors.push(format!(
            "スライドの始まり（{}）と終わり（{}）の数が合いません。テンプレートの関数の外に中身を書いていないか確かめてください",
            check.slides, check.ends
        ));
    }
    if check.pages != check.slides {
        errors.push(format!(
            "ページの数（{}）がスライドの数（{}）と違います。はみ出しか、スライドの外の中身で空のページができています",
            check.pages, check.slides
        ));
    }
    if check.links == 0 {
        errors.push("PR と Issue へのリンクが 1 つもありません".to_string());
    }
    if pdf_links != check.links {
        errors.push(format!(
            "PDF の中のリンク（{pdf_links}）が、原稿のリンク（{}）と合いません",
            check.links
        ));
    }
    errors
}

/// 見出しの中のマイルストーンのコードネーム（`M0`、`F1` など）。変更の中身を表さないので使わない。
fn codename(heading: &str) -> Option<&str> {
    heading
        .split(|c: char| !c.is_ascii_alphanumeric())
        .find(|w| {
            let mut cs = w.chars();
            matches!(cs.next(), Some('M' | 'F' | 'H'))
                && !cs.as_str().is_empty()
                && cs.all(|c| c.is_ascii_digit())
        })
}

/// 1 つの PR / Issue のページは同じ見出しで続けて並び、一覧の行も同じ見出しを使う。
pub fn judge_headings(
    changes: &[(usize, String, String)],
    entries: &[(String, String)],
) -> Vec<String> {
    let mut errors = Vec::new();
    let mut keys: Vec<&str> = Vec::new();
    for (_, key, _) in changes {
        if !keys.contains(&key.as_str()) {
            keys.push(key);
        }
    }
    for key in &keys {
        let pages: Vec<&(usize, String, String)> = changes.iter().filter(|c| c.1 == *key).collect();
        let first = &pages[0].2;
        for (page, _, heading) in &pages {
            if heading != first {
                errors.push(format!(
                    "{page} ページ目：{key} の見出し「{heading}」が、同じ変更の 1 枚目の見出し「{first}」と違います"
                ));
            }
        }
        let nums: Vec<usize> = pages.iter().map(|c| c.0).collect();
        if nums.windows(2).any(|w| w[1] != w[0] + 1) {
            errors.push(format!(
                "{key} のページが続いていません（{nums:?}）。同じ変更のページはまとめて並べてください"
            ));
        }
        if let Some(word) = codename(first) {
            errors.push(format!(
                "{key} の見出し「{first}」にコードネーム {word} があります。変更の中身を表す名前にしてください"
            ));
        }
        if let Some((other, _, _)) = changes
            .iter()
            .map(|c| (&c.1, c.0, &c.2))
            .find(|(k, _, h)| *k != key && *h == first)
        {
            errors.push(format!(
                "{key} と {other} が同じ見出し「{first}」を使っています"
            ));
        }
        match entries.iter().find(|e| e.0 == *key) {
            None => errors.push(format!("{key} が一覧にありません")),
            Some((_, h)) if h != first => errors.push(format!(
                "一覧の {key} の見出し「{h}」が、スライドの見出し「{first}」と違います"
            )),
            Some(_) => {}
        }
    }
    for (key, _) in entries {
        if !keys.contains(&key.as_str()) {
            errors.push(format!("一覧の {key} にスライドがありません"));
        }
    }
    errors
}

/// PDF の中の外部リンクの数。リンクの動作の辞書の `/URI` の鍵（直後が文字列の `(`）を数える。
/// 動作の種類の `/S /URI` は数えない。
fn pdf_uri_count(pdf: &[u8]) -> usize {
    pdf.windows(4)
        .enumerate()
        .filter(|(i, w)| {
            *w == b"/URI"
                && pdf[i + 4..]
                    .iter()
                    .find(|b| !b.is_ascii_whitespace())
                    .is_some_and(|b| *b == b'(')
        })
        .count()
}

fn typst_args(cmd: &mut Command, root: &Path, fonts: &Path) {
    cmd.arg("--root")
        .arg(root)
        .arg("--ignore-system-fonts")
        .arg("--ignore-embedded-fonts")
        .arg("--font-path")
        .arg(fonts);
}

/// テンプレートの置き場（リポジトリの直下から）。
pub const TEMPLATE: &str = ".claude/skills/ebykn-report-slides/template/ebykn-slides.typ";

/// パッケージ（`"@preview/.."` など）を読み込む行（行番号と行）。Typst はパッケージをネットから取ってきて、
/// 止める引数がない。取ってくると版が固定されず、手元と CI で中身が変わりうるので、組む前に拒否する。
pub fn package_imports(src: &str) -> Vec<(usize, String)> {
    src.lines()
        .enumerate()
        .filter(|(_, l)| {
            let t = l.trim_start().trim_start_matches('#');
            (t.starts_with("import") || t.starts_with("include")) && t.contains("\"@")
        })
        .map(|(i, l)| (i + 1, l.trim().to_string()))
        .collect()
}

pub fn build(root: &Path, dir: &str) -> bool {
    match try_build(root, Path::new(dir)) {
        Ok((pdf, check)) => {
            eprintln!(
                "ok: slides-build: {}（{} ページ、リンク {}）",
                pdf.display(),
                check.pages,
                check.links
            );
            true
        }
        Err(errors) => {
            for e in &errors {
                eprintln!("error: slides-build: {e}");
            }
            false
        }
    }
}

pub(crate) fn try_build(root: &Path, dir: &Path) -> Result<(PathBuf, Check), Vec<String>> {
    let one = |e: String| vec![e];
    let src = dir.join("slides.typ");
    let pdf = dir.join("slides.pdf");
    let text = std::fs::read_to_string(&src).map_err(|e| one(format!("{}: {e}", src.display())))?;
    let small = small_sizes(&text);
    if !small.is_empty() {
        return Err(small
            .into_iter()
            .map(|(line, v)| {
                format!(
                    "{}:{line}: 文字の大きさ {v} は使えません（pt で {MIN_SIZE_PT}pt 以上にする。em は大きさを確かめられない）",
                    src.display()
                )
            })
            .collect());
    }
    let template = std::fs::read_to_string(root.join(TEMPLATE))
        .map_err(|e| one(format!("{TEMPLATE}: {e}")))?;
    let mut packages = Vec::new();
    for (name, body) in [
        (src.display().to_string(), &text),
        (TEMPLATE.to_string(), &template),
    ] {
        for (line, l) in package_imports(body) {
            packages.push(format!(
                "{name}:{line}: パッケージは使えません（ネットから取ってくるので版が固定されない）。Typst の標準の機能で書いてください: {l}"
            ));
        }
    }
    if !packages.is_empty() {
        return Err(packages);
    }
    let typst = installed_typst().map_err(one)?;
    let fonts = installed_fonts().map_err(one)?;
    let _ = std::fs::remove_file(&pdf);
    // 作成日時は HEAD のコミットの時刻にする（同じコミットからは同じ PDF になる）。
    let stamp = Command::new("git")
        .args(["log", "-1", "--format=%ct", "HEAD"])
        .current_dir(root)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .ok_or_else(|| one("git log で HEAD の時刻を読めません".to_string()))?;
    let mut compile = Command::new(&typst);
    compile.arg("compile");
    typst_args(&mut compile, root, &fonts);
    compile
        .arg("--creation-timestamp")
        .arg(&stamp)
        .arg(&src)
        .arg(&pdf);
    run_typst(
        &mut compile,
        &format!("typst compile {} {}", src.display(), pdf.display()),
    )
    .map_err(one)?;
    let mut eval = Command::new(&typst);
    eval.args(["eval", CHECK_EXPR, "--in"]).arg(&src);
    typst_args(&mut eval, root, &fonts);
    let out = run_typst(
        &mut eval,
        &format!("typst eval <検査の記録> --in {}", src.display()),
    )
    .map_err(one)?;
    let check = json_string(&String::from_utf8_lossy(&out.stdout))
        .and_then(|t| read_check(&t))
        .map_err(one)?;
    let bytes = std::fs::read(&pdf).map_err(|e| one(format!("{}: {e}", pdf.display())))?;
    let errors = judge(&check, pdf_uri_count(&bytes));
    if errors.is_empty() {
        Ok((pdf, check))
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{mutate, real};

    const GOOD: &str = "slides 3\nends 3\npages 3\nlinks 5\nslide 1 0 0 - 表紙\nslide 2 3 0 4,6 実例\nslide 3 2 0 - 確かめ方";

    #[test]
    fn passes_a_deck_that_fits() {
        let c = read_check(GOOD).unwrap();
        assert_eq!(judge(&c, 5), Vec::<String>::new());
    }

    #[test]
    fn fails_when_a_slide_overflows() {
        let text = format!(
            "{}\noverflow 2 3 構文解析器",
            mutate(GOOD, "pages 3", "pages 4")
        );
        let c = read_check(&text).unwrap();
        assert_eq!(c.overflow, vec![("構文解析器".to_string(), 2, 3)]);
        let errors = judge(&c, 5);
        assert!(
            errors.iter().any(|e| e.contains("「構文解析器」")),
            "{errors:?}"
        );
        assert!(
            errors.iter().any(|e| e.contains("ページの数（4）")),
            "{errors:?}"
        );
    }

    #[test]
    fn fails_when_a_slide_is_too_dense() {
        let c = read_check(GOOD).unwrap();
        assert_eq!(
            c.density[1],
            Density {
                page: 2,
                title: "実例".to_string(),
                bullets: 3,
                narrow: 0,
                codes: vec![4, 6],
            }
        );
        let judge_of =
            |from: &str, to: &str| judge(&read_check(&mutate(GOOD, from, to)).unwrap(), 5);
        let has = |errors: Vec<String>, needle: &str| {
            assert!(
                errors
                    .iter()
                    .any(|e| e.contains(needle) && e.contains("「実例」")),
                "{needle}: {errors:?}"
            )
        };
        has(judge_of("slide 2 3 0", "slide 2 6 0"), "箇条書きが 6 項目");
        has(judge_of("slide 2 3 0", "slide 2 3 1"), "段組み");
        has(judge_of("4,6 実例", "7,7 実例"), "2 つあります");
        has(judge_of("4,6 実例", "6,6,5 実例"), "合わせて 17 行");
        // ちょうど上限なら通る。
        assert!(judge_of("slide 2 3 0 4,6", "slide 2 5 0 10,6").is_empty());
        // 密度の記録が欠けたら失敗にする。
        let c = read_check(&mutate(GOOD, "\nslide 3 2 0 - 確かめ方", "")).unwrap();
        assert!(judge(&c, 5).iter().any(|e| e.contains("密度の記録（2）")));
    }

    #[test]
    fn fails_when_headings_of_one_change_differ() {
        let c = |p: usize, k: &str, h: &str| (p, k.to_string(), h.to_string());
        let e = |k: &str, h: &str| (k.to_string(), h.to_string());
        let changes = vec![
            c(4, "pr-13", "Verus 取り込み"),
            c(5, "pr-13", "Verus 取り込み"),
            c(6, "pr-15", "構文解析器"),
        ];
        let entries = vec![e("pr-13", "Verus 取り込み"), e("pr-15", "構文解析器")];
        assert_eq!(judge_headings(&changes, &entries), Vec::<String>::new());
        let has =
            |changes: &[(usize, String, String)], entries: &[(String, String)], needle: &str| {
                let errors = judge_headings(changes, entries);
                assert!(
                    errors.iter().any(|x| x.contains(needle)),
                    "{needle}: {errors:?}"
                );
            };
        let mut bad = changes.clone();
        bad[1].2 = "実例".to_string();
        has(&bad, &entries, "1 枚目の見出し「Verus 取り込み」と違います");
        let mut bad = changes.clone();
        bad[1].0 = 7;
        has(&bad, &entries, "続いていません");
        let mut bad = changes.clone();
        for x in bad.iter_mut().take(2) {
            x.2 = "M0：Verus".to_string();
        }
        let ent = vec![e("pr-13", "M0：Verus"), e("pr-15", "構文解析器")];
        has(&bad, &ent, "コードネーム M0");
        has(&changes, &entries[..1], "pr-15 が一覧にありません");
        has(
            &changes,
            &[e("pr-13", "Verus 取り込み"), e("pr-15", "構文")],
            "一覧の pr-15 の見出し",
        );
        let mut ent = entries.clone();
        ent.push(e("pr-99", "ほか"));
        has(&changes, &ent, "pr-99 にスライドがありません");
        let mut bad = changes.clone();
        bad[2].2 = "Verus 取り込み".to_string();
        has(
            &bad,
            &[e("pr-13", "Verus 取り込み"), e("pr-15", "Verus 取り込み")],
            "同じ見出し",
        );
        // M2 のような語でない英数字（`Ptr`、`E0504`、`MF`）はコードネームではない。
        assert_eq!(codename("Ptr と E0504 と MF"), None);
        assert_eq!(codename("F1 の参照モデル"), Some("F1"));
    }

    #[test]
    fn fails_when_a_code_line_wraps() {
        let c = read_check(&format!("{GOOD}\nwrap 2 test fetch::tests::x ... ok")).unwrap();
        assert_eq!(
            c.wraps,
            vec![(2, "test fetch::tests::x ... ok".to_string())]
        );
        let errors = judge(&c, 5);
        assert!(
            errors
                .iter()
                .any(|e| e.contains("2 ページ目") && e.contains("fetch::tests::x")),
            "{errors:?}"
        );
    }

    #[test]
    fn fails_when_there_is_an_empty_page() {
        let c = read_check(&mutate(GOOD, "pages 3", "pages 4")).unwrap();
        let errors = judge(&c, 5);
        assert!(
            errors.iter().any(|e| e.contains("空のページ")),
            "{errors:?}"
        );
    }

    #[test]
    fn fails_when_links_are_missing_from_the_pdf() {
        let c = read_check(GOOD).unwrap();
        assert!(
            judge(&c, 4)
                .iter()
                .any(|e| e.contains("PDF の中のリンク（4）"))
        );
        let c = read_check(&mutate(GOOD, "links 5", "links 0")).unwrap();
        assert!(
            judge(&c, 0)
                .iter()
                .any(|e| e.contains("リンクが 1 つもありません"))
        );
    }

    #[test]
    fn fails_when_slide_markers_do_not_pair() {
        let c = read_check(&mutate(GOOD, "ends 3", "ends 2")).unwrap();
        assert!(
            judge(&c, 5)
                .iter()
                .any(|e| e.contains("始まり（3）と終わり（2）"))
        );
    }

    #[test]
    fn fails_when_the_check_record_is_unreadable() {
        assert!(read_check("").unwrap_err().contains("ありません"));
        assert!(
            read_check(&format!("{GOOD}\n---\n{GOOD}"))
                .unwrap_err()
                .contains("2 つ以上")
        );
        assert!(
            read_check(&mutate(GOOD, "pages 3", "pages x"))
                .unwrap_err()
                .contains("読めません")
        );
        assert!(
            read_check(&mutate(GOOD, "\npages 3", ""))
                .unwrap_err()
                .contains("pages がありません")
        );
        assert!(
            read_check(&format!("{GOOD}\nlinks 5"))
                .unwrap_err()
                .contains("重複")
        );
        assert!(
            read_check(&format!("{GOOD}\nfoo 1"))
                .unwrap_err()
                .contains("未知")
        );
    }

    #[test]
    fn reads_the_json_string_from_typst_eval() {
        let t = json_string("\"slides 1\\noverflow 1 2 \\\"題\\\" \\u00e9\"\n").unwrap();
        assert_eq!(t, "slides 1\noverflow 1 2 \"題\" é");
        assert!(json_string("[1]").is_err());
        // テンプレートを使わない原稿では null になる。空として read_check の分かりやすい文言に回す。
        assert_eq!(json_string("null\n").unwrap(), "");
        assert!(
            read_check(&json_string("null").unwrap())
                .unwrap_err()
                .contains("deck.with")
        );
        assert!(json_string("\"\\x\"").is_err());
    }

    #[test]
    fn fails_on_text_smaller_than_the_minimum() {
        let src = "#text(size: 12pt)[a]\n#set text(size: 9.5pt)\n#text(14pt)[b]";
        assert_eq!(small_sizes(src), vec![(2, "9.5pt".to_string())]);
        assert!(small_sizes(&mutate(src, "9.5pt", "10pt")).is_empty());
        // 位置の引数の大きさと、大きさを確かめられない em も捕まえる。
        assert_eq!(
            small_sizes(&mutate(src, "text(14pt)", "text(8pt)")),
            vec![(2, "9.5pt".to_string()), (3, "8pt".to_string())]
        );
        assert_eq!(
            small_sizes(&mutate(src, "size: 12pt", "size: 0.5em")),
            vec![(1, "0.5em".to_string()), (2, "9.5pt".to_string())]
        );
        // 大きさでない引数（text(fill: ..)）は見ない。
        assert!(small_sizes("#text(fill: red)[x]").is_empty());
    }

    #[test]
    fn fails_on_package_imports() {
        let src = "#import \"/.claude/x.typ\": *\n#import \"@preview/cetz:0.3.0\"\n  #include \"@local/a:1.0.0\"";
        assert_eq!(
            package_imports(src).iter().map(|x| x.0).collect::<Vec<_>>(),
            vec![2, 3]
        );
        assert!(package_imports(&mutate(src, "@preview/cetz:0.3.0", "/b.typ")).len() == 1);
        // 本物のテンプレートはパッケージを使っていない。
        assert!(package_imports(&real(TEMPLATE)).is_empty());
    }

    #[test]
    fn counts_uri_links_in_a_pdf() {
        let pdf = b"<< /S /URI /URI (https://a) >> <</Type/Action/S/URI/URI(https://b)>>";
        assert_eq!(pdf_uri_count(pdf), 2);
    }

    #[test]
    fn the_template_records_the_check() {
        // テンプレートの検査の記録と、ここで読む項目が食い違わない。
        let t = real(".claude/skills/ebykn-report-slides/template/ebykn-slides.typ");
        for key in [
            "slides:",
            "ends:",
            "pages:",
            "overflow:",
            "wraps:",
            "<ebykn-wrap>",
            "density:",
            "<ebykn-codebox>",
            "changes:",
            "entries:",
            "<ebykn-change>",
            "<ebykn-entry>",
            "links:",
            "<ebykn-check>",
        ] {
            assert!(t.contains(key), "テンプレートに {key} がない");
        }
        assert!(t.contains(&format!("#let size-min = {MIN_SIZE_PT}pt")));
    }

    #[test]
    fn pins_are_well_formed() {
        let hex = |s: &str, n: usize| s.len() == n && s.bytes().all(|b| b.is_ascii_hexdigit());
        assert!(hex(TYPST_SHA256, 64));
        assert!(hex(FONTS_COMMIT, 40));
        for (path, sha) in FONTS {
            assert!(hex(sha, 64), "{path}");
            assert!(path.ends_with(".otf"), "{path}");
        }
        assert!(TYPST_ASSET.starts_with(TYPST_UNPACKED));
    }

    #[test]
    fn fails_when_the_fetched_file_is_not_the_pinned_one() {
        let dir = std::env::temp_dir().join(format!("ebykn-xtask-slides-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let dest = dir.join("font.otf");
        let mut asked = None;
        let r = fetch_pinned(
            "font.otf",
            "https://example/f",
            &dest,
            FONTS[0].1,
            |url, d| {
                asked = Some(url.to_string());
                std::fs::write(d, "途中で切れたフォント").map_err(|e| e.to_string())
            },
        );
        let e = r.unwrap_err();
        assert!(e.contains("sha256") && e.contains("font.otf"), "{e}");
        assert!(!dest.exists(), "sha256 の合わないファイルを残した");
        assert_eq!(asked.as_deref(), Some("https://example/f"));
        let r = fetch_pinned("x", "u", &dest, FONTS[0].1, |_, _| {
            Err("取れない".to_string())
        });
        assert_eq!(r, Err("取れない".to_string()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn fails_when_try_install_fetches_outside_fetch_pinned() {
        // fetch_pinned を通らない取得は sha256 の照合をすり抜ける。
        let src = real("xtask/src/slides/mod.rs");
        let body = crate::testutil::without_tests(&src);
        let call = "fetch::download(";
        assert_eq!(
            body.matches(call).count(),
            2,
            "Typst とフォントの 2 か所だけ"
        );
        for (at, _) in body.match_indices(call) {
            let start = body[..at]
                .rfind("fetch_pinned(")
                .expect("fetch_pinned の外");
            assert!(
                !body[start..at].contains(';'),
                "fetch_pinned の引数の外: {}",
                &body[start..at]
            );
        }
    }
}
