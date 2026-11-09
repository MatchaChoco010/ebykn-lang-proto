---
name: ebykn-pr
description: ebykn-lang-proto で PR を作る手順。ゲートを通し、日本語の本文を yomiyasu（--doc-type pr）で推敲してから gh pr create し、CI が緑になるまで見届ける。PR を作るとき、PR 本文を書くときに使う。
---

# PR を作る

前提：作業は orca のワークツリーで行い、1 つの Issue に 1 つの PR にする（AGENTS.md）。ワークツリー名は `<種類>-<Issue 番号>-<短い名前>` で、ブランチ名は orca が付ける `<GitHub ユーザー名>/<ワークツリー名>`（`git branch --show-current` で分かる）をそのまま使う。

## 1. ゲートを通す

```bash
cargo xtask ci
```

落ちたら直す。ゲートを外したり、テストを `#[ignore]` にしたりして通さない。

## 2. 本文を書く

日本語で書き、ファイルに保存する（例：scratchpad の `pr-body.md`）。結論を先に書き、次を入れる。

- 変更点。指示にないファイルを足したら、何のファイルかを書く。
- 確認したこと（実行したコマンドと結果）。
- 決めた既定と理由（あれば）。`OPEN.md` や `needs review` の Issue に記録したものは番号を挙げる。
- 関連 Issue。`Closes #N` で紐づける。閉じない Issue は閉じる動詞の後に `#N` を書かない。

## 3. yomiyasu で推敲する（必須）

`yomiyasu:yomiyasu` スキルを呼び、PR 向けの設定 `--doc-type pr` で推敲する。スキルの手順どおり lint → 書き直し → `run --final <ファイル> --doc-type pr` → `verify` まで通す。verify を通した最終稿は、そのあと変えない。

## 4. PR を作る

```bash
git push -u origin <ブランチ>
gh pr create --base main --head <ブランチ> --title "<日本語の題>" --body-file <最終稿>
```

## 5. CI を見届ける

```bash
gh pr checks <番号> --watch
```

`ci-required` が緑になるまで、落ちたら直して push する。

## 6. マージしない

マージ方針は #2 で判断待ち。判断が出るまで、エージェントは PR をマージしない。PR の URL と CI の結果を報告して終える。
