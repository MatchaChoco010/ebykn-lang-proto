// Ebykn の報告スライドのテンプレート（Typst）。
//
// 原稿（target/reports/<識別子>/slides.typ）から次のように読み込む。`cargo xtask slides-build` は
// リポジトリの直下を Typst の root にするので、絶対パスで書ける。
//
//   #import "/.claude/skills/ebykn-report-slides/template/ebykn-slides.typ": *
//   #show: deck.with(release: "draft-1", base: "6b4075e", head: "f9cf01f", date: "2026-11-09")
//
// 1 枚のスライドは 1 ページに収める。はみ出すと次のページに流れ、`slides-build` が
// 「はみ出し」として失敗する（末尾の `<ebykn-check>` を読む）。

#let repo-default = "MatchaChoco010/ebykn-lang-proto"

// 文字の大きさ。これより小さい文字は使わない（`slides-build` が原稿の `size:` を検査する）。
#let size-body = 15pt
#let size-small = 12pt
#let size-code = 12pt
#let size-min = 10pt

#let ink = rgb("#1f2328")
#let muted = rgb("#57606a")
#let faint = rgb("#d0d7de")
#let paper = rgb("#ffffff")
#let code-bg = rgb("#f5f6f8")
#let accent = rgb("#0b5cad")
#let bad = rgb("#b42318")
#let good = rgb("#0a6b2e")

// 変更した部分の分類。原稿では鍵（transpiler など）で指定する。
#let areas = (
  transpiler: (label: "トランスパイラ本体", color: rgb("#0b5cad")),
  prelude: (label: "prelude", color: rgb("#6639ba")),
  vendor: (label: "取り込み（vendor）", color: rgb("#57606a")),
  golden: (label: "テスト・golden", color: rgb("#1f6f78")),
  book: (label: "Book", color: rgb("#0a6b2e")),
  harness: (label: "CI・ハーネス", color: rgb("#9a4d00")),
  formal: (label: "形式モデル", color: rgb("#a3185b")),
  rules: (label: "開発ルール・文書", color: rgb("#3d4b5c")),
)

#let config = state("ebykn-deck", (repo: repo-default, release: "", base: "", head: "", date: ""))

// ---- リンク ----

// 外部リンク。数を `<ebykn-link>` に残し、`slides-build` が PDF のリンクの数と比べる。
#let ext-link(url, body) = {
  [#metadata(url) <ebykn-link>]
  // 行の途中で折り返すと PDF のリンクが 2 つに分かれるので、折り返さない箱に入れる。
  box(link(url, underline(stroke: 0.6pt + accent.lighten(40%), offset: 2pt, text(fill: accent, body))))
}

#let gh-url(kind, n) = context {
  "https://github.com/" + config.get().repo + "/" + kind + "/" + str(n)
}

#let gh-link(kind, n, prefix: "#") = context {
  let url = "https://github.com/" + config.get().repo + "/" + kind + "/" + str(n)
  ext-link(url, prefix + str(n))
}

#let pr(n) = gh-link("pull", n, prefix: "PR #")
#let issue(n) = gh-link("issues", n, prefix: "Issue #")
#let commit(sha) = gh-link("commit", sha, prefix: "")

// ---- 部品 ----

#let area-chip(key) = {
  if key not in areas { panic("未知の分類: " + key + "（" + areas.keys().join("、") + " のどれか）") }
  let a = areas.at(key)
  box(
    fill: a.color.lighten(88%),
    stroke: 0.6pt + a.color.lighten(30%),
    inset: (x: 6pt, y: 3pt),
    radius: 3pt,
    baseline: 3pt,
    text(size: size-small, fill: a.color.darken(10%), weight: "bold", a.label),
  )
}

#let chips(keys) = keys.map(area-chip).join(h(4pt))

#let mono = ("Noto Sans Mono CJK JP",)
#let code-inset = (x: 8pt, y: 7pt)

