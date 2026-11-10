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
ebykn-limited cargo xtask ci    # ebykn-limited がない環境では cargo xtask ci
```

重い処理は AGENTS.md の「マシンの資源」のとおり、`ebykn-limited` の下で 1 つずつ走らせる。変えたクレートだけを確かめるとき（`ebykn-limited cargo test -p <クレート>`）も同じ。`cargo xtask ci` は PR の前に 1 回通せば足りる。

落ちたら直す。ゲートを外したり、テストを `#[ignore]` にしたりして通さない。

`vendor/` に手を入れたら、`vendor/` の変更だけのコミットにし、`vendor/` の外の変更と混ぜない（AGENTS.md の「外部から取り込むコード」）。混ぜると `check-vendor-commits` が落ちるので、rebase でコミットを分けて push し直す。pre-commit フックは作っているコミットの直前までの履歴を見るので、混ぜたコミットは、手元では次のコミットのときに、そうでなければ CI で落ちる。本文には、`vendor/` を変えるコミットを含むので、マージコミットでマージする必要があると書く。

### 書き下しが変わったとき

書き下し（`ebykn_lower`）を変えると、golden の書き下しの期待（`tests/golden/*/expected/verus/`）が合わなくなり、`lowered_verus_matches_expected` と `every_blessed_expectation_matches_the_lowering` が落ちる。期待は手で直さず、生成物から作り直す。

```bash
ebykn-limited cargo xtask golden-bless            # 全ケース。ケースを挙げれば、そのケースだけ（golden-bless stack drop）
git diff -- tests/golden                          # 意図した変更だけか読む
ebykn-limited cargo test -p ebykn --test golden   # 作り直した期待が比較を通る
ebykn-limited cargo xtask golden-verify           # Verus の結果が expected/status.txt と一致する
```

- 差分は 1 行ずつ読む。変えたつもりのない形が変わっていたら、作り直した期待をコミットせず、書き下しの方を直す。期待を作り直すと、どんな出力でも比較は通る。差分を読むのが検査になる。
- `golden-verify` は Verus が要る（`cargo xtask verus-install`）。`golden-verify` は `status.txt` を書き換えず、結果が違えば落ちる。検証の結果の変化が意図どおりなら、`status.txt` を手で直し、何がなぜ変わったかを PR 本文に書く。`golden-bless` は `status.txt` を作り直さない。
- `golden-bless` は `Cargo.lock` に触らない。依存の版を変えたときは、別に作り直す（`OPEN.md` の依存の版の固定）。
- 期待は生成物そのもの（バイト単位で同じ）にし、コメントを書かない。ケースの説明は、ケースの `README.md`（`tests/golden/stack/README.md` など）に書く。期待にコメントを足すと、`every_blessed_expectation_matches_the_lowering` が落ちる。作り直すと消えるコメント（生成物に同じ本文がないもの）が期待にあれば、`golden-bless` は何も書かずに、その位置と本文を挙げて止まる。そのときはコメントを `README.md` に移してから作り直す。

新しいケースを足すときは、`tests/golden/<名前>/` に `Ebykn.toml`・`src/`・`expected/diag.txt` を置いてから、次のようにする。

```bash
ebykn-limited cargo xtask golden-bless <名前>      # expected/verus/ の src/ と Cargo.toml を作る
ebykn-limited cargo generate-lockfile --manifest-path tests/golden/<名前>/expected/verus/Cargo.toml   # 依存の版を固定する
```

- `Cargo.lock` の vstd が固定した版であることは `check-verus-pin` が確かめる。
- 検証の結果の期待（`expected/status.txt`）は、`ebykn-limited cargo build -p ebykn` のあと、ケースのディレクトリで `ebykn-limited <リポジトリ>/target/debug/ebykn verify` を走らせ、出力を読んで意図どおりなら、その出力をそのまま書く。書き出した `build/verus/` はコミットしない。

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

`ci-required` が緑になるまで、落ちたら直して push する。

`develop` の ruleset は strict で、PR の head が `develop` の先頭に追いついていなければ、緑でもマージできない（AGENTS.md の「作業の場所」）。

