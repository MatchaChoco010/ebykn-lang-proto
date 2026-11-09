---
name: ebykn-release
description: ebykn-lang-proto でリリースを切る手順。ワークツリーを最新にしてコーディネーターの Run を確かめ、develop の先頭が緑か確かめ、範囲の PR と Issue を集めて報告スライドの PDF を作り、タグ report/YYYY-MM-DD.N と Release を出してから release を早送りし、作者の確認用の release のワークスペースを進め、release → main の PR を更新して、結果をコーディネーターの Run に知らせる。作者には催促しない。毎朝 6 時のオートメーションが起こすエージェントが使う。コーディネーターが手で切るとき（作者が見つけた問題を直したあと）、途中で止まったリリースを再開するとき、毎朝のオートメーションを作るとき、止まった回を続けるとき、知らせの届かなかった回を見回るとき、作者が main にマージしたあとにも使う。
---

# リリースを切る

毎朝 6:00（日本時間）に Orca のオートメーションが起こすエージェントが、`origin/develop` から作った新しいワークツリーで行う（オートメーションの作り方は末尾の「毎朝のリリースのオートメーション」）。コーディネーターが手で切るときも同じ手順で、手順 0 と手順 9 の送信だけを飛ばす。ブランチの役割と作者の確認は AGENTS.md の「作業の場所」と「リリースと作者の確認」。

- この手順で「止まって報告する」は、手順 9 の「止まったとき」の形でコーディネーターの Run に知らせて終わること。作者には報告しない。レビューや確認を催促しない。結果は Release と、リリースの PR の本文とコメントに残す。
- 取り消せない操作（タグの push、Release、`release` の早送り）は、確認と PDF が済んでから手順 4〜6 でまとめて行う。手順 1〜3 で止まっても、リモートには何も残らない。
- `release` は早送りだけで進める。force push しない。直接コミットしない。`release` の先頭は、いつも Release のある報告済みのタグにする。
- 押したタグと Release は消さない。手順 6 までのどこかで止まったら、手順 1 からやり直す。手順 1 が、止まったリリースを見つけて同じ識別子で再開する。手順 7 以降で止まったら、その手順からやり直す（`release` はもうタグまで進んでいるので、手順 1 は新しいリリースとみなす）。止まった点ごとの続け方は「止まった回を続ける」。
- どの手順でも、コマンドが権限の判定で拒否されたり確認を求められたりしたら、そこで止まる。別の方法で回り込まない。取り消せない操作（タグと `release` の push、`gh release create`、`gh pr create`・`gh pr edit`・`gh pr comment`）だけでなく、手順 0〜3 で使うもの（スライドの道具のダウンロード、`git worktree add`、yomiyasu など）も同じ。手順 9 の「止まったとき」の形で、止まった手順・`$dir`・PDF のパス（まだなければ「なし」）を知らせる。残りの手順は、コーディネーターが「止まった回を続ける」の表に従って行う。コーディネーターは、同じマシンの `$dir` を読める。
- 例外は、見つけた問題の `gh issue create` が拒否されたとき。作る予定だった Issue の題と本文を `$dir/pending-issues/<番号>.md`（1 から数える）に残し、手順 9 の知らせに書く。リリースは止めない。
- コマンドのブロックは、どれも `bash -euo pipefail <<'EOF'` … `EOF` で包んで打つ。Bash ツールのシェルは zsh のことがあり、zsh では `"$sha:refs/..."` の `:r` が修飾子として展開されるため。ブロックの中でも、`:` の前の変数は `${sha}` のように括る。
- シェルの変数はブロックをまたいで残らないので、手順 1 で作る `vars` を各ブロックの最初に読み込む。`<scratchpad>` と `<id>` は実際の値に置き換える。

## 0. ワークツリーを最新にし、コーディネーターの Run を確かめる

オートメーションのエージェントだけが行う。コーディネーターが手で切るときは飛ばす。

Orca は、ワークツリーを作る前の `git fetch` の結果を待たず、確かめもしない。そのため、ワークツリーが `origin/develop` より古いことがある。まず最新にし、スキルが変わっていたら読み直してから進む。

```bash
bash -euo pipefail <<'EOF'
git fetch origin --prune --tags
if [ "$(git rev-parse HEAD)" != "$(git rev-parse origin/develop)" ]; then
  git merge --ff-only origin/develop \
    || { echo "止まる：このワークツリーを origin/develop まで早送りできない"; exit 1; }
  echo "origin/develop まで早送りした。.claude/skills/ebykn-release/SKILL.md を読み直してから進む"
fi
git log -1 --oneline
EOF
```

次に、結果を知らせる先を確かめる。Run の id は、コーディネーターがオートメーションのプロンプトに書き込んである（「毎朝のリリースのオートメーション」）。`orca orchestration run-list` は、すべてのリポジトリの Run の一覧で、Run は消せないので、一覧から推して選ばない。

```bash
bash -euo pipefail <<'EOF'
orca=${ORCA_CLI_COMMAND:-orca}
run=<プロンプトにある Run の id>
out=<scratchpad>/run-show.json
: > <scratchpad>/coordinator-run
if ! "$orca" orchestration run-show --id "$run" --json --full > "$out"; then
  echo "Run を確かめられない：run-show が失敗した"; cat "$out"; exit 1
fi
[ "$(jq -r '.ok' "$out")" = true ] || { echo "Run を確かめられない：run-show が失敗した"; cat "$out"; exit 1; }
[ "$(jq -r '.result.run.id' "$out")" = "$run" ] || { echo "Run を確かめられない：id が違う"; jq -c '.result.run' "$out"; exit 1; }
[ "$(jq -r '.result.run.legacy' "$out")" = 0 ] || { echo "Run を確かめられない：legacy の Run"; jq -c '.result.run' "$out"; exit 1; }
echo "$run" > <scratchpad>/coordinator-run
jq -c '.result.run | {id, objective}' "$out"
EOF
```

- 確かめられたら、`coordinator-run` に id が入る。手順 9 はそこへ送る。
- 確かめられなくても（失敗が出て `coordinator-run` が空のまま）、リリースは止めずに手順 1 から進める。知らせが届かないことを目に見えるようにするため、手順 8 の PR のコメントに「知らせ：送れない（理由）」の行を足し、手順 9 では送らずに知らせの中身をこのターミナルに出す。コーディネーターは「見回り」で、この回を見つける。
- Run の id が古くなる（コーディネーターが替わった）と、ここで確かめられなくなる。直し方は「毎朝のリリースのオートメーション」の「コーディネーターが替わったら」。

## 1. 識別子と範囲を決め、CI を確かめる

