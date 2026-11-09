---
name: ebykn-release
description: ebykn-lang-proto でリリースを切る手順。develop の先頭が緑か確かめ、release を早送りしてタグ report/YYYY-MM-DD.N を付け、範囲の PR と Issue を集めて報告スライドの PDF を作り、Release と release → main の PR に出して作者に報告する。リリースを切るとき（基本 1 日 1 回、作者が見つけた問題を直したあと）と、作者が main にマージしたあとに使う。
---

# リリースを切る

コーディネーターが行う。ブランチの役割と作者の確認は AGENTS.md の「作業の場所」と「リリースと作者の確認」。

- `release` は早送りだけで進める。force push しない。直接コミットしない。
- 押したタグと Release は消さない。途中で止まったら、同じ識別子で止まった手順からやり直す。
- 下のコマンドは bash で打つ。シェルの変数は呼び出しをまたいで残らないので、手順 1 で作る `vars` を毎回読み込む。

## 1. 事前の確認と識別子

```bash
git fetch origin --prune --tags
sha=$(git rev-parse origin/develop)
gh run list --workflow ci.yml --event push --branch develop --commit "$sha" \
  --json databaseId,status,conclusion
```

`develop` の先頭のコミットで、push で走った CI を確かめる。

| 結果 | すること |
| --- | --- |
| `completed` / `success` | 進む |
| `queued` / `in_progress` | `gh run watch <databaseId> --exit-status` で待ち、もう一度確かめる |
| 空 | push の CI がまだ始まっていない。数分待って、もう一度確かめる |
| `cancelled` | 新しい push で取り消された。`git fetch` からやり直す |
| それ以外（`failure` など） | 止まって報告する。直す PR を `develop` に積み、緑になってから切る |

PR の CI は、ほかの PR が先にマージされたあとの組み合わせを試していないので、この確認を飛ばさない。

識別子と前回のタグを決める。日付は日本時間、`N` はその日の何回目か。

```bash
today=$(TZ=Asia/Tokyo date +%F)
n=$(( $(git tag --list "report/$today.*" | wc -l) + 1 ))
id="$today.$n"
prev=$(git tag --list 'report/*' --sort=-v:refname | head -n1)
[ -n "$prev" ] || prev=origin/main   # 初回は main から
prev_sha=$(git rev-parse "$prev^{commit}")
git merge-base --is-ancestor "$prev_sha" "$sha" || { echo "止まる：$prev が develop の祖先ではない"; exit 1; }
[ "$prev_sha" != "$sha" ] || { echo "止まる：前回のリリースから develop が進んでいない"; exit 1; }
dir=<scratchpad>/release-$id
mkdir -p "$dir"
printf 'id=%s\nsha=%s\nprev=%s\nprev_sha=%s\ndir=%s\n' "$id" "$sha" "$prev" "$prev_sha" "$dir" > "$dir/vars"
```

- 「止まる」が出たら（`exit 1` で終わる）、そこで止まって報告する。`develop` が進んでいなければ、リリースは切らない。
- 以降は、手順 1 で確かめた `$sha` だけを使う。途中で `develop` が進んでも、今回のリリースには入れない。

## 2. release を早送りする

```bash
. <scratchpad>/release-<id>/vars
if git ls-remote --exit-code --heads origin release > /dev/null; then
  git merge-base --is-ancestor origin/release "$sha" || { echo "止まる：release が develop の祖先ではない"; exit 1; }
fi
git push origin "$sha:refs/heads/release"
```

- `release` がなければ（初回）、`ls-remote` が 2 で終わり、`push` が `develop` の先頭から `release` を作る。
- `release` が `develop` の祖先でなければ、誰かが `release` に直接書いた。push せず止まって報告する。
- `--force` は付けない。早送りでない push はサーバーが拒否する。

## 3. タグを付ける

注釈付きのタグにする。

```bash
. <scratchpad>/release-<id>/vars
git tag -a "report/$id" -m "報告 $id（$prev..$(git rev-parse --short=12 "$sha")）" "$sha"
git push origin "report/$id"
```

前回のタグは、手順 1 のとおり `git tag --list 'report/*' --sort=-v:refname` の先頭（`2026-11-09.10` は `2026-11-09.9` より後に並ぶ）。言語のリリースのタグ（`v0.1.0` など）は `report/` の外なので混ざらない。

