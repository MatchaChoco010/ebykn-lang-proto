// テンプレートの部品を一通り使った見本。CI の slides ジョブが組み、道具とテンプレートが壊れていないことを確かめる。
// 中身は PR #20 の実例の一部。報告スライドを書くときの書き方の手本にもなる。
#import "/.claude/skills/ebykn-report-slides/template/ebykn-slides.typ": *
#show: deck.with(release: "example", base: "f9cf01f", head: "6d45dbc", date: "2026-11-09")

#cover(stats: ("PR": [1 本], "判断待ち": [1 件]))

#highlights[
  - 権限を二重に解放するプログラムを、Verus にかける前に `ebykn check` が E0504 で拒否する
]

#overview((
  (pr: 20, issues: (17,), areas: ("transpiler", "golden"), heading: [権限の検査], title: [プロジェクトの読み込みと §6.2〜§6.5 の検査、`ebykn init` / `check`]),
))

#change(pr: 20, issues: (17,), areas: ("transpiler", "golden"), heading: [権限の検査], caption: [実例の入力])[
  二重の解放は E0504
  #code-box(title: [`src/lib.eyk`（一部）], ```eyk
pub fn free_twice() {
    let p = Ptr::new(Node { val: 1 });
    let a = p.free();
    let b = p.free();
}
```)
]

#change(pr: 20, issues: (17,), areas: ("transpiler", "golden"), heading: [権限の検査], caption: [実例の出力])[
  #before-after(
    before-title: [修正前（#commit("f9cf01f")）],
    after-title: [修正後（#commit("6d45dbc")）],
    before: code-box(```console
$ ebykn check --lang ja
ebykn: 不明な引数です: check --lang ja
… 使い方（省略）。終了コード 2
```),
    after: code-box(title: [長い行は折り返した], ```console
$ ebykn check --lang ja
src/lib.eyk:9:17: E0504: 権限 `p@` がここではなしなので、
  `.free()` で解放できません
```),
  )
  #note[判断待ち #issue(19) は既定で実装している]
]

#slide(title: [試し方（1/1）：権限の誤り])[
  - `examples/linearity-broken` は、権限の誤りを 2 つ入れた例。`ebykn check` が Verus に渡す前に止める
  #code-box(title: [release のワークスペースで `cargo xtask playground` のあと（848b2c8）。長い行は折り返した], ```console
$ cd playground/linearity-broken
$ ebykn --lang ja check
src/lib.eyk:8:1: E0505: 権限 `p@` をここでまだ所有しています。
  解放・`put`・移動・`leak` のどれかをします（残してよいのは `gives` の戻り値だけです）
src/lib.eyk:19:13: E0504: 権限 `p@` をここでは持っていないので、`.free()` で解放できません
ebykn: 検査が 2 件のエラーで失敗しました
```)
]