```bash
bash -euo pipefail <<'EOF'
git fetch origin --prune --tags

# 前提：既定のブランチが develop（手順 2 の commits/<sha>/pulls が PR を返す条件）
default=$(gh repo view --json defaultBranchRef --jq .defaultBranchRef.name)
[ "$default" = develop ] || { echo "止まる：既定のブランチが develop ではなく $default"; exit 1; }

# 前回のリリースの PR が main にマージコミットで入ったか
last=$(gh pr list --base main --head release --state merged --limit 1 --json headRefOid --jq '.[0].headRefOid // empty')
[ -z "$last" ] || git merge-base --is-ancestor "$last" origin/main \
  || { echo "止まる：前回のリリースの PR が main にマージコミットで入っていない"; exit 1; }

latest=$(git tag --list 'report/*' --sort=-v:refname | head -n1)

# 手元にだけあるタグ（手順 4 の push の前か途中で止まった回のもの）は消す。リモートにないので、何も押していない
while [ -n "$latest" ]; do
  rc=0; git ls-remote --exit-code origin "refs/tags/${latest}" > /dev/null || rc=$?
  [ "$rc" != 0 ] || break
  [ "$rc" = 2 ] || { echo "止まる：git ls-remote が失敗した（終了コード $rc）"; exit 1; }
  echo "手元にだけある ${latest} を消す"
  git tag -d "$latest"
  latest=$(git tag --list 'report/*' --sort=-v:refname | head -n1)
done

# 止まったリリースがあれば、同じ識別子で再開する
mode=new
if [ -n "$latest" ]; then
  if err=$(gh release view "$latest" 2>&1 > /dev/null); then
    if ! git merge-base --is-ancestor "$latest" origin/release 2> /dev/null; then
      mode=resume-ff        # Release はあるが release が進んでいない：手順 6 から
    fi
  elif [ "$err" = "release not found" ]; then
    mode=resume-release     # タグはあるが Release がない：手順 5 から
  else
    echo "止まる：gh release view $latest が失敗した：$err"; exit 1
  fi
fi

if [ "$mode" = new ]; then
  sha=$(git rev-parse origin/develop)
  today=$(TZ=Asia/Tokyo date +%F)
  id="$today.$(( $(git tag --list "report/$today.*" | wc -l) + 1 ))"
  prev=${latest:-origin/main}   # 初回は main から
  prev_sha=$(git rev-parse "${prev}^{commit}")
  [ "$prev_sha" != "$sha" ] || { echo "止まる：前回のリリースから develop が進んでいない"; exit 1; }
else
  id=${latest#report/}
  sha=$(git rev-parse "${latest}^{commit}")
  msg=$(git tag --list --format='%(contents)' "$latest")
  prev=$(printf '%s\n' "$msg" | sed -n 's/^前回：\(.*\)（[0-9a-f]\{40\}）$/\1/p')
  prev_sha=$(printf '%s\n' "$msg" | sed -n 's/^前回：.*（\([0-9a-f]\{40\}\)）$/\1/p')
  [ -n "$prev_sha" ] || { echo "止まる：$latest のメッセージに前回のコミットがない"; exit 1; }
fi
git merge-base --is-ancestor "$prev_sha" "$sha" || { echo "止まる：$prev が $sha の祖先ではない"; exit 1; }

dir=<scratchpad>/release-$id   # 前の回の $dir を続けるときは、そのパス（「止まった回を続ける」）
# 新しく切るときは、前に止まった回の出力（古い PDF など）を消す。リモートにはまだ何もない
[ "$mode" != new ] || rm -rf "$dir"
mkdir -p "$dir"
printf 'mode=%s\nid=%s\nsha=%s\nprev=%s\nprev_sha=%s\ndir=%s\n' \
  "$mode" "$id" "$sha" "$prev" "$prev_sha" "$dir" > "$dir/vars"
cat "$dir/vars"
EOF
```

- 「止まる」が出たら（`exit 1` で終わる）、そこで止まって報告する。`develop` が進んでいなければ、リリースは切らない。これは失敗ではなく、何もすることがなかったという結果として手順 9 で知らせる。
- 既定のブランチが `develop` でなければ止まる。GitHub の `commits/<sha>/pulls` は、既定のブランチの上のコミットにしか、マージ済みの PR を返さない。既定のブランチが変わると、手順 2 で範囲のすべてのコミットが「PR のないコミット」に見える。
- 前回のリリースの PR が `main` に squash か rebase で入っていたら止まる。次の PR の差分に同じ変更がもう一度出る。作者の操作が要るが、この時点ではリリースの PR がない（前回の PR は閉じ、今回の PR はまだ作っていない）ので、手順 9 の知らせにだけ書く。コーディネーターは、作者が次に話しかけたときに伝える（催促はしない）。
- 手元にだけある `report/*` のタグ（`git ls-remote --exit-code` が 2 で終わる）は消す。手順 4 の push が失敗したか、権限で止まった回のタグで、リモートには何も残っていない。そのまま残すと、`git fetch --prune --tags` でも消えず、次の回の手順 1 が `resume-release` を選び、手順 5 の `--verify-tag` で毎回止まる。`ls-remote` がほかの理由で失敗したら（通信など）、消さずに止まる。
- `gh release view` の失敗は、`release not found` のときだけ Release がないとみなす。通信の失敗などほかのエラーでは止まり、再開と取り違えない。
- 新しく切るとき（`mode=new`）は、作業用のフォルダを消してから作る。前に手順 3 までで止まった回の出力を使わないため。
- `mode` で、どこから進めるかが決まる。

| `mode` | 状態 | 進める手順 |
| --- | --- | --- |
| `new` | 新しいリリース | 下の CI の確認、手順 2 から |
| `resume-release` | タグは押したが、Release がない | 手順 2・3 の出力が `$dir` になければ作り直し、手順 4 を飛ばして 5 から |
| `resume-ff` | Release はあるが、`release` がタグまで進んでいない | 手順 2 の出力が `$dir` になければ作り直し、手順 6 から |

- 以降は、ここで決めた `$sha` だけを使う。途中で `develop` が進んでも、今回のリリースには入れない。
- 初回の `prev` は `origin/main` だが、作者がマージすると `origin/main` は動く。手順 4 でタグのメッセージに `prev_sha` を残し、再開のときはそこから読む。

`mode=new` のときは、`$sha` で push の CI が緑かを確かめる（再開のときは、タグを押す前に確かめてある）。

```bash
bash -euo pipefail <<'EOF'
. <scratchpad>/release-<id>/vars
gh run list --workflow ci.yml --event push --branch develop --commit "$sha" \
  --json databaseId,status,conclusion
EOF
```

| 結果 | すること |
| --- | --- |
| `completed` / `success` | 進む |
| `queued` / `in_progress` | `gh run watch <databaseId> --exit-status` で待ち、もう一度確かめる |
| 空 | push の CI がまだ始まっていない。数分待って、もう一度確かめる |
| `cancelled` | 新しい push で取り消された。手順 1 からやり直す（`$dir` を消してよい。リモートには何も残っていない） |
| それ以外（`failure` など） | 止まって報告する。直す PR を `develop` に積み、緑になってから切る |

PR の CI は、ほかの PR が先にマージされたあとの組み合わせを試していないので、この確認を飛ばさない。

## 2. 範囲の PR と Issue を集める

`develop` の first-parent の各コミットについて、GitHub に、そのコミットでマージされた PR を聞く（`commits/<sha>/pulls` のうち `merge_commit_sha` がそのコミットのもの）。ベースが `main` のころに入った PR も、件数の上限なく取れる。GitHub がマージ済みの PR を返すのは既定のブランチの上のコミットだけなので、既定のブランチが `develop` であることが前提になる（手順 1 で確かめる）。コミットの題の `(#N)`（squash）と `Merge pull request #N`（取り込みの PR のマージコミット）とも突き合わせる。

```bash
bash -euo pipefail <<'EOF'
. <scratchpad>/release-<id>/vars
repo=$(gh repo view --json nameWithOwner --jq .nameWithOwner)
git log --first-parent --reverse --format='%H %s' "$prev_sha..$sha" > "$dir/commits.txt"
: > "$dir/prs.txt"; : > "$dir/direct.txt"; : > "$dir/mismatch.txt"
while read -r c s; do
  api=$(gh api "repos/$repo/commits/$c/pulls" < /dev/null \
    --jq "[.[] | select(.merged_at != null and .merge_commit_sha == \"$c\") | .number] | map(tostring) | join(\" \")")
  subj=$(printf '%s\n' "$s" | sed -E -n \
    -e 's/^Merge pull request #([0-9]+) from .*$/\1/p' -e t \
    -e 's/^.*\(#([0-9]+)\)$/\1/p')
  if [ -z "$api" ]; then
    echo "$c $s" >> "$dir/direct.txt"
  else
    echo "$api" >> "$dir/prs.txt"
    [ "$api" = "$subj" ] || echo "$c 題 #${subj:-なし} / GitHub #$api：$s" >> "$dir/mismatch.txt"
  fi
done < "$dir/commits.txt"

while read -r n; do
  gh pr view "$n" --json closingIssuesReferences --jq '.closingIssuesReferences[].number' < /dev/null
done < "$dir/prs.txt" | awk '!seen[$0]++' > "$dir/issues.txt"
gh issue list --label "needs review" --state open --limit 200 --json number,title,url > "$dir/needs-review.json"
wc -l "$dir/commits.txt" "$dir/prs.txt" "$dir/direct.txt" "$dir/mismatch.txt" "$dir/issues.txt"
EOF
```

