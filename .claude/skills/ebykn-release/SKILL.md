---
name: ebykn-release
description: ebykn-lang-proto でリリースを切る手順。develop の先頭が緑か確かめ、範囲の PR と Issue を集めて報告スライドの PDF を作り、タグ report/YYYY-MM-DD.N と Release を出してから release を早送りし、作者の確認用の release のワークスペースを進め、release → main の PR を更新して作者に報告する。リリースを切るとき（基本 1 日 1 回、作者が見つけた問題を直したあと）、途中で止まったリリースを再開するとき、作者が main にマージしたあとに使う。
---

# リリースを切る

コーディネーターが行う。ブランチの役割と作者の確認は AGENTS.md の「作業の場所」と「リリースと作者の確認」。

- 取り消せない操作（タグの push、Release、`release` の早送り）は、確認と PDF が済んでから手順 4〜6 でまとめて行う。手順 1〜3 で止まっても、リモートには何も残らない。
- `release` は早送りだけで進める。force push しない。直接コミットしない。`release` の先頭は、いつも Release のある報告済みのタグにする。
- 押したタグと Release は消さない。手順 6 までのどこかで止まったら、手順 1 からやり直す。手順 1 が、止まったリリースを見つけて同じ識別子で再開する。手順 7 以降で止まったら、その手順からやり直す（`release` はもうタグまで進んでいるので、手順 1 は新しいリリースとみなす）。
- コマンドのブロックは、どれも `bash -euo pipefail <<'EOF'` … `EOF` で包んで打つ。Bash ツールのシェルは zsh のことがあり、zsh では `"$sha:refs/..."` の `:r` が修飾子として展開されるため。ブロックの中でも、`:` の前の変数は `${sha}` のように括る。
- シェルの変数はブロックをまたいで残らないので、手順 1 で作る `vars` を各ブロックの最初に読み込む。`<scratchpad>` と `<id>` は実際の値に置き換える。

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

dir=<scratchpad>/release-$id
# 新しく切るときは、前に止まった回の出力（古い PDF など）を消す。リモートにはまだ何もない
[ "$mode" != new ] || rm -rf "$dir"
mkdir -p "$dir"
printf 'mode=%s\nid=%s\nsha=%s\nprev=%s\nprev_sha=%s\ndir=%s\n' \
  "$mode" "$id" "$sha" "$prev" "$prev_sha" "$dir" > "$dir/vars"
cat "$dir/vars"
EOF
```

- 「止まる」が出たら（`exit 1` で終わる）、そこで止まって報告する。`develop` が進んでいなければ、リリースは切らない。
- 既定のブランチが `develop` でなければ止まる。GitHub の `commits/<sha>/pulls` は、既定のブランチの上のコミットにしか、マージ済みの PR を返さない。既定のブランチが変わると、手順 2 で範囲のすべてのコミットが「PR のないコミット」に見える。
- 前回のリリースの PR が `main` に squash か rebase で入っていたら止まる。次の PR の差分に同じ変更がもう一度出るので、作者に報告する。
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
- `direct.txt`：PR のないコミット（`develop` への直接のコミット）。履歴は書き換えないので、止まらずに「PR のないコミット」としてスライドと報告に載せ、作者に知らせる。
- `mismatch.txt`：題の番号と GitHub の PR が食い違うコミット（題を書き換えたマージなど）。PR は GitHub の側を使い、食い違いをスライドと報告に載せる。
- 範囲の行数が `commits.txt` = `prs.txt` + `direct.txt` になっていることを確かめる。
- Issue は、閉じたかどうか（Issue の状態や `closedByPullRequestsReferences`）ではなく、PR の `closingIssuesReferences` から集める。Issue はあとから開き直されたり、手で閉じられたりするためで、PR が何を閉じると言ったかは PR の側にしか残らない。
- `gh` が通信の失敗で落ちたら、このブロックをやり直す。

## 3. 報告スライドの PDF を作る

`ebykn-report-slides` スキルを呼ぶ。渡すもの：識別子 `$id`、範囲（`$prev`・`$prev_sha` と `$sha`）、`prs.txt`、`issues.txt`、`direct.txt`、`mismatch.txt`、`needs-review.json`。PDF と、その元のファイルを `$dir` に受け取る。

PDF の作り方はそのスキルが受け持つ。スライドはリリースごとに 1 つ作り、前のものを上書きしない。PDF ができなければ、ここで止まって報告する（まだ何も押していない）。

範囲の PR を動かして実例を作る途中で問題が見つかったら、AGENTS.md の「見つけた問題」のとおり扱う。

- `ebykn-issue` スキルで Issue にし、本文に `report/$id` のスライドを作る途中で見つかったと書く。番号を `$dir/found.txt` に 1 行に 1 つ書く。手順 5 の Release の本文、手順 8 の PR のコメント、手順 9 の報告に載せる。
- リリースは止めない。範囲は手順 1 で決めた `$sha` のまま進める。直す PR は `develop` に積み、次のリリースに入る。
- 見つかった問題のせいで PDF が作れないときだけ、ここで止まって報告する。

## 4. タグを付ける

`mode=resume-release` なら飛ばす。注釈付きのタグにし、メッセージに前回のコミットを 40 桁で残す（手順 1 の再開が読む）。

```bash
bash -euo pipefail <<'EOF'
. <scratchpad>/release-<id>/vars
printf '報告 %s\n\n前回：%s（%s）\n今回：%s\n' "$id" "$prev" "$prev_sha" "$sha" \
  | git tag -a "report/${id}" -F - "$sha"