## 4. 範囲の PR と Issue を集める

`develop` の first-parent の各コミットは、1 つの PR に当たる。squash のコミットは題の末尾が `(#N)`、取り込みの PR のマージコミットは題が `Merge pull request #N from ...`（AGENTS.md の「マージ」）。

```bash
. <scratchpad>/release-<id>/vars
git log --first-parent --reverse --format='%H %s' "$prev_sha..$sha" > "$dir/commits.txt"

# コミットの題から、コミットと PR 番号の組を、develop に入った順に取る
sed -E -n \
  -e 's/^([0-9a-f]{40}) Merge pull request #([0-9]+) from .*$/\1 \2/p' -e t \
  -e 's/^([0-9a-f]{40}) .*\(#([0-9]+)\)$/\1 \2/p' \
  "$dir/commits.txt" > "$dir/from-git.txt"

# GitHub から、develop にマージされた PR のうち、マージのコミットが範囲にあるものを取る
gh pr list --base develop --state merged --limit 1000 --json number,mergeCommit \
  --jq '.[] | "\(.mergeCommit.oid) \(.number)"' \
  | awk 'NR == FNR { c[$1] = 1; next } c[$1]' "$dir/commits.txt" - > "$dir/from-gh.txt"

# 3 つが食い違ったら止まる
diff <(LC_ALL=C sort "$dir/from-git.txt") <(LC_ALL=C sort "$dir/from-gh.txt") \
  && diff <(cut -d' ' -f1 "$dir/commits.txt" | LC_ALL=C sort) <(cut -d' ' -f1 "$dir/from-gh.txt" | LC_ALL=C sort) \
  && cut -d' ' -f2 "$dir/from-git.txt" > "$dir/prs.txt"
```

- 1 つめの `diff`：題から取った組と GitHub の組が同じか。どちらかにしかないものがあれば止まる。
- 2 つめの `diff`：範囲のどのコミットにも PR があるか。PR のないコミットは、`develop` への直接のコミットか、マージの仕方の誤り。止まって報告する。
- `prs.txt` は、`develop` に入った順の PR 番号。

PR が閉じた Issue と、判断待ちの Issue を集める。

```bash
. <scratchpad>/release-<id>/vars
while read -r n; do
  gh pr view "$n" --json body,closingIssuesReferences \
    --jq '.closingIssuesReferences[].number, .body' \
    | grep -oiE '(^[0-9]+$)|\b(close[sd]?|fix(e[sd])?|resolve[sd]?)[[:space:]]+#[0-9]+' \
    | grep -oE '[0-9]+$'
done < "$dir/prs.txt" | awk '!seen[$0]++' > "$dir/issues.txt"
gh issue list --label "needs review" --state open --limit 200 --json number,title,url > "$dir/needs-review.json"
```

既定のブランチは `main` なので、`develop` への PR では `closingIssuesReferences` が空になる。そのため本文の `Closes #N`（close / fix / resolve とその活用形）も読み、両方を合わせる。

## 5. 報告スライドの PDF を作る

`ebykn-report-slides` スキルを呼ぶ。渡すもの：識別子 `$id`、範囲（`$prev` と `$sha`）、`prs.txt`、`issues.txt`、`needs-review.json`。PDF と、その元のファイルを `$dir` に受け取る。

PDF の作り方はそのスキルが受け持つ。スライドはリリースごとに 1 つ作り、前のものを上書きしない。

## 6. Release を作る

タグ `report/$id` の prerelease として、PDF とその元のファイルを添付する。元のファイルが複数なら、1 つの `tar.gz` にまとめて添付する。

```bash
. <scratchpad>/release-<id>/vars
{
  echo "範囲：\`$prev\`..\`$(git rev-parse --short=12 "$sha")\`"
  echo
  echo "## PR"
  echo
  while read -r n; do gh pr view "$n" --json number,title --jq '"- #\(.number) \(.title)"'; done < "$dir/prs.txt"
  echo
  echo "## 閉じた Issue"
  echo
  while read -r n; do gh issue view "$n" --json number,title --jq '"- #\(.number) \(.title)"'; done < "$dir/issues.txt"
} > "$dir/release-notes.md"
gh release create "report/$id" --verify-tag --prerelease \
  --title "報告 $id" --notes-file "$dir/release-notes.md" \
  "$dir/<PDF>#報告スライド（PDF）" "$dir/<元のファイル>#報告スライドの元のファイル"
gh release view "report/$id" --json url --jq .url
```

