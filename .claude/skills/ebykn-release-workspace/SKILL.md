---
name: ebykn-release-workspace
description: ebykn-lang-proto の release のワークスペース（作者の確認用。release ブランチを取り出したワークツリー）で、作者と話すエージェントの手順。報告スライドについての質問に、Release の PDF とその元のファイル、そのコミットのコード、PR と Issue から答える。作者が直したいことを Issue にしてコーディネーターの Run に知らせ、main へのマージを案内する。release のワークスペースで起動されたとき（`git branch --show-current` が release）に使う。
---

# release のワークスペースで作者と話す

作者は、都合のよいときに release のワークスペースでエージェントを起動し、報告スライドについて質問したり、直したいことを伝えたり、`release` を `main` に入れると決めたりする。このスキルは、そのエージェントの手順。運用は AGENTS.md の「リリースと作者の確認」。

- ここでは実装しない。`release` にコミットせず、push もしない。`develop` と `main` にも直接書かない。直す作業は Issue にしてコーディネーターに渡し、コーディネーターがワーカーに割り振る（手順 3）。
- 作業ツリーのファイルは作者のもの。作者が試しに書いたファイルを、`git stash`・`git checkout -- .`・`git clean`・`git reset` で戻したり消したりしない。作者に頼まれて試しのファイルを書いたり動かしたりするのはよいが、コミットしない。
- 作者にレビューや確認を催促しない。作者が聞いたことに答える。
- コマンドのブロックは `bash -euo pipefail <<'EOF'` … `EOF` で包んで打つ（理由は `ebykn-release` の冒頭）。`<scratchpad>` は実際の値に置き換える。

## 1. 今の状態を知る

最初に一度、状態を調べる。

```bash
bash -euo pipefail <<'EOF'
git fetch origin --prune --tags
git branch --show-current                                   # release のはず
git status --short --untracked-files=all                    # 作者の変更。あっても触らない
git rev-parse HEAD origin/release
git tag --points-at HEAD --list 'report/*'                  # 今の作業ツリーのリリース
gh pr list --base main --head release --state open --json number,url,headRefOid
git tag --list 'report/*' --merged origin/release --no-merged origin/main --sort=v:refname   # 確認待ちのリリース
git log --oneline origin/release..origin/develop | wc -l    # release のあとに develop に入ったコミットの数
gh issue list --label "needs review" --state open --limit 200 --json number,title,url
EOF
```

- ブランチが `release` でなければ、このスキルの対象ではない。作者に伝えて、何をするかを聞く。
- `HEAD` が `origin/release` より古ければ、ワークスペースはまだ前のリリースのまま。作者に伝える。作者が進めてほしいと言い、作業ツリーに変更がなく、`HEAD` が `origin/release` の祖先なら、`git merge --ff-only --no-overwrite-ignore origin/release` で進めてよい。変更があれば進めない。`--no-overwrite-ignore` で、無視されたファイル（`playground/` の下など）を git に黙って上書きさせない（`ebykn-release` の手順 7 と同じ）。
- 作者がすぐ試せる例は `playground/` にある（リリースのたびに `ebykn-release` の手順 7 が `cargo xtask playground` で写す。使い方は `examples/README.md`）。`export PATH="$PWD/target/debug:$PATH"` のあと、各例の README のコマンドを打つ。`playground/` は git が追跡しないので、作者が自由に書き換えてよい。進めたあとに新しい例を足すときは `cargo xtask playground` を打つ（すでにあるファイルは上書きしない）。

## 2. 質問に答える

答えは、次のものから引く。答えには根拠（ファイルとコミット、PR や Issue の番号）を添える。分からないことは分からないと言う。

| 何を聞かれたか | 見るもの |
| --- | --- |
| スライドに何が書いてあるか、なぜそう書いたか | その回の Release に添付した PDF とその元のファイル |
| 何が入ったか、範囲 | Release の本文、タグのメッセージ、リリースの PR の本文とコメント |
| コードがどうなっているか | そのタグのコミットのコード |
| なぜその変更をしたか、何を決めたか | 範囲の PR の本文とコメント、閉じた Issue、`OPEN.md`、`docs/current-spec.md` |