git push origin "refs/tags/report/${id}"
EOF
```

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

# orca のワークスペースのうち、release を取り出しているもの・表示名が同じもの・パスが /release で終わるもの
if ! "$orca" worktree list --repo "path:$repo" --limit 10000 --json > "$dir/worktrees.json"; then
  echo "止まる：orca worktree list が失敗した"; cat "$dir/worktrees.json"; exit 1
fi
[ "$(jq -r '.ok' "$dir/worktrees.json")" = true ] || { echo "止まる：orca worktree list が失敗した"; cat "$dir/worktrees.json"; exit 1; }
[ "$(jq -r '.result.truncated' "$dir/worktrees.json")" = false ] || { echo "止まる：orca worktree list の一覧が切れている"; exit 1; }
# 一覧に入っていないホストがあると、空の答えは「ワークスペースがない」ことを示さない
[ "$(jq '.result.hostScope.omittedHostIds | length' "$dir/worktrees.json")" = 0 ] \
  || { echo "止まる：orca worktree list が調べていないホストがある"; jq -c '.result.hostScope' "$dir/worktrees.json"; exit 1; }
jq -c --arg d "$display" '[.result.worktrees[]
  | select(.branch == "refs/heads/release" or .displayName == $d or (.path | endswith("/release")))
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
| 1 件で、`branch` が `refs/heads/release` でない | — | 止まる。作者がワークスペースで別のブランチに切り替えたのかもしれない。触らずに報告する |
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

[ "$(git -C "$ws" symbolic-ref --short HEAD)" = release ] \
  || { echo "止まる：$ws が release ではなく $(git -C "$ws" symbolic-ref --short HEAD) を取り出した"; exit 1; }
[ "$(git -C "$ws" rev-parse --abbrev-ref 'release@{upstream}')" = origin/release ] \
  || { echo "止まる：$ws の release の上流が origin/release ではない"; exit 1; }
[ "$(git -C "$ws" rev-parse HEAD)" = "$sha" ] || { echo "止まる：$ws の HEAD が ${sha} ではない"; exit 1; }

if ! "$orca" worktree set --worktree "path:$ws" --display-name "$display" --json > "$dir/release-set.json"; then
  echo "止まる：orca worktree set が失敗した"; cat "$dir/release-set.json"; exit 1
fi
[ "$(jq -r '.ok' "$dir/release-set.json")" = true ] || { echo "止まる：orca worktree set が失敗した"; cat "$dir/release-set.json"; exit 1; }
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
- ローカルの `release` に `origin/release` にないコミットがあって止まったら、作者に、そのコミットが要るかを聞く。要らなければ、作者の了承を得てから `git branch -D release` で消し、この手順をやり直す。

### 早送りする（2 回目以降）

```bash
bash -euo pipefail <<'EOF'
. <scratchpad>/release-<id>/vars
display='release（作者の確認用）'
[ "$(jq 'length' "$dir/release-ws.json")" = 1 ] || { echo "止まる：release のワークスペースが 1 つではない"; exit 1; }
ws=$(jq -r '.[0].path' "$dir/release-ws.json")
printf 'ws=%s\nws_display=%s\n' "$ws" "$display" > "$dir/release-ws.txt"