- `prs.txt`：`develop` に入った順の PR 番号。
- `direct.txt`：PR のないコミット（`develop` への直接のコミット）。履歴は書き換えないので、止まらずに「PR のないコミット」としてスライド、Release の本文、手順 8 の PR のコメント、手順 9 の知らせに載せる。作者はスライドと PR で知る。
- `mismatch.txt`：題の番号と GitHub の PR が食い違うコミット（題を書き換えたマージなど）。PR は GitHub の側を使い、食い違いをスライドと報告に載せる。
- 範囲の行数が `commits.txt` = `prs.txt` + `direct.txt` になっていることを確かめる。
- Issue は、閉じたかどうか（Issue の状態や `closedByPullRequestsReferences`）ではなく、PR の `closingIssuesReferences` から集める。Issue はあとから開き直されたり、手で閉じられたりするためで、PR が何を閉じると言ったかは PR の側にしか残らない。
- `gh` が通信の失敗で落ちたら、このブロックをやり直す。

## 3. 報告スライドの PDF を作る

`ebykn-report-slides` スキルを呼ぶ。渡すもの：識別子 `$id`、範囲（`$prev`・`$prev_sha` と `$sha`）、`prs.txt`、`issues.txt`、`direct.txt`、`mismatch.txt`、`needs-review.json`。PDF と、その元のファイルを `$dir` に受け取る。

PDF の作り方はそのスキルが受け持つ。スライドはリリースごとに 1 つ作り、前のものを上書きしない。PDF ができなければ、ここで止まって報告する（まだ何も押していない）。

範囲の PR を動かして実例を作る途中で問題が見つかったら、AGENTS.md の「見つけた問題」のとおり扱う。

- `ebykn-issue` スキルで Issue にし、本文に `report/$id` のスライドを作る途中で見つかったと書く。番号を `$dir/found.txt` に 1 行に 1 つ書く。手順 5 の Release の本文、手順 8 の PR のコメント、手順 9 の知らせに載せる。
- リリースは止めない。範囲は手順 1 で決めた `$sha` のまま進める。直す PR は `develop` に積み、次のリリースに入る。
- 見つかった問題のせいで PDF が作れないときだけ、ここで止まって報告する。

## 4. タグを付ける

`mode=resume-release` なら飛ばす。注釈付きのタグにし、メッセージに前回のコミットを 40 桁で残す（手順 1 の再開が読む）。

```bash
bash -euo pipefail <<'EOF'
. <scratchpad>/release-<id>/vars
printf '報告 %s\n\n前回：%s（%s）\n今回：%s\n' "$id" "$prev" "$prev_sha" "$sha" \
  | git tag -a "report/${id}" -F - "$sha"
git push origin "refs/tags/report/${id}" \
  || { git tag -d "report/${id}"; echo "止まる：タグを push できない"; exit 1; }
EOF
```

- push が失敗したら（認証や通信）、手元のタグを消して止まる。リモートには何も残らないので、手順 0〜3 で止まったときと同じに扱える。このブロックが権限の判定で拒否されたときは、タグも作られない。ほかの理由で手元にだけタグが残っても、次の手順 1 が消す。

前回のタグは、手順 1 のとおり `git tag --list 'report/*' --sort=-v:refname` の先頭（`2026-11-09.10` は `2026-11-09.9` より後に並ぶ）。言語のリリースのタグ（`v0.1.0` など）は `report/` の外なので混ざらない。

ここから手順 6 までで止まったら、手順 1 からやり直す。手順 1 が、Release のないタグを `mode=resume-release` として見つける。

## 5. Release を作る

タグ `report/$id` の prerelease として、PDF とその元のファイルを添付する。元のファイルが複数なら、1 つの `tar.gz` にまとめて添付する。

```bash
bash -euo pipefail <<'EOF'
. <scratchpad>/release-<id>/vars
{
  echo "範囲：\`$prev\`（\`$(git rev-parse --short=12 "$prev_sha")\`）..\`$(git rev-parse --short=12 "$sha")\`"
  echo
  echo "## PR"
  echo
  while read -r n; do gh pr view "$n" --json number,title --jq '"- #\(.number) \(.title)"' < /dev/null; done < "$dir/prs.txt"
  echo
  echo "## 閉じた Issue"
  echo
  while read -r n; do gh issue view "$n" --json number,title --jq '"- #\(.number) \(.title)"' < /dev/null; done < "$dir/issues.txt"
  if [ -s "$dir/direct.txt" ]; then
    echo
    echo "## PR のないコミット"
    echo
    sed 's/^\([0-9a-f]\{12\}\)[0-9a-f]* /- `\1` /' "$dir/direct.txt"
  fi
  if [ -s "$dir/found.txt" ]; then
    echo
    echo "## 報告スライドの作成で見つかった Issue"
    echo
    while read -r n; do gh issue view "$n" --json number,title --jq '"- #\(.number) \(.title)"' < /dev/null; done < "$dir/found.txt"
  fi
} > "$dir/release-notes.md"
cat "$dir/release-notes.md"
EOF
```

```bash
bash -euo pipefail <<'EOF'
. <scratchpad>/release-<id>/vars
gh release create "report/${id}" --verify-tag --prerelease \
  --title "報告 ${id}" --notes-file "$dir/release-notes.md" \
  "<PDF>#報告スライド（PDF）" "<元のファイル>#報告スライドの元のファイル"
gh release view "report/${id}" --json url --jq .url
EOF
```

`--verify-tag` で、手順 4 で押したタグがなければ失敗させる（`gh` に別のタグを作らせない）。

## 6. release を早送りする

Release ができてから、`release` を今回のコミットまで進める。

```bash
bash -euo pipefail <<'EOF'
. <scratchpad>/release-<id>/vars
gh release view "report/${id}" > /dev/null || { echo "止まる：report/${id} の Release がない"; exit 1; }
if git ls-remote --exit-code origin refs/heads/release > /dev/null; then
  git fetch origin release
  git merge-base --is-ancestor origin/release "$sha" || { echo "止まる：release が ${sha} の祖先ではない"; exit 1; }
fi
git push origin "${sha}:refs/heads/release"
EOF
```

- `release` がなければ（初回）、`ls-remote` が 2 で終わり、`push` が `release` を作る。
- `release` が今回のコミットの祖先でなければ、誰かが `release` に直接書いた。push せず止まって報告する。
- `--force` は付けない。早送りでない push はサーバーが拒否する。

## 7. release のワークスペースを進める

作者が `release` ブランチで動作を確かめるための orca のワークスペースを、今回のコミットまで進める。初めてリリースを切るときに作り、2 回目からは早送りする。ワークスペースの名前は `release`、表示名は `release（作者の確認用）` にする。

この手順のコマンドは、まだ実際には動かしていない。`release` ブランチがまだないため。初回のリリースで動かしたら、下の「確かめたこと・確かめていないこと」と食い違いがないかを見て、違っていたらこの手順を直す PR を出す。

orca の CLI は、失敗すると終了コード 1 で終わり、エラーの JSON を標準出力に出す。`set -e` の下でそのまま `> ファイル` にすると、理由を出さずに止まるので、どの呼び出しも `if ! "$orca" … > ファイル; then … cat ファイル; exit 1; fi` で包み、そのあとで `.ok` を確かめる。

