//! ソースファイルと位置。
//!
//! 位置はバイトの範囲で持ち、行と列はファイルの本文から求める。行は 1 から、列は 1 から
//! 数え、列は Unicode のスカラー値（`char`）の数で数える（rustc の JSON 診断の
//! `column_start` と同じ）。長さはバイトで数える（`docs/current-spec.md`）。

use std::fmt;

/// ファイルの識別子。どのファイルを指すかは呼び出し側が決める。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FileId(pub u32);

/// 構文解析の入力。
#[derive(Debug, Clone)]
pub struct SourceFile {
    pub id: FileId,
    /// 診断に出すパス（`src/lib.eyk` など）。
    pub path: String,
    pub text: String,
    /// 各行の先頭のバイト位置。
    line_starts: Vec<usize>,
}

impl SourceFile {
    pub fn new(id: FileId, path: impl Into<String>, text: impl Into<String>) -> Self {
        let text = text.into();
        let mut line_starts = vec![0];
        line_starts.extend(text.match_indices('\n').map(|(i, _)| i + 1));
        SourceFile {
            id,
            path: path.into(),
            text,
            line_starts,
        }
    }

    /// バイト位置の行と列（どちらも 1 から）。
    pub fn line_col(&self, byte: usize) -> LineCol {
        let byte = byte.min(self.text.len());
        let line = self.line_starts.partition_point(|&s| s <= byte) - 1;
        let start = self.line_starts[line];
        let col = self.text[start..byte].chars().count();
        LineCol {
            line: line + 1,
            col: col + 1,
        }
    }

    /// バイトの範囲から位置を作る。
    pub fn span(&self, lo: usize, hi: usize) -> Span {
        let lo = lo.min(self.text.len());
        let hi = hi.clamp(lo, self.text.len());
        Span {
            file: self.id,
            lo,
            hi,
            start: self.line_col(lo),
            end: self.line_col(hi),
        }
    }

    /// ファイルの末尾（長さ 0）。
    pub fn eof(&self) -> Span {
        self.span(self.text.len(), self.text.len())
    }
}

/// 行と列（どちらも 1 から。列は `char` の数）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LineCol {
    pub line: usize,
    pub col: usize,
}

/// 元の位置。`lo..hi` はファイルの先頭からのバイトの範囲。
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub file: FileId,
    pub lo: usize,
    pub hi: usize,
    pub start: LineCol,
    pub end: LineCol,
}

impl Span {
    /// 長さ（バイト）。
    pub fn len(&self) -> usize {
        self.hi - self.lo
    }

    pub fn is_empty(&self) -> bool {
        self.lo == self.hi
    }

    /// 二つの位置を覆う位置。同じファイルでなければ `self`。
    pub fn to(self, other: Span) -> Span {
        if self.file != other.file {
            return self;
        }
        let (lo, start) = if other.lo < self.lo {
            (other.lo, other.start)
        } else {
            (self.lo, self.start)
        };
        let (hi, end) = if other.hi > self.hi {
            (other.hi, other.end)
        } else {
            (self.hi, self.end)
        };
        Span {
            file: self.file,
            lo,
            hi,
            start,
            end,
        }
    }
}

impl fmt::Debug for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}-{}:{}",
            self.start.line, self.start.col, self.end.line, self.end.col
        )
    }
}