[ "$(git -C "$ws" symbolic-ref --short HEAD)" = release ] || { echo "止まる：$ws が release を取り出していない"; exit 1; }
if [ -n "$(git -C "$ws" status --porcelain --untracked-files=all)" ]; then
  echo "止まる：$ws の作業ツリーに変更がある"
  git -C "$ws" status --short --untracked-files=all
  exit 1
fi
git -C "$ws" merge-base --is-ancestor HEAD origin/release \
  || { echo "止まる：$ws の release に origin/release にないコミットがある"; git -C "$ws" log --oneline origin/release..HEAD; exit 1; }

git -C "$ws" merge --ff-only origin/release
[ "$(git -C "$ws" rev-parse HEAD)" = "$sha" ] || { echo "止まる：$ws の HEAD が ${sha} ではない"; exit 1; }
echo 'ws_state=早送りした' >> "$dir/release-ws.txt"
cat "$dir/release-ws.txt"
EOF
```

- 作業ツリーに変更（追跡していないファイルを含む）があれば、捨てずに止まる。作者が試しに書いたものかもしれないため。`git stash`・`git checkout -- .`・`git clean` はしない。
- 作者がワークスペースでコミットしていたら（`release` に `origin/release` にないコミットがある）、同じく止まる。
- ここで止まっても、タグ・Release・`release` の早送りは済んでいる。ワークスペースは進めずに手順 8 に進み、手順 9 の報告に、止まった理由と `status` か `log` の出力を書く。作者が片づけたら、この手順だけをやり直す（何度やり直してもよい）。

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

`release` を進めると、この PR の CI が走る。`ci-required` が緑になるのを見届けてから報告する。

## 9. 作者に報告する

次の形で報告する。

```text
リリース report/<id> を切りました。
- 範囲：<前回のタグ>（<前回のコミット>）..<コミット>（PR <件数> 件、閉じた Issue <件数> 件）
- Release（報告スライド）：<Release の URL>
- リリースの PR：<PR の URL>（確認待ち：report/<id>, …）
- マージするとき：gh pr merge <PR 番号> --merge --match-head-commit <sha>
- 判断待ちの Issue：#<番号> <題>, …（なければ「なし」）
- PR のないコミット・題と PR の食い違い：<direct.txt と mismatch.txt の中身>（なければ「なし」）
- スライドを作る途中で見つかった Issue：#<番号> <題>, …（found.txt。なければ「なし」）
- release のワークスペース：<表示名>（<パス>）。<作った / 早送りした / 進めていない：理由>
- 確かめ方：スライドの最後のページ。release のワークスペースでターミナルを開くと、今回のコミットで動きます。
```

- ワークスペースの行は、手順 7 の `release-ws.txt` から書く。手順 7 で止まったときは「進めていない」とし、止まった理由と `status` か `log` の出力を添える。作者に片づけてもらう。

## 作者が問題を見つけたとき

AGENTS.md の「見つけた問題」のとおり扱う。

1. 問題を Issue にする（`ebykn-issue` スキル）。どのリリースで見つかったかを本文に書く。
2. 直す PR を `develop` に積む。ワーカーに作らせ、AGENTS.md の「マージ」のとおりマージする。過去の履歴は直さない。
3. もう一度リリースを切る（この手順の 1 から）。同じ日なら `N` が 1 つ増える。スライドは前回のタグからの差分になる。

## 作者が main にマージしたあと

- リリースの PR は閉じる。次のリリースの手順 8 では開いた PR がないので、新しく作る。
- 新しい PR の「確認待ちのリリース」には、`main` に入っていないタグだけが載る。
- 次のリリースの手順 1 で、前回の PR がマージコミットで入ったことを確かめる。