```bash
bash -euo pipefail <<'EOF'
id=<識別子。例：2026-11-10.1>
d=<scratchpad>/report-$id
mkdir -p "$d/assets" "$d/src"
gh release view "report/${id}"                                  # 本文（範囲・PR・閉じた Issue）
gh release download "report/${id}" --dir "$d/assets" --clobber   # PDF と元のファイル
for t in "$d"/assets/*.tar.gz; do if [ -e "$t" ]; then tar -xzf "$t" -C "$d/src"; fi; done
git tag --list --format='%(contents)' "report/${id}"            # 前回と今回のコミット
ls -R "$d"
EOF
```

- 作業ツリーの `HEAD` が聞かれたリリースのタグなら、作業ツリーのファイルを読む。違うタグなら、作業ツリーを切り替えずに `git show report/<id>:<パス>` や `git grep <語> report/<id>` で読む。
- PR と Issue は `gh pr view <番号> --comments`、`gh issue view <番号> --comments` で読む。
- 動かして確かめるときは、AGENTS.md の「コマンド」にあるものを使う。作業ツリーの追跡しているファイルは変えない。
- `develop` がリリースのあとに進んでいて、聞かれたことに関わる変更があれば、それを分けて伝える（`git log origin/release..origin/develop`）。まだどのリリースにも入っていない。

## 3. 作者が直したいことを言ったら

実装はしない。Issue にして、コーディネーターに知らせる。

1. 何を直したいかを 1、2 文で言い直し、作者に合っているかを確かめる。
2. `ebykn-issue` スキルで Issue にする。本文に、見つけた経路（作者の確認、release のワークスペース）と、どのリリース（`report/<id>`）で見つかったかを書く。作者がその場で直し方を決めたら、その決定を本文に書く。作者が決めかねていて判断が要るなら、`needs review` にする。
3. 作者が、開いている `needs review` の Issue に判断を出したら、その Issue に判断をコメントで残し、`needs review` のラベルを外す。新しい Issue は作らなくてよい。
4. コーディネーターの Run の id を確かめる。id は、毎朝のリリースのオートメーションのプロンプトに書いてある（`ebykn-release` の「毎朝のリリースのオートメーション」）。

```bash
bash -euo pipefail <<'EOF'
orca=${ORCA_CLI_COMMAND:-orca}
: > <scratchpad>/coordinator-run
aid=$("$orca" automations list --json \
  | jq -r '[.. | objects | select(.name? == "ebykn-daily-release") | .id] | unique | .[]')
[ "$(printf '%s\n' "$aid" | grep -c .)" = 1 ] \
  || { echo "Run を確かめられない：オートメーション ebykn-daily-release が 1 つに決まらない"; exit 1; }
run=$("$orca" automations show "$aid" --json | jq -r '.. | strings' \
  | sed -n 's/^コーディネーターの Run の id：\(run_[0-9A-Za-z_]*\)$/\1/p' | head -n1)
[ -n "$run" ] || { echo "Run を確かめられない：プロンプトに Run の id がない"; exit 1; }
"$orca" orchestration run-show --id "$run" --json --full | jq -e '.result.run.legacy == 0' > /dev/null \
  || { echo "Run を確かめられない：$run が見つからないか、legacy の Run"; exit 1; }
echo "$run" > <scratchpad>/coordinator-run
cat <scratchpad>/coordinator-run
EOF
```

   - 確かめられなければ送らない。Issue の番号を作者に伝え、コーディネーターに伝えてもらう。
   - `automations list` と `show` の JSON の形は、オートメーションを作ったあとに確かめる（`ebykn-release` の「作ったあとに確かめる」）。形が違って id を読めなければ、このブロックを直す PR を出すよう、作者に伝える。
5. コーディネーターに知らせる。Issue を作ったとき：