まず、ワークスペースがあるかを調べる。

```bash
bash -euo pipefail <<'EOF'
. <scratchpad>/release-<id>/vars
orca=${ORCA_CLI_COMMAND:-orca}
display='release（作者の確認用）'
git fetch origin release
[ "$(git rev-parse origin/release)" = "$sha" ] || { echo "止まる：origin/release が ${sha} ではない"; exit 1; }
repo=$(dirname "$(git rev-parse --path-format=absolute --git-common-dir)")   # 元のリポジトリ

# orca のワークスペースのうち、release（か release-<番号>）を取り出しているもの・表示名が release で始まるもの・
# パスが /release（か /release-<番号>）で終わるもの。作る途中で止まって残った、番号付きのワークスペースも拾う
if ! "$orca" worktree list --repo "path:$repo" --limit 10000 --json > "$dir/worktrees.json"; then
  echo "止まる：orca worktree list が失敗した"; cat "$dir/worktrees.json"; exit 1
fi
[ "$(jq -r '.ok' "$dir/worktrees.json")" = true ] || { echo "止まる：orca worktree list が失敗した"; cat "$dir/worktrees.json"; exit 1; }
[ "$(jq -r '.result.truncated' "$dir/worktrees.json")" = false ] || { echo "止まる：orca worktree list の一覧が切れている"; exit 1; }
# 一覧に入っていないホストがあると、空の答えは「ワークスペースがない」ことを示さない
# hostScope がない（古い orca）ときも、調べた範囲が分からないので止まる
[ "$(jq '(.result | has("hostScope")) and (.result.hostScope.omittedHostIds | type == "array" and length == 0)' "$dir/worktrees.json")" = true ] \
  || { echo "止まる：orca worktree list が調べていないホストがある"; jq -c '.result.hostScope' "$dir/worktrees.json"; exit 1; }
jq -c --arg d "$display" '[.result.worktrees[]
  | select((.branch // "" | test("^refs/heads/release(-[0-9]+)?$"))
      or (.displayName // "" | startswith("release"))
      or (.path | test("/release(-[0-9]+)?$")))
  | {path, branch, displayName}]' "$dir/worktrees.json" > "$dir/release-ws.json"
cat "$dir/release-ws.json"

# git の側で release を取り出しているワークツリー（orca の外のものも含む）
git worktree list --porcelain \
  | awk '/^worktree /{p=substr($0,10)} $0=="branch refs/heads/release"{print p}' > "$dir/release-git.txt"
cat "$dir/release-git.txt"
EOF
```

| `release-ws.json` | `release-git.txt` | すること |
| --- | --- | --- |
| 空（`[]`） | 空 | 初回（または作者がワークスペースを消した）。下の「作る」へ |
| 1 件で、`branch` が `refs/heads/release` | その 1 件のパスだけ | 2 回目以降。下の「早送りする」へ |
| 1 件で、`branch` が `refs/heads/release` でない | — | 止まる。作者がワークスペースで別のブランチに切り替えたか、前の回の「作る」が番号付きのワークスペース（パス `…/release-<番号>`、ブランチ `release-<番号>`）を残したのかもしれない。触らずに、パスとブランチを報告する |
| 2 件以上 | — | 止まる。どれを使うかを決めずに報告する |
| 空 | 空でない | 止まる。orca の外で `release` を取り出しているワークツリーがある。触らずに報告する |

### 作る（初回）

```bash
bash -euo pipefail <<'EOF'
. <scratchpad>/release-<id>/vars
orca=${ORCA_CLI_COMMAND:-orca}
display='release（作者の確認用）'
repo=$(dirname "$(git rev-parse --path-format=absolute --git-common-dir)")
[ "$(jq 'length' "$dir/release-ws.json")" = 0 ] && [ ! -s "$dir/release-git.txt" ] \
  || { echo "止まる：release のワークスペースか、release を取り出したワークツリーがある"; exit 1; }

# ローカルの release が残っているとき（作者がワークスペースだけを消したときなど）は、origin/release まで早送りしておく。
# 取り出されていないことは release-git.txt が空であることで確かめてある。orca は、同じコミットにあって取り出されていないローカルの release を使い回す
if git show-ref --verify --quiet refs/heads/release; then
  git merge-base --is-ancestor release origin/release \
    || { echo "止まる：ローカルの release に origin/release にないコミットがある"; git log --oneline origin/release..release; exit 1; }
  git branch -f release origin/release
fi

if ! "$orca" worktree create --repo "path:$repo" --name release \
    --base-branch origin/release --branch release \
    --no-parent --setup skip \
    --comment "作者の確認用。リリースのたびに origin/release まで早送りする" \
    --json > "$dir/release-create.json"; then
  echo "止まる：orca worktree create が失敗した"; cat "$dir/release-create.json"; exit 1
fi
[ "$(jq -r '.ok' "$dir/release-create.json")" = true ] || { echo "止まる：orca worktree create が失敗した"; cat "$dir/release-create.json"; exit 1; }
ws=$(jq -r '.result.worktree.path' "$dir/release-create.json")
[ -d "$ws" ] || { echo "止まる：作ったワークスペースのパスが読めない：$ws"; exit 1; }
printf 'ws=%s\nws_display=%s\n' "$ws" "$display" > "$dir/release-ws.txt"
# ここから下で止まったら、作ったワークスペース（$ws）が残る。報告にパスを書く
trap 'echo "作ったワークスペースが残っている：$ws"' EXIT

[ "$(git -C "$ws" symbolic-ref --short HEAD)" = release ] \
  || { echo "止まる：$ws が release ではなく $(git -C "$ws" symbolic-ref --short HEAD) を取り出した"; exit 1; }
[ "$(git -C "$ws" rev-parse --abbrev-ref 'release@{upstream}')" = origin/release ] \
  || { echo "止まる：$ws の release の上流が origin/release ではない"; exit 1; }
[ "$(git -C "$ws" rev-parse HEAD)" = "$sha" ] || { echo "止まる：$ws の HEAD が ${sha} ではない"; exit 1; }

if ! "$orca" worktree set --worktree "path:$ws" --display-name "$display" --json > "$dir/release-set.json"; then
  echo "止まる：orca worktree set が失敗した"; cat "$dir/release-set.json"; exit 1
fi
[ "$(jq -r '.ok' "$dir/release-set.json")" = true ] || { echo "止まる：orca worktree set が失敗した"; cat "$dir/release-set.json"; exit 1; }
trap - EXIT
echo 'ws_state=作った' >> "$dir/release-ws.txt"
cat "$dir/release-ws.txt"
EOF
```

- `--branch release` で、orca の付ける接頭辞（`MatchaChoco010/`）なしの `release` というブランチにする。`--base-branch origin/release` で、その出発点を `origin/release` にする。orca は、ローカルの `release` がなければ `origin/release` の位置に作って上流を設定し、同じコミットにあって取り出されていないローカルの `release` があれば、それを使い回す。そのため、残っているローカルの `release` は先に早送りしておく。
- orca が名前に番号を付けるのは、残っているディレクトリや、大文字小文字だけが違う名前とぶつかったときだけ。そのときは `release` 以外のブランチを取り出すので、止まって報告する。
- `--no-parent`：コーディネーターのワークスペースの子にしない。作者が使う、独立したワークスペースにする。
- `--setup skip`：リポジトリの setup のフックを走らせない。
- `--agent` を付けない。orca は最初のターミナル（シェル）だけを開く。作者はそこで試す。
- 表示名は `worktree create` では付けられないので、`worktree set --display-name` で付ける。
- ワークスペースを作ったあとで止まったら（`作ったワークスペースが残っている` が出たら）、そのパスを手順 8 の PR のコメントと手順 9 の知らせに書く。コーディネーターは、作者が次に話しかけたときに片付けてよいかを聞く。作者の了承を得てから、コーディネーターが `orca worktree rm --worktree "path:<パス>" --delete-branch` で消してもよい。片付けるまでは、やり直しても上の表の「止まる」に当たり、もう 1 つ作ることはない。
- ローカルの `release` に `origin/release` にないコミットがあって止まったら、そのコミットを手順 8 の PR のコメントと手順 9 の知らせに書く。コーディネーターは、作者が次に話しかけたときに、そのコミットが要るかを聞く。要らなければ、作者の了承を得てから `git branch -D release` で消し、この手順をやり直す。