`--verify-tag` で、手順 3 で押したタグがなければ失敗させる（`gh` に別のタグを作らせない）。

## 7. release → main の PR

まず、前回のリリースの PR が `main` にマージコミットで入ったかを確かめる。

```bash
. <scratchpad>/release-<id>/vars
git fetch origin --prune
pr=$(gh pr list --base main --head release --state open --json number --jq '.[0].number // empty')
last=$(gh pr list --base main --head release --state merged --limit 1 --json headRefOid --jq '.[0].headRefOid // empty')
[ -z "$last" ] || git merge-base --is-ancestor "$last" origin/main || { echo "止まる：前回のリリースの PR が main にマージコミットで入っていない"; exit 1; }
git tag --list 'report/*' --merged "$sha" --no-merged origin/main --sort=v:refname   # 確認待ちのリリース
```

- 「止まる」が出たら、`main` が squash か rebase でマージされた。次の PR の差分に同じ変更がもう一度出るので、止まって作者に報告する。
- 最後のコマンドの一覧が、本文の「確認待ちのリリース」に載せるものの全部。

本文とコメントを書き、`yomiyasu:yomiyasu` スキルの `--doc-type pr` で推敲する（lint → 書き直し → `run --final <ファイル> --doc-type pr` → `verify`）。verify を通した最終稿を変えずに使う。

本文（PR がなければ新しく書き、あれば今の本文 `gh pr view "$pr" --json body --jq .body` を元に直す）：

```markdown
`release` を `main` に入れる PR です。作者が報告スライドで確かめてから、マージコミットでマージします。squash や rebase はしません。

## 確認待ちのリリース

- [report/<id>](<Release の URL>)：<1 文の要約>（PR <件数> 件）

## 判断待ちの Issue

- #<番号> <題>
```

- 「確認待ちのリリース」は、上の `git tag` の一覧と同じにする。前からある行はそのまま残し、今回の行を足す。一覧にないタグの行（`main` に入ったもの）は消す。作者は確認待ちのリリースを複数まとめてマージするので、一覧はいつも最新にする。
- 「判断待ちの Issue」は `needs-review.json` で書き直す。

コメント（今回のリリースの分）：

```markdown
## report/<id>

- Release：<Release の URL>（報告スライドの PDF）
- 範囲：`<前回のタグ>`..`<コミット>`
- PR：#<番号>, …（<件数> 件）
- 閉じた Issue：#<番号>, …

<何が入ったかを 2〜4 文で>
```

```bash
# PR がないとき
gh pr create --base main --head release --title "確認待ちのリリースを main に入れる" --body-file <本文の最終稿>
# PR があるとき
gh pr edit "$pr" --body-file <本文の最終稿>
# どちらでも
gh pr comment <PR 番号> --body-file <コメントの最終稿>
gh pr checks <PR 番号> --watch
```

`release` を進めると、この PR の CI が走る。`ci-required` が緑になるのを見届けてから報告する。

## 8. 作者に報告する

次の形で報告する。

```text
リリース report/<id> を切りました。
- 範囲：<前回のタグ>..<コミット>（PR <件数> 件、閉じた Issue <件数> 件）
- Release（報告スライド）：<Release の URL>
- リリースの PR：<PR の URL>（確認待ち：report/<id>, …）
- 判断待ちの Issue：#<番号> <題>, …（なければ「なし」）
- 確かめ方：スライドの最後のページ。`release` ブランチで動きます。
```

## 作者が問題を見つけたとき

1. 問題を Issue にする（`ebykn-issue` スキル）。どのリリースで見つかったかを本文に書く。
2. 直す PR を `develop` に積む。ワーカーに作らせ、AGENTS.md の「マージ」のとおりマージする。過去の履歴は直さない。
3. もう一度リリースを切る（この手順の 1 から）。同じ日なら `N` が 1 つ増える。スライドは前回のタグからの差分になる。

## 作者が main にマージしたあと

- リリースの PR は閉じる。次のリリースの手順 7 では開いた PR がないので、新しく作る。
- 新しい PR の「確認待ちのリリース」には、`main` に入っていないタグだけが載る。
- 手順 7 の最初の確認で、前回の PR がマージコミットで入ったことを確かめる。