// 箱の幅に収まらず折り返されるコードの行を `<ebykn-wrap>` に残す。`slides-build` が失敗にする。
// 折り返しはコードを読み違えさせるので、行を短くするか、箱を広くする。
#let wrap-check(body, size) = layout(region => {
  // 箱の幅（段組みの中のコードを見つけるため）。
  [#metadata(region.width.pt() + 2 * code-inset.x.pt()) <ebykn-codebox>]
  if type(body) == content and body.func() == raw {
    let width = region.width
    for line in body.text.split("\n") {
      if measure(text(font: mono, size: size, line)).width > width {
        [#metadata(line) <ebykn-wrap>]
      }
    }
  }
})

// コードの箱。`title` は箱の上の小さな見出し（ファイル名やコマンドの説明）。
#let code-box(body, title: none, size: size-code) = {
  // raw の既定の 0.8em は相対の大きさでは打ち消せないので、pt で指定する。
  show raw: set text(size: size)
  block(width: 100%, breakable: false, {
    if title != none {
      block(below: 3pt, text(size: size-small, fill: muted, weight: "bold", title))
    }
    block(
      width: 100%,
      fill: code-bg,
      stroke: 0.6pt + faint,
      radius: 4pt,
      inset: code-inset,
      // 検査は流れの外に置き、箱の高さを変えない。
      { place(top + left, wrap-check(body, size)); body },
    )
  })
}

// 枠付きの短い注記。
#let note(body, color: accent) = block(
  width: 100%,
  fill: color.lighten(92%),
  stroke: (left: 3pt + color),
  inset: (x: 10pt, y: 7pt),
  text(size: size-small, body),
)

// 修正前と修正後を上下に並べる。`before` と `after` は code-box などの中身。
// コードは段組みにしない（`slides-build` が失敗にする）。長い出力なら、修正前と修正後を別のスライドにする。
#let before-after(before: none, after: none, before-title: "修正前", after-title: "修正後") = {
  let head(label, color) = block(
    below: 4pt,
    text(size: size-small, weight: "bold", fill: color, label),
  )
  block(below: 12pt, [#head(before-title, bad)#before])
  block([#head(after-title, good)#after])
}

// 文章だけの段組み。コードの箱は入れない（`slides-build` が失敗にする）。
#let cols(..args, columns: (1fr, 1fr), gutter: 18pt) = grid(
  columns: columns,
  column-gutter: gutter,
  ..args,
)

// ---- スライド ----

// 内容を地の文字列にする（検査の記録に題を残すため）。
#let plain(c) = {
  if c == none { "" } else if type(c) == str { c } else if c.has("text") { c.text } else if c.has("children") {
    c.children.map(plain).join()
  } else if c.has("body") { plain(c.body) } else if c == [ ] { " " } else { "" }
}

#let slide-start(title) = [#metadata(plain(title)) <ebykn-slide-start>]
#let slide-end = [#metadata("end") <ebykn-slide-end>]

// 素のスライド。`kicker` は題の上の小さな行（分類や番号）。
#let slide(title: none, kicker: none, label-name: none, body) = {
  pagebreak(weak: true)
  slide-start(title)
  if kicker != none {
    block(below: 6pt, kicker)
  }
  if title != none {
    let h = block(below: 10pt, text(size: 24pt, weight: "bold", fill: ink, title))
    if label-name != none [#h #label(label-name)] else { h }
    line(length: 100%, stroke: 0.8pt + faint)
    v(4pt)
  }
  body
  slide-end
}

// PR / Issue ごとのスライド。1 つの変更のすべてのページに同じ `heading`（変更の中身を表す短い名前。
// M0 などのコードネームは使わない）を H1 として出し、ページごとの題は `caption` に書く。
// 先頭に PR と Issue へのリンクと分類を出し、H1 の横に変更の中での位置（2/5 など）を自動で出す。
// 一覧（`overview`）の行は、同じ `heading` で 1 枚目にリンクする。
#let change-key(pr, issues) = if pr != none { "pr-" + str(pr) } else { "issue-" + str(issues.first()) }

#let change(pr: none, issues: (), areas: (), heading: none, caption: none, body) = {
  if pr == none and issues.len() == 0 { panic("change には pr か issues が要る: " + repr(heading)) }
  if areas.len() == 0 { panic("change には areas が要る: " + repr(heading)) }
  if heading == none or caption == none { panic("change には heading と caption が要る: " + repr(heading)) }
  let key = change-key(pr, issues)
  let links = ()
  if pr != none { links.push(gh-link("pull", pr, prefix: "PR #")) }
  for i in issues { links.push(gh-link("issues", i, prefix: "Issue #")) }
  pagebreak(weak: true)
  slide-start(plain(heading) + "：" + plain(caption))
  [#metadata((key: key, heading: plain(heading))) <ebykn-change>]
  block(below: 6pt, {
    set text(size: size-small)
    links.join(text(fill: muted, "　"))
    h(10pt)
    chips(areas)
  })
  context {
    let mine = query(<ebykn-change>).filter(m => m.value.key == key)
    let n = query(selector(<ebykn-change>).before(here())).filter(m => m.value.key == key).len()
    let pos = if mine.len() > 1 { str(n) + "/" + str(mine.len()) } else { none }
    let h = block(below: 9pt, {
      text(size: 24pt, weight: "bold", fill: ink, heading)
      if pos != none { h(10pt); text(size: 14pt, weight: "bold", fill: muted, pos) }
    })
    if n == 1 [#h #label(key)] else { h }
  }
  block(below: 8pt, text(size: 15pt, weight: "bold", fill: accent, caption))
  line(length: 100%, stroke: 0.8pt + faint)
  v(4pt)
  body
  slide-end
}

// 表紙。識別子・範囲・日付・件数だけを載せる。内容の要約は次の「更新のハイライト」に書く。
#let cover(title: [Ebykn 開発報告], stats: (:)) = context {
  let c = config.get()
  pagebreak(weak: true)
  slide-start("表紙")
  v(1fr)
  text(size: 14pt, fill: muted, weight: "bold", "リリース " + c.release)
  v(2pt)
  text(size: 34pt, weight: "bold", fill: ink, title)
  v(10pt)
  line(length: 40%, stroke: 2pt + accent)
  v(10pt)
  set text(size: size-body)
  grid(
    columns: (auto, 1fr),
    column-gutter: 16pt,
    row-gutter: 9pt,
    text(fill: muted, "範囲"), [#commit(c.base) → #commit(c.head)（`develop`）],
    text(fill: muted, "日付"), c.date,
    ..stats.pairs().map(((k, v)) => (text(fill: muted, k), v)).flatten(),
  )
  v(1fr)
  slide-end
}

// 更新のハイライト。利用者から見て何ができるようになったか、どの誤りが直ったかを、短い箇条で書く。
#let highlights(title: [更新のハイライト], body) = slide(title: title, {
  set text(size: 17pt)
  set list(spacing: 1.1em)
  body
})