### 早送りする（2 回目以降）

```bash
bash -euo pipefail <<'EOF'
. <scratchpad>/release-<id>/vars
orca=${ORCA_CLI_COMMAND:-orca}
display='release（作者の確認用）'
[ "$(jq 'length' "$dir/release-ws.json")" = 1 ] || { echo "止まる：release のワークスペースが 1 つではない"; exit 1; }
[ "$(jq -r '.[0].branch' "$dir/release-ws.json")" = refs/heads/release ] || { echo "止まる：release のワークスペースが release を取り出していない"; exit 1; }
ws=$(jq -r '.[0].path' "$dir/release-ws.json")
printf 'ws=%s\nws_display=%s\n' "$ws" "$display" > "$dir/release-ws.txt"

# 表示名が違えば付け直す（前の回の「作る」が表示名を付ける前に止まった場合など）。ファイルには触らない
if [ "$(jq -r '.[0].displayName' "$dir/release-ws.json")" != "$display" ]; then
  if ! "$orca" worktree set --worktree "path:$ws" --display-name "$display" --json > "$dir/release-set.json"; then
    echo "止まる：orca worktree set が失敗した"; cat "$dir/release-set.json"; exit 1
  fi
  [ "$(jq -r '.ok' "$dir/release-set.json")" = true ] || { echo "止まる：orca worktree set が失敗した"; cat "$dir/release-set.json"; exit 1; }
fi

[ "$(git -C "$ws" symbolic-ref --short HEAD)" = release ] || { echo "止まる：$ws が release を取り出していない"; exit 1; }
if [ -n "$(git -C "$ws" status --porcelain --untracked-files=all)" ]; then
  echo "止まる：$ws の作業ツリーに変更がある"
  git -C "$ws" status --short --untracked-files=all
  exit 1
fi
git -C "$ws" merge-base --is-ancestor HEAD origin/release \
  || { echo "止まる：$ws の release に origin/release にないコミットがある"; git -C "$ws" log --oneline origin/release..HEAD; exit 1; }

git -C "$ws" merge --ff-only --no-overwrite-ignore origin/release
[ "$(git -C "$ws" rev-parse HEAD)" = "$sha" ] || { echo "止まる：$ws の HEAD が ${sha} ではない"; exit 1; }
echo 'ws_state=早送りした' >> "$dir/release-ws.txt"
cat "$dir/release-ws.txt"
EOF
```

- 作業ツリーに変更（追跡していないファイルを含む）があれば、捨てずに止まる。作者が試しに書いたものかもしれないため。`git stash`・`git checkout -- .`・`git clean` はしない。
- `--no-overwrite-ignore`：無視されたファイル（`playground/` の下など）は `git status --porcelain` に出ないが、`git merge` は既定で、それを上書きする。新しいコミットが同じパスのファイルを追跡していたら、作者が書き換えたファイルを黙って上書きしないよう、git に止めさせる。止まったら、そのパスを手順 8 の PR のコメントと手順 9 の知らせに書く。片付けは作者に任せ、コーディネーターが作者の次に話しかけたときに伝える。
- 作者がワークスペースでコミットしていたら（`release` に `origin/release` にないコミットがある）、同じく止まる。
- ここで止まっても、タグ・Release・`release` の早送りは済んでいる。ワークスペースは進めずに手順 8 に進み、手順 8 の PR のコメントと手順 9 の知らせに、止まった理由と `status` か `log` の出力を書く。作者が片づけたら、この手順だけをやり直す（何度やり直してもよい）。

### 例を playground に写す

「作る」か「早送りする」が済んだら、作者がすぐ試せるように、ワークスペースで `cargo xtask playground` を実行する。`ebykn` をビルドし、`examples/` を `playground/` に写す（使い方は `examples/README.md`）。

```bash
bash -euo pipefail <<'EOF'
. <scratchpad>/release-<id>/vars
ws=$(sed -n 's/^ws=//p' "$dir/release-ws.txt")
[ -d "$ws" ] || { echo "止まる：release-ws.txt にワークスペースのパスがない"; exit 1; }
[ "$(git -C "$ws" rev-parse HEAD)" = "$sha" ] || { echo "止まる：$ws の HEAD が ${sha} ではない"; exit 1; }
if [ ! -f "$ws/examples/README.md" ]; then
  echo 'playground=なし' >> "$dir/release-ws.txt"; exit 0   # この版の develop にまだ examples/ がない
fi
(cd "$ws" && cargo xtask playground) 2>&1 | tee "$dir/playground.txt"
echo "playground=$ws/playground" >> "$dir/release-ws.txt"
EOF
```

- `playground/` は `.gitignore` に入っているので、作者が書き換えても、次の回の「早送りする」の「作業ツリーに変更がある」には当たらない。`playground/` の下のファイルは追跡しない（`check-examples` が、追跡されたファイルがあれば失敗にする）ので、早送りが作者のファイルを上書きすることもない。万一追跡されていても、「早送りする」の `--no-overwrite-ignore` で git が止まる。
- すでにあるファイルは上書きしない。作者が書き換えたものは残り、新しく増えた例だけが足される。中身が `examples/` と違って残したファイルは、出力の「上書きせずに残した」に並ぶ。手順 8 の PR のコメントと手順 9 の知らせに書く。
- ここで失敗しても（`ebykn` のビルドの失敗など）、タグ・Release・`release` の早送りは済んでいる。手順 8 に進み、手順 8 の PR のコメントと手順 9 の知らせに `playground.txt` の終わりを書く。

### 確かめたこと・確かめていないこと

`release` ブランチがまだないので、この手順は通しては動かしていない。

- 手元で動かして確かめた：`worktree list --repo "path:<元のリポジトリ>" --json` が `.ok`、`.result.truncated`（切れていなければ `false`）、`.result.hostScope.omittedHostIds`（すべてのホストを調べたら空）を返すこと。orca が失敗すると終了コード 1 で終わること。`git worktree list --porcelain` を読む `awk`。
- orca 本体のソースを読んで確かめた（PR #36 のレビュー）：`--branch release --base-branch origin/release` で、ローカルの `release` が番号なしで `origin/release` の位置に作られること。上流を orca が設定すること。同じコミットにあって取り出されていないローカルの `release` は使い回すこと。番号が付くのは、残っているディレクトリか、大文字小文字だけが違う名前とぶつかったときだけであること。エラーの JSON を標準出力に出すこと。
- まだ確かめていない：`worktree create --json` の結果で、パスが `.result.worktree.path` にあること（`worktree show --json` と `worktree current --json` の形から推した）。

## 8. release → main の PR

開いたリリースの PR と、確認待ちのリリースを調べる（前回の PR がマージコミットで入ったかは、手順 1 で確かめてある）。

```bash
bash -euo pipefail <<'EOF'
. <scratchpad>/release-<id>/vars
git fetch origin --prune
gh pr list --base main --head release --state open --json number,url
git tag --list 'report/*' --merged "$sha" --no-merged origin/main --sort=v:refname   # 確認待ちのリリース
EOF
```