- `develop` と衝突したら（`mergeStateStatus` が `DIRTY`）、`develop` に rebase して push し直す（feature ブランチなので `--force-with-lease` でよい）。push のたびに CI が走り直すので、もう一度 `gh pr checks <番号> --watch` で `ci-required` が緑になるまで見届けてから報告する。前の緑は、新しい head には効かない。
- 衝突がなく、`develop` に遅れているだけなら、ワーカーは何もしなくてよい。マージのときに、コーディネーターが update-branch で追いつかせる（下の 6）。
- コーディネーターが update-branch をしたあとで直しを頼まれたら、リモートの feature ブランチに `develop` を取り込んだマージのコミットが足されている。手元を `git pull --ff-only` で合わせてから直すか、今の `develop` に rebase して `--force-with-lease` で push する。そのマージのコミットを含まず、今の `develop` に rebase してもいないブランチを push すると、追いつかせたのが取り消され、PR は `BEHIND` に戻り、予約した自動マージは知らせなしで待ち続ける。

## 6. マージしない

ワーカーは PR をマージしない。PR の URL と CI の結果を報告して終える。

マージはコーディネーターが、作者の承認を待たずに行う。`ci-required` が緑で、コーディネーターが起動した別のサブエージェントのコードレビューで指摘が残っていないものを `develop` に squash でマージする（`vendor/` を変えるコミット、つまり取り込みか手入れのコミットを含む PR はマージコミット）。レビューの指摘はワーカーに送られるので、直して push し、再レビューを受ける。自動マージを予約したあとでワーカーに PR を戻すときは、コーディネーターが先に予約を外す（下）。詳しくは AGENTS.md の「マージ」。

コーディネーターは、PR の head を `develop` の先頭に追いつかせてから、自動マージを予約する。

```bash
pr=<PR 番号>
gh pr view "$pr" --json mergeStateStatus,headRefOid   # BEHIND：develop に遅れている。DIRTY：衝突がある
gh pr update-branch "$pr"             # BEHIND のときだけ打つ。DIRTY ならワーカーに rebase を頼む
gh pr view "$pr" --json headRefOid --jq .headRefOid   # update-branch は非同期。最初の headRefOid から変わるまで打ち直す
head=$(gh pr view "$pr" --json headRefName --jq .headRefName)
git fetch -q origin develop "$head"
git rev-list --count --no-merges --full-history "origin/develop..origin/$head" -- vendor/   # 0 でなければ vendor/ を変えるコミットを含む
gh pr merge "$pr" --squash --auto     # vendor/ を変えるコミットを含む PR は --merge --auto
gh pr checks "$pr" --watch            # update-branch のあとの CI
gh pr view "$pr" --json state --jq .state   # MERGED になってから片付ける
```

- 衝突（`DIRTY`）の PR は、update-branch に頼らず、ワーカーに `develop` への rebase を頼む。
- update-branch は、GitHub がマージのコミットを作るのを待たずに戻る。head が変わる前に `gh pr checks --watch` を打つと、古い head の緑を見て終わることがある。`headRefOid` が変わってから見る。
- 自動マージを予約したあとでワーカーに PR を戻すとき（レビューの指摘の直しや rebase を頼むとき）は、先に予約を外す。どのエージェントも同じアカウントなので、予約したままワーカーが直しを push すると、GitHub は予約を外さず、`ci-required` が緑になった時点で再レビューなしにマージする。

```bash
gh pr merge "$pr" --disable-auto      # ワーカーに戻す前に。再レビューで指摘が残っていなければ、上の手順で予約し直す
```

- 自動マージは、`ci-required` が緑で、head が `develop` の先頭に追いついているときに GitHub が行う。待つあいだに `develop` が進むと、もう一度 update-branch が要る。
- マージした head のブランチは GitHub が消す。コーディネーターはリモートのブランチを消さない。状態が `MERGED` になる前にブランチを消すと、PR が閉じてしまう。

マージしたら、コーディネーターは PR の `closingIssuesReferences` の Issue が閉じたかを確かめ、閉じていなければコメントして閉じる。

```bash
pr=<PR 番号>
gh pr view "$pr" --json closingIssuesReferences --jq '.closingIssuesReferences[].number' \
  | while read -r n; do
      [ "$(gh issue view "$n" --json state --jq .state)" = CLOSED ] \
        || gh issue close "$n" --comment "PR #$pr で develop に入った"
    done
```