// 入った変更の一覧。`entries` は (pr:, issues:, areas:, heading:, title:) の配列。`heading` は `change` と同じ見出しにし、
// 各変更の 1 枚目へのリンクになる。
// `per-page` 行ごとにスライドを分ける。
#let overview-table(entries, size) = {
  set text(size: size)
  table(
    columns: (auto, auto, 1fr, auto),
    stroke: (x, y) => if y == 0 { (bottom: 0.8pt + ink) } else { (bottom: 0.4pt + faint) },
    inset: (x: 6pt, y: 5pt),
    align: (x, y) => if x == 3 { right + horizon } else { left + horizon },
    table.header(
      text(weight: "bold", "PR"),
      text(weight: "bold", "Issue"),
      text(weight: "bold", "見出し（クリックでスライドへ）：内容"),
      text(weight: "bold", "分類"),
    ),
    ..entries
      .map(e => (
        if e.at("pr", default: none) != none { gh-link("pull", e.pr) } else { [—] },
        e.at("issues", default: ()).map(i => gh-link("issues", i)).join(", "),
        {
          let key = change-key(e.at("pr", default: none), e.at("issues", default: ()))
          [#metadata((key: key, heading: plain(e.heading))) <ebykn-entry>]
          link(label(key), text(weight: "bold", fill: accent, e.heading))
          [：]
          e.title
        },
        chips(e.areas),
      ))
      .flatten(),
  )
}