- 最後のコマンドの一覧が、本文の「確認待ちのリリース」に載せるものの全部。手順 6 のとおり `release` は Release のあるタグにしか進まないので、どれにも Release がある。

本文とコメントを書き、`yomiyasu:yomiyasu` スキルの `--doc-type pr` で推敲する（lint → 書き直し → `run --final <ファイル> --doc-type pr` → `verify`）。verify を通した最終稿を変えずに使う。

本文（PR がなければ新しく書き、あれば今の本文 `gh pr view <PR 番号> --json body --jq .body` を元に直す）：

```markdown
`release` を `main` に入れる PR です。作者が報告スライドで確かめてから、マージコミットでマージします。squash や rebase はしません。

## 確認待ちのリリース

- [report/<id>](<Release の URL>)：<1 文の要約>（PR <件数> 件）

## マージのしかた

この PR の先頭は、いちばん新しい報告済みのタグ `report/<id>`（`<sha 40 桁>`）です。報告していない範囲を入れないよう、先頭を指定してマージします。

    gh pr merge <PR 番号> --merge --match-head-commit <sha 40 桁>

GitHub の画面でマージするときは、PR の先頭のコミットがこの sha であることを確かめます。

## 判断待ちの Issue

- #<番号> <題>
```

- 「確認待ちのリリース」は、上の `git tag` の一覧と同じにする。前からある行はそのまま残し、今回の行を足す。一覧にないタグの行（`main` に入ったもの）は消す。作者は確認待ちのリリースを複数まとめてマージするので、一覧はいつも最新にする。
- 「マージのしかた」の識別子と sha は、今回の `$id` と `$sha` に書き換える。
- 「判断待ちの Issue」は `needs-review.json` で書き直す。
- 本文とコメントに `Closes #N` を書かない。`main` は既定のブランチではないので効かず、Issue は `develop` へのマージで閉じている。閉じた Issue は「閉じた Issue：#N」のように、閉じる動詞を付けずに書く。

コメント（今回のリリースの分）：

```markdown
## report/<id>

- Release：<Release の URL>（報告スライドの PDF）
- 範囲：`<前回のタグ>`（`<前回のコミット 12 桁>`）..`<コミット 12 桁>`
- PR：#<番号>, …（<件数> 件）
- 閉じた Issue：#<番号>, …
- PR のないコミット：`<コミット 12 桁>` <題>, …（なければこの行を消す）
- 報告スライドの作成で見つかった Issue：#<番号>, …（found.txt。なければこの行を消す）
- release のワークスペース：<作った / 早送りした / 進めていない：理由と status か log の出力>（release-ws.txt）
- 試せる例：`<ワークスペースのパス>/playground/`。上書きせずに残したファイル：<一覧>（playground.txt。手順 7 で失敗したら、その終わり。`playground=なし` ならこの行を消す）
- 作れなかった Issue：`<$dir>/pending-issues/` の <件数> 件（`gh issue create` が拒否された。なければこの行を消す）
- 知らせ：送れない（<手順 0 で Run を確かめられなかった理由>。確かめられたらこの行を消す）

<何が入ったかを 2〜4 文で>
```

PR がないとき：

```bash
bash -euo pipefail <<'EOF'
gh pr create --base main --head release --title "確認待ちのリリースを main に入れる" --body-file <本文の最終稿>
EOF
```

PR があるとき：

```bash
bash -euo pipefail <<'EOF'
gh pr edit <PR 番号> --body-file <本文の最終稿>
EOF
```

どちらでも：

```bash
bash -euo pipefail <<'EOF'
gh pr comment <PR 番号> --body-file <コメントの最終稿>
gh pr checks <PR 番号> --watch
EOF
```

`release` を進めると、この PR の CI が走る。`ci-required` が緑になるのを見届けてから手順 9 に進む。

本文とコメントは作者があとで読むためのもの。作者に読むよう促す言葉（「ご確認ください」など）は書かない。

## 9. コーディネーターに知らせる

作者には報告しない。終わったときも、途中で止まったときも（手順 1 で `develop` が進んでいなかったときを含む）、手順 0 で確かめた Run に結果を送る。コーディネーターは、見つかった Issue を直す作業を割り振り、このワークツリーを片付ける。

知らせの中身を `<scratchpad>/notice.txt` に書く。終わったとき：

```text
結果：リリース report/<id> を切りました。
- 範囲：<前回のタグ>（<前回のコミット>）..<コミット>（PR <件数> 件、閉じた Issue <件数> 件）
- Release（報告スライド）：<Release の URL>
- リリースの PR：<PR の URL>（確認待ち：report/<id>, …）
- 判断待ちの Issue：#<番号> <題>, …（なければ「なし」）
- PR のないコミット・題と PR の食い違い：<direct.txt と mismatch.txt の中身>（なければ「なし」）
- スライドを作る途中で見つかった Issue：#<番号> <題>, …（found.txt。なければ「なし」。直す作業の割り振りを頼む）
- 作れなかった Issue：<$dir>/pending-issues/ の <件数> 件（なければ「なし」。作って割り振るのを頼む）
- release のワークスペース：<表示名>（<パス>）。<作った / 早送りした / 進めていない：理由>
- 試せる例：<パス>/playground/（`export PATH="<パス>/target/debug:$PATH"` のあと、各例の README のコマンドを打つ）。上書きせずに残したファイル：<一覧>（なければ「なし」）
- 作者に伝えること：<作者の操作が要るもの。なければ「なし」>
- 作業用のフォルダ：<$dir>
- このワークツリー：<パス>（片付けを頼む）
```

止まったとき：

```text
結果：リリースが手順 <番号> で止まりました。
- 理由：<「止まる：…」の行か、権限の判定で拒否された・確認を求められたコマンド。develop が進んでいないときは「前回のリリースから develop が進んでいない」>
- 識別子と範囲：<vars の中身。手順 1 より前で止まったら「なし」>
- 押したもの：<タグ・Release・release の早送り・PR の作成と更新・PR のコメントのうち済んだもの。なければ「なし」>
- 作業用のフォルダ：<$dir。手順 1 より前で止まったら「なし」>
- PDF：<PDF のパス。まだなければ「なし」>
- 作れなかった Issue：<$dir>/pending-issues/ の <件数> 件（なければ「なし」）
- 作者に伝えること：<作者の操作が要るもの。なければ「なし」>
- 続けるには：<「止まった回を続ける」の表のどの行か。develop が進んでいないときは「なし」>
- このワークツリー：<パス>（片付けを頼む）
```

```bash
bash -euo pipefail <<'EOF'
orca=${ORCA_CLI_COMMAND:-orca}
echo "このワークツリー：$(git rev-parse --show-toplevel)"
run=$(cat <scratchpad>/coordinator-run)
[ -n "$run" ] || { echo "送らない：手順 0 で Run を確かめられなかった。知らせの中身："; cat <scratchpad>/notice.txt; exit 1; }
"$orca" orchestration send --to "run:${run}" \
  --subject "<件名：例「リリース report/<id> を切った」「リリース：develop が進んでいない」「リリースが手順 <番号> で止まった」>" \
  --body "$(cat <scratchpad>/notice.txt)"
EOF
```

- 送るのは 1 回だけ。送ったら終わる。このワークツリーと `$dir` は消さない（コーディネーターが続きに使い、片付ける）。
- 送れなかったら（Run を確かめられなかった、送信が失敗した）、`notice.txt` の中身をこのターミナルに出して終わる。PR のコメントまで進んだ回は、その「知らせ：送れない」の行が目印になる。コーディネーターは「見回り」で見つける。
- コーディネーターが手で切ったときは送らない。同じ中身を手元で確かめ、見つかった Issue を割り振る。

## コーディネーターが知らせを受けたら

