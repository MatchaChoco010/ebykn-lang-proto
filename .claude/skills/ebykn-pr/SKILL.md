---
name: ebykn-pr
description: ebykn-lang-proto で PR を作る手順。ゲートを通し、日本語の本文を yomiyasu（--doc-type pr）で推敲してから develop へ gh pr create し、CI が緑になるまで見届ける。PR を作るとき、PR 本文を書くときに使う。
---

# PR を作る

前提：作業は orca のワークツリーで行い、1 つの Issue に 1 つの PR にする（AGENTS.md）。ワークツリーの元は `origin/develop`、ワークツリー名は `<種類>-<Issue 番号>-<短い名前>`。

## 0. ブランチ名を変える

orca はブランチ名に `<GitHub ユーザー名>/` を付けるので、作業の最初に `feature/<Issue 番号>-<短い名前>` に変える。

```bash
git log -1 --oneline; git log -1 --oneline origin/develop   # 元が origin/develop か確かめる
git branch -m feature/<Issue 番号>-<短い名前>
```

`main`・`release`・`develop` には直接コミットしない。

## 1. ゲートを通す

```bash
cargo xtask ci
```

落ちたら直す。ゲートを外したり、テストを `#[ignore]` にしたりして通さない。

## 2. 本文を書く

日本語で書き、ファイルに保存する（例：scratchpad の `pr-body.md`）。結論を先に書き、次を入れる。

- 変更点。指示にないファイルを足したら、何のファイルかを書く。
- 確認したこと（実行したコマンドと結果）。
- 決めた既定と理由（あれば）。`OPEN.md` や `needs review` の Issue に記録したものは番号を挙げる。`docs/current-spec.md` を変えたら、変えた項目を挙げる。
- 関連 Issue。`Closes #N` で紐づける。閉じない Issue は閉じる動詞の後に `#N` を書かない。

本文は、リリースの報告スライドを作るときにも読まれる。何を直したか・足したか、なぜ必要だったかを、文脈を知らない人にも分かるように書く。

## 3. yomiyasu で推敲する（必須）

`yomiyasu:yomiyasu` スキルを呼び、PR 向けの設定 `--doc-type pr` で推敲する。スキルの手順どおり lint → 書き直し → `run --final <ファイル> --doc-type pr` → `verify` まで通す。verify を通した最終稿は、そのあと変えない。

## 4. PR を作る

ベースは `develop`。

```bash
git push -u origin feature/<Issue 番号>-<短い名前>
gh pr create --base develop --head feature/<Issue 番号>-<短い名前> --title "<日本語の題>" --body-file <最終稿>
gh pr view <番号> --json closingIssuesReferences --jq '[.closingIssuesReferences[].number]'
```

既定のブランチは `develop` なので `--base` を省いても `develop` に向くが、明示する。本文の `Closes #N` の Issue が `closingIssuesReferences` に載っていることを確かめる。載っていなければ、本文の書き方（閉じる動詞と `#N` のあいだ）を直して `gh pr edit` する。

## 5. CI を見届ける

```bash
gh pr checks <番号> --watch
```

`ci-required` が緑になるまで、落ちたら直して push する。`develop` と衝突したら `develop` に rebase して push し直す（feature ブランチなので `--force-with-lease` でよい）。

## 6. マージしない

ワーカーは PR をマージしない。PR の URL と CI の結果を報告して終える。

マージはコーディネーターが、作者の承認を待たずに行う。`ci-required` が緑で、コーディネーターが起動した別のサブエージェントのコードレビューで指摘が残っていないものを `develop` に squash でマージする（取り込みのコミットを含む PR はマージコミット）。レビューの指摘はワーカーに送られるので、直して push し、再レビューを受ける。詳しくは AGENTS.md の「マージ」。

マージしたら、コーディネーターは PR の `closingIssuesReferences` の Issue が閉じたかを確かめ、閉じていなければコメントして閉じる。

```bash
pr=<PR 番号>
gh pr view "$pr" --json closingIssuesReferences --jq '.closingIssuesReferences[].number' \
  | while read -r n; do
      [ "$(gh issue view "$n" --json state --jq .state)" = CLOSED ] \
        || gh issue close "$n" --comment "PR #$pr で develop に入った"
    done
```