#let overview(entries, title: "入った変更の一覧", per-page: 10, size: size-small) = {
  let chunks = entries.chunks(per-page)
  for (i, chunk) in chunks.enumerate() {
    let t = if chunks.len() == 1 { title } else { title + "（" + str(i + 1) + "/" + str(chunks.len()) + "）" }
    slide(title: t, overview-table(chunk, size))
  }
}

// ---- 文書全体 ----

#let margin-x = 1.6cm

#let deck(release: "", base: "", head: "", date: "", repo: repo-default, body) = {
  set document(title: "Ebykn 開発報告 " + release, author: "ebykn-lang-proto")
  set page(
    paper: "presentation-16-9",
    margin: (x: margin-x, top: 1.3cm, bottom: 1.3cm),
    fill: paper,
    footer: context {
      set text(size: size-min, fill: muted)
      line(length: 100%, stroke: 0.4pt + faint)
      v(-4pt)
      [Ebykn 開発報告 #release]
      h(1fr)
      [#counter(page).display() / #counter(page).final().first()]
    },
    footer-descent: 0.5cm,
  )
  set text(
    font: ("Noto Sans JP",),
    size: size-body,
    lang: "ja",
    fill: ink,
  )
  set par(leading: 0.75em, spacing: 0.9em, justify: false)
  set list(indent: 0.4em, body-indent: 0.5em, spacing: 0.7em, marker: text(fill: accent, "•"))
  set enum(indent: 0.4em, body-indent: 0.5em, spacing: 0.7em)
  show raw: set text(font: mono)
  // raw の既定の大きさ（0.8em）は相対の指定では打ち消せない。ブロックは pt で決め、
  // 行内は外側を 1.15 倍して 0.8 倍と合わせ、地の文の 0.92 倍にする。
  show raw.where(block: true): set text(size: size-code)
  set raw(
    syntaxes: ("eyk.sublime-syntax", "console.sublime-syntax"),
    theme: "code.tmTheme",
  )
  show raw.where(block: false): it => box(
    fill: code-bg,
    inset: (x: 3pt, y: 0pt),
    outset: (y: 3pt),
    radius: 2pt,
    text(size: 1.15em, it),
  )
  show raw.where(block: true): set par(leading: 0.5em)
  show strong: set text(weight: "bold")
  // 箇条書きの項目を数えるための印（list.item は query できないので、list ごとに項目の数だけ残す）。
  show list: it => { for _ in it.children [#metadata("item") <ebykn-item>]; it }
  show enum: it => { for _ in it.children [#metadata("item") <ebykn-item>]; it }
  set table(stroke: 0.4pt + faint)
  config.update((repo: repo, release: release, base: base, head: head, date: date))
  body
  // はみ出しと空のページの検査。`slides-build` が `typst query` で読む。
  context {
    let starts = query(<ebykn-slide-start>)
    let ends = query(<ebykn-slide-end>)
    let overflow = ()
    // スライドごとの密度：箇条書きの項目の数、コードの箱ごとの行数、段組みの中のコードの箱の数。
    let full = (page.width - 2 * margin-x).pt()
    let density = ()
    for (s, e) in starts.zip(ends) {
      let a = s.location().page()
      let b = e.location().page()
      if a != b { overflow.push((title: s.value, from: a, to: b)) }
      let within(x) = query(selector(x).after(s.location()).before(e.location()))
      density.push((
        page: a,
        title: s.value,
        bullets: within(<ebykn-item>).len(),
        codes: within(raw.where(block: true)).map(r => r.text.split("\n").len()),
        narrow: within(<ebykn-codebox>).filter(m => m.value < full * 0.97).len(),
      ))
    }
    [#metadata((
      slides: starts.len(),
      ends: ends.len(),
      pages: counter(page).final().first(),
      overflow: overflow,
      density: density,
      wraps: query(<ebykn-wrap>).map(w => (page: w.location().page(), line: w.value)),
      links: query(<ebykn-link>).len(),
      changes: query(<ebykn-change>).map(m => (page: m.location().page(), key: m.value.key, heading: m.value.heading)),
      entries: query(<ebykn-entry>).map(m => (key: m.value.key, heading: m.value.heading)),
    )) <ebykn-check>]
  }
}