- 終わったとき：`found.txt` の Issue と `pending-issues/` の Issue を、AGENTS.md の着手順で割り振る（`pending-issues/` のものは、先に `ebykn-issue` で作る）。作者には報告しない（催促しない）。ただし、そのうち `needs review` の Issue は、AGENTS.md の「人間の判断が要るもの」のとおり、作者が次にコーディネーターに話しかけたときに番号を伝える（催促はしない）。
- 止まったとき：理由に応じて直し、次の「止まった回を続ける」の表に従って続ける。CI が赤ければ、直す PR を `develop` に積み、緑になったら手で切るか、次の朝に任せる。
- 「作者に伝えること」は、作者が次に話しかけたときに伝える。催促しない。
- どちらでも、続きが済んだら、知らせにあるワークツリーを片付ける。リリースのワークツリーはどこにも push しないブランチなので、ブランチごと消す。`$dir` はオートメーションのエージェントの scratchpad にあり、ワークツリーとは別なので残る。

```bash
bash -euo pipefail <<'EOF'
orca=${ORCA_CLI_COMMAND:-orca}
"$orca" worktree rm --worktree "path:<知らせにあるワークツリーのパス>" --delete-branch --json
EOF
```

## 止まった回を続ける

コーディネーターが、自分のワークスペースで行う。`$dir` は同じマシンのパスなので、知らせにあるパスをそのまま読める。

| 止まった点 | リモートの状態 | 続け方 |
| --- | --- | --- |
| 手順 0〜4 の push の前 | 何も押していない（手元にだけあるタグは、次の手順 1 が消す） | 次の朝に任せるか、`orca automations run <id>` で走らせる。手で切ってもよい（手順 1 から） |
| 手順 4 の push のあと〜6 | タグか Release は押したが、`release` がタグまで進んでいない | 手で手順 1 を走らせると、`resume-release` か `resume-ff` で同じ識別子を再開する。そのとき、手順 1 のブロックの `dir=` を知らせの `$dir` にし、PDF と手順 2 の出力を使い回す。次の回に任せてもよい（次の回の手順 1 も同じように再開するが、`$dir` は新しくなるので、手順 2・3 を作り直す） |
| 手順 7〜8 | `release` はタグまで進んだ | 手順 1 は新しいリリースとみなすので、走らせない。知らせの `$dir/vars` を読む。読めなければ、知らせの「識別子と範囲」から `vars` を作り直す（下のブロック）。手順 2 の出力が `$dir` になければ手順 2 のブロックを走らせ、止まった手順（7 か 8）から 9 の前まで行う |
| 手順 9（知らせが届かない） | すべて済んだ | 「見回り」で見つける。結果は Release と PR のコメントにある |

`vars` を作り直す（手順 7〜8 で止まり、`$dir/vars` が読めないとき）：

```bash
bash -euo pipefail <<'EOF'
id=<識別子>
dir=<scratchpad>/release-$id
sha=$(git rev-parse "report/${id}^{commit}")
msg=$(git tag --list --format='%(contents)' "report/${id}")
prev=$(printf '%s\n' "$msg" | sed -n 's/^前回：\(.*\)（[0-9a-f]\{40\}）$/\1/p')
prev_sha=$(printf '%s\n' "$msg" | sed -n 's/^前回：.*（\([0-9a-f]\{40\}\)）$/\1/p')
[ -n "$prev_sha" ] || { echo "止まる：report/${id} のメッセージに前回のコミットがない"; exit 1; }
mkdir -p "$dir"
printf 'mode=%s\nid=%s\nsha=%s\nprev=%s\nprev_sha=%s\ndir=%s\n' \
  continue "$id" "$sha" "$prev" "$prev_sha" "$dir" > "$dir/vars"
cat "$dir/vars"
EOF
```

## 見回り

知らせが届かない回は、ワークツリーが残り、止まった点も分からない。知らせが届かないのは、手順 0 で Run を確かめられなかった、権限の確認で止まった、Orca が終了した、マシンがスリープした、エージェントの文脈が尽きた、などのとき。Orca は new-per-run のワークツリーを消さない。

コーディネーターは、毎朝の知らせが届かなかった日、手でリリースを切る前、作者が次にコーディネーターに話しかけたときに見回る。待っているコーディネーターは、届かなかった知らせには気づけないため。

```bash
bash -euo pipefail <<'EOF'
orca=${ORCA_CLI_COMMAND:-orca}
"$orca" automations list --json | jq -c '.. | objects | select(.name? == "ebykn-daily-release")'
"$orca" automations runs --id <オートメーションの id> --limit 20 --json
repo=$(dirname "$(git rev-parse --path-format=absolute --git-common-dir)")
"$orca" worktree list --repo "path:$repo" --limit 10000 --json \
  | jq -r '.result.worktrees[] | "\(.id)\t\(.path)\t\(.branch // "")\t\(.displayName // "")"'
EOF
```

- `runs` の各回について、状態と、その回のワークスペース（`workspaceId`）を読む。`worktree list` の 1 列目の `id` と突き合わせる。
  - 終わっていない状態：`pending`、`dispatching`、`dispatched`。この回には触らない。
  - 終わった状態：`completed`、`dispatch_failed`、`skipped_*`（`skipped_precheck` など）。
  - `dispatch_failed` の回でも、エージェントがまだ生きていることがある。手で続ける前に、そのエージェントを止めるか、ワークツリーを消してから続ける。
- 終わった回のワークスペースが `worktree list` に残っていて、その回の知らせを受け取っていなければ、「知らせのなかった回」とする。
  - リモートの状態（`report/*` のタグ、Release、`release` の位置、リリースの PR のコメント）で止まった点を決め、「止まった回を続ける」の表に従って続ける。
  - リリースの PR まで進んだ回なら、PR に「毎朝の回 <日時> は知らせを送らずに終わった（<分かった理由>）」とコメントして目印にする。PR まで進んでいなければ、作者が次に話しかけたときに伝える。催促はしない。
- 続きが済んだら、知らせのなかった回のワークスペースも `orca worktree rm --worktree "path:<パス>" --delete-branch --json` で消す。
- `runs` と `automations list` の JSON の形は、最初の試し走りで確かめる（下の「作ったあとに確かめる」）。形が上の読み方と違えば、この節を直す PR を出す。

## 作者が問題を見つけたとき

AGENTS.md の「見つけた問題」のとおり扱う。

1. 問題を Issue にする（`ebykn-issue` スキル）。どのリリースで見つかったかを本文に書く。
2. 直す PR を `develop` に積む。ワーカーに作らせ、AGENTS.md の「マージ」のとおりマージする。過去の履歴は直さない。
3. 直したものは次の朝のリリースに入る。急ぐときは、コーディネーターが手でリリースを切る（この手順の 1 から）。同じ日なら `N` が 1 つ増える。スライドは前回のタグからの差分になる。

## 作者が main にマージしたあと

- リリースの PR は閉じる。次のリリースの手順 8 では開いた PR がないので、新しく作る。
- 新しい PR の「確認待ちのリリース」には、`main` に入っていないタグだけが載る。
- 次のリリースの手順 1 で、前回の PR がマージコミットで入ったことを確かめる。

## 毎朝のリリースのオートメーション

オートメーションは、リポジトリの外にある Orca の設定。コーディネーターが、コーディネーターのワークスペースで作る。

作るのは、手順 3 が呼ぶ `ebykn-report-slides` スキルが `origin/develop` に入ってからにする。オートメーションのワークツリーは `origin/develop` から作るので、スキルがまだなければ、毎朝の回が手順 3 で止まる。