```bash
bash -euo pipefail <<'EOF'
orca=${ORCA_CLI_COMMAND:-orca}
run=$(cat <scratchpad>/coordinator-run)
"$orca" orchestration send --to "run:${run}" \
  --subject "作者の確認から：#<番号> <題>" \
  --body "release のワークスペースで、作者が report/<id> を確かめて伝えたことを Issue にしました。
- Issue：#<番号> <題>（<種類>、<優先度>）
- 作者の言葉：<要約>
- 作者が決めたこと：<なければ「なし」>
- 判断を出した needs review の Issue：#<番号>（なければ「なし」）
直す作業の割り振りを頼みます。"
EOF
```

   作者が `needs review` の Issue に判断を出しただけのとき（新しい Issue はない）：

```bash
bash -euo pipefail <<'EOF'
orca=${ORCA_CLI_COMMAND:-orca}
run=$(cat <scratchpad>/coordinator-run)
"$orca" orchestration send --to "run:${run}" \
  --subject "作者の判断：#<番号> <題>" \
  --body "release のワークスペースで、作者が needs review の Issue に判断を出しました。
- Issue：#<番号> <題>
- 判断：<作者が選んだ案と、その言葉の要約>
- 判断待ちの既定との違い：<既定のままでよい / 既定と違う：作り直しが要る>
Issue に判断をコメントし、needs review のラベルを外しました。既定と違えば、作り直しの割り振りを頼みます。"
EOF
```

6. 作者に、Issue の番号と、コーディネーターに知らせたことを伝える。直したものは、マージされたあとの次の朝のリリースに入る。急ぐなら、コーディネーターに手でリリースを切ってもらえると伝える。

## 4. main へのマージ

作者が `release` を `main` に入れると決めたら、マージの前に確かめる。

```bash
bash -euo pipefail <<'EOF'
git fetch origin --prune --tags
pr=$(gh pr list --base main --head release --state open --json number --jq '.[0].number // empty')
[ -n "$pr" ] || { echo "止まる：開いた release → main の PR がない"; exit 1; }
body_sha=$(gh pr view "$pr" --json body --jq .body \
  | sed -n 's/.*--match-head-commit \([0-9a-f]\{40\}\).*/\1/p' | head -n1)
[ -n "$body_sha" ] || { echo "止まる：本文の「マージのしかた」に sha がない"; exit 1; }
head=$(gh pr view "$pr" --json headRefOid --jq .headRefOid)
echo "PR #$pr の本文の sha：$body_sha"
echo "PR #$pr の先頭：      $head"
[ "$body_sha" = "$head" ] || { echo "止まる：本文の sha と PR の先頭が違う"; exit 1; }
tags=$(git tag --points-at "$head" --list 'report/*')
[ -n "$tags" ] || { echo "止まる：PR の先頭に report/* のタグがない"; exit 1; }
echo "先頭のタグ：$tags"
state=$(gh pr checks "$pr" --json name,state --jq '.[] | select(.name == "ci-required") | .state')
[ "$state" = SUCCESS ] || { echo "止まる：ci-required が緑ではない（${state:-ない}）"; exit 1; }
echo "マージできる：gh pr merge $pr --merge --match-head-commit $head"
EOF
```

- 「止まる」が出ずに「マージできる」が出たら、作者に次のコマンドを示す。

```bash
gh pr merge <PR 番号> --merge --match-head-commit <本文の sha 40 桁>
```

- 作者が実行してと言ったら、そのとおり実行する。マージコミットでマージする。squash や rebase はしない（次のリリースの PR に同じ変更がもう一度出る）。
- 本文の sha と PR の先頭が違ったら、マージしない。リリースを切っている途中（`release` を進めたが、本文をまだ直していない）か、本文が古い。作者に伝え、リリースが終わってから（本文が直ってから）もう一度確かめる。
- `ci-required` が緑でなければ、マージしない。作者に伝える。
- マージしたあとのことは、次のリリースの `ebykn-release` が受け持つ（「作者が main にマージしたあと」）。ここでは何もしない。