| 設定 | 値 | 理由 |
| --- | --- | --- |
| 時刻 | 毎日 6:00（`Asia/Tokyo`） | 作者の指示。Orca は起動しているときだけ走らせるので、Orca が起動していない日は切らない |
| 逃した回の猶予 | 15 分 | 6 時を少し過ぎて目覚めたときだけ走らせ、何時間もあとに取り戻して切らない（既定は 720 分） |
| エージェント | `claude` | このリポジトリのスキル（`.claude/skills/`）と yomiyasu を使う |
| ワークツリー | 走るたびに新しく作る（`new-per-run`）。元は `origin/develop` | 前の回のワークツリーの状態を引き継がない。使い終わったものはコーディネーターが片付ける |
| setup | `skip` | リポジトリの setup のフックは空。走らせないと明示する |
| precheck | 付けない | `develop` が進んだかは手順 1 が判断する。判断する場所を 1 つにする |
| プロンプトの Run の id | コーディネーターの Run（`orca orchestration run-current --json`） | `run-list` はすべてのリポジトリの Run の一覧で、Run は消せない。一覧から推すと、Run が 1 つ増えただけで宛先を決められなくなる |

### 無人の実行の権限

無人で最後まで進むには、取り消せない手順（手順 4〜8 のタグと `release` の push、`gh release create`、`gh pr create`・`gh pr edit`・`gh pr comment`）を、確認なしで行える権限が要る。

- 今の Orca の全体の設定（`agentPermissionDefault: "auto"`）では、オートメーションは Claude を `--permission-mode auto` で起動する。権限の判定でこのリポジトリへの push などが信頼されていなければ、拒否されるか確認待ちで止まる。確認待ちで止まると、知らせも届かない。
- 権限をどうするかは作者が決める。作者が選ぶ方法は 2 つ：Orca の UI でこのオートメーションの権限のモードを設定する（CLI では設定できない）、またはプロジェクトの設定で、この手順のコマンドを許可する。エージェントとコーディネーターは、権限の設定のファイルを足したり変えたりしない。
- 作者が決めるまでは、冒頭のとおり、拒否されたらそこで止まって知らせ、コーディネーターが「止まった回を続ける」で残りを行う。リリースは止まらない。
- マシンがスリープすると、スライドを作る長い回が途中で切れうる。Orca の `keepComputerAwakeWhileAgentsRun` は今は `false`。切れた回は知らせが届かないので「見回り」で見つける。この設定を変えるかも作者が決める。

### 作る

まず、同じオートメーション（名前 `ebykn-daily-release`）がないかを確かめる。あれば作らず、下の「コーディネーターが替わったら」の `edit` で直す。

```bash
bash -euo pipefail <<'EOF'
orca=${ORCA_CLI_COMMAND:-orca}
"$orca" automations list
EOF
```

プロンプトを書き出す。全文は、このブロックの `PROMPT` のあいだ。`@RUN_ID@` を、コーディネーターの Run の id に置き換える。

```bash
bash -euo pipefail <<'EOF'
orca=${ORCA_CLI_COMMAND:-orca}
if ! cur=$("$orca" orchestration run-current --json); then
  echo "止まる：orca orchestration run-current が失敗した"; echo "$cur"; exit 1
fi
run=$(printf '%s\n' "$cur" | jq -r '.result.runId // empty')
[ -n "$run" ] || { echo "止まる：このワークスペースに Run が結び付いていない"; exit 1; }
"$orca" orchestration run-show --id "$run" --json --full | jq -e '.result.run.legacy == 0' > /dev/null \
  || { echo "止まる：$run を確かめられない（legacy か、見つからない）"; exit 1; }
prompt=$(cat <<'PROMPT'
ebykn-lang-proto の毎朝のリリースです。このワークツリーは origin/develop から作られています。
コーディネーターの Run の id：@RUN_ID@

1. AGENTS.md を読み、ebykn-release スキルを手順 0 から最後まで実行してください。手順 0 の Run の id には、上の id を使ってください。
2. develop が前回のリリースから進んでいなければ、手順 1 で止まります。それも結果として手順 9 で知らせてください。
3. 終わったら、途中で止まったときも、スキルの手順 9 のとおり、コーディネーターの Run に結果を送ってください。手順 0 で Run を確かめられなくても、リリースは止めずに進め、スキルのとおり PR のコメントに書いてください。
4. どの手順でも、コマンドが権限の判定で拒否されたり確認を求められたりしたら、回り込まずにそこで止まり、手順 9 で、止まった手順・$dir・PDF のパスを知らせてください。タグや release の push、gh release create、gh pr create・edit・comment だけでなく、手順 0〜3 のコマンドも同じです。例外は gh issue create で、拒否されたらスキルのとおり本文を $dir/pending-issues/ に残して進めてください。
5. 作者（ユーザー）に、レビューや確認を催促しないでください。結果は Release、リリースの PR の本文とコメント、コーディネーターへの知らせにだけ残します。
6. develop・release・main に直接コミットしないでください。このワークツリーと $dir は消さずに残してください。コーディネーターが続きに使い、片付けます。
PROMPT
)
printf '%s\n' "${prompt//@RUN_ID@/$run}" > <scratchpad>/automation-prompt.txt
cat <scratchpad>/automation-prompt.txt
EOF
```

作る。

```bash
bash -euo pipefail <<'EOF'
orca=${ORCA_CLI_COMMAND:-orca}
repo=$(dirname "$(git rev-parse --path-format=absolute --git-common-dir)")   # 元のリポジトリ
"$orca" automations create --name ebykn-daily-release \
  --trigger daily --time 06:00 --timezone Asia/Tokyo \
  --missed-run-grace-minutes 15 \
  --provider claude --prompt "$(cat <scratchpad>/automation-prompt.txt)" \
  --repo "path:$repo" --workspace-mode new-per-run --base-branch origin/develop --setup skip \
  --enabled --json
EOF
```

### 作ったあとに確かめる

作成の出力の `.result.automation.id`（最上位の `.id` は要求の id で、オートメーションの id ではない）で設定を確かめ、一度走らせる。

```bash
bash -euo pipefail <<'EOF'
orca=${ORCA_CLI_COMMAND:-orca}
"$orca" automations show <id> --json
"$orca" automations run <id> --json
"$orca" automations runs --id <id> --json
EOF
```

- `show` で、時刻・タイムゾーン・猶予・ワークツリーの作り方・元のブランチ・プロンプトの Run の id が、上の表のとおりかを見る。
- `run` で一度走らせる。合格は、確認や拒否なしに手順 9 まで届き、コーディネーターに知らせが届いたときだけ。リリースが切られても、「develop が進んでいない」で止まってもよい。`gh` の認証がオートメーションの端末で効くことも、この回で確かめる。
- 権限で止まったり、知らせが届かなかったりしたら、合格としない。「止まった回を続ける」で残りを行い、作者が権限を決めたあとでもう一度走らせる。
- `runs` と `automations list` の JSON の形（回の状態とワークスペースの欄）を見て、「見回り」の読み方と合っているかを確かめる。

### 手で切る前に

オートメーションが走っている間は、手で切らない。同じ識別子を 2 つの回で切り合うため。最新の回が終わっているかを確かめる。

```bash
bash -euo pipefail <<'EOF'
orca=${ORCA_CLI_COMMAND:-orca}
"$orca" automations runs --id <id> --limit 1 --json
EOF
```

最新の回が終わっていない状態（実行中か待ち）なら、終わるまで待つ。

### コーディネーターが替わったら

コーディネーターの Run が替わると、プロンプトの Run の id が古くなり、毎朝の回が手順 0 で Run を確かめられなくなる（リリースは切るが、知らせが届かない）。新しいコーディネーターは、上の「プロンプトを書き出す」のブロックを自分のワークスペースで走らせてから、プロンプトを直す。

```bash
bash -euo pipefail <<'EOF'
orca=${ORCA_CLI_COMMAND:-orca}
"$orca" automations edit <id> --prompt "$(cat <scratchpad>/automation-prompt.txt)" --json
"$orca" automations show <id> --json
EOF
```
