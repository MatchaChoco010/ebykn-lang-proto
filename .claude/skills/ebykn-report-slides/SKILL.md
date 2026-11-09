---
name: ebykn-report-slides
description: ebykn-lang-proto のリリースの報告スライド（PDF）を作る手順。範囲（前回のタグか main と、今回のコミット）を受け取り、develop に入った PR と Issue を読んで解釈し、修正前と修正後を実際に動かした実例を載せ、yomiyasu（--doc-type slides）で推敲し、固定した Typst で組んで、ページごとに画像で確かめる。リリースを切るとき（ebykn-release から）や、報告スライドを作り直すときに使う。
---

# 報告スライドを作る

作者は毎日は見ず、都合のよいときに数日分のスライドをまとめて読み、気になったところを質問し、`release` ブランチで動作を確かめてから `main` にマージします。スライドだけで、何が入ったか、なぜ入ったか、どう試せばよいかが分かるようにします。

スライドにも、呼び出し元への報告にも、作者に確認やレビューを促す文言（「確認をお願いします」など）は書きません。リリースのたびに作者を急かさないためです。

このスキルは「範囲を受け取って PDF を作る」ことだけを受け持ちます。タグ、Release、`release` → `main` の PR は `ebykn-release` スキルが受け持ちます。

## 道具

| 置き場 | 中身 |
| --- | --- |
| `xtask/src/slides/mod.rs`、`pin.rs` | `cargo xtask slides-install`：固定した Typst（単体のバイナリ）とフォント（Noto Sans JP、Noto Sans Mono CJK JP）を sha256 を確かめて `~/.cache/ebykn/` に入れる。一時的なエラーは再試行する |
| 同上 | `cargo xtask slides-build <ディレクトリ>`：`<ディレクトリ>/slides.typ` を `slides.pdf` に組み、はみ出し・空のページ・折り返したコードの行・リンクの数・1 枚の密度・変更ごとの見出し（下の「質の基準」）・Typst の警告・小さすぎる文字・パッケージの読み込みを検査する。どれかがあれば失敗する |
| `xtask/src/slides/selftest.rs` | `cargo xtask slides-selftest`：見本を 1 か所ずつ壊して組み、上の検査が本物のテンプレートで失敗できることを確かめる。CI の `slides` ジョブが走らせる |
| `template/ebykn-slides.typ` | テンプレート（下の「部品」） |
| `template/eyk.sublime-syntax`、`console.sublime-syntax`、`code.tmTheme` | `.eyk` とコマンドの出力の色分け |
| `example/slides.typ` | 部品を一通り使った見本。CI の `slides` ジョブが組む |

組むときはシステムのフォントも Typst に同梱のフォントも使わないので、手元でも CI でも同じ PDF になります。パッケージ（`@preview/..`）は使いません。Typst はパッケージをネットから取ってきて、それを止める引数がないので、原稿かテンプレートにパッケージを読み込む行があると `slides-build` が組む前に失敗します。テンプレートは Typst の標準の機能だけで書きます。版を上げるときは `xtask/src/slides/pin.rs` の固定値と sha256 を書き換え、見本を組み直して目で確かめます。

## 1. 入力を決める

- 識別子：`ebykn-release` から渡される `YYYY-MM-DD.N`。下書きなら `draft-N`。
- 範囲：前回のリリースのタグ（初回は `origin/main`）から、今回のコミット（`develop` の先頭）まで。
- `ebykn-release` から呼ばれたときは、`prs.txt`（入った順の PR 番号）、`issues.txt`（PR が閉じた Issue）、`direct.txt`（PR のないコミット）、`mismatch.txt`（題と PR の食い違い）、`needs-review.json` を受け取る。手順 2 の一覧の代わりにこれを使う。`direct.txt` と `mismatch.txt` に行があれば、一覧の次に 1 枚使って載せる。
- 受け取った一覧は番号だけなので、手順 3 では各 PR と Issue を `gh pr view` / `gh issue view` で読む。`issues.txt` には PR の本文が Refs で挙げた Issue が入らないので、PR の本文から拾う。
- 置き場：`target/reports/<識別子>/`（`target/` はコミットしない）。PDF の元のファイルもここに置く。

```bash
BASE=<前回のタグか origin/main>
HEAD=<今回のコミット>
ID=<識別子>
DIR=target/reports/$ID
mkdir -p $DIR
```

## 2. 集める

develop の first-parent の各コミットが 1 つの PR です（squash でもマージコミットでも同じ）。

```bash
git log --first-parent --reverse --format='%h %s' $BASE..$HEAD
gh api repos/{owner}/{repo}/commits/<コミット>/pulls --jq '.[].number'   # コミットから PR 番号
gh pr view <番号> --json number,title,body,mergeCommit,closingIssuesReferences,files,commits
gh issue view <番号> --json title,body,labels,comments
gh issue list --label "needs review" --state open --json number,title,labels
```

- 並べる順は develop に入った順（first-parent の順）。
- Issue は、PR が閉じたもの（`closingIssuesReferences`）と、本文が Refs で挙げたもの。
- 判断待ちの一覧は、範囲に関わらず開いている `needs review` をすべて載せる。
- 範囲の PR が既定で実装した判断待ちの Issue に、あとから作者の判断が出ていれば（ラベルが外れ、コメントに決定がある）、その判断と、実装がまだかどうかを PR のページに書く。

## 3. 読んで解釈する

PR ごとに、本文、閉じた Issue、差分（`git show --stat`、要所の `git show`）、足したテストを読みます。コミットメッセージを並べるだけにしません。書くことは次の 4 つです。

- 何を：何を足したか、直したか。コンテキストを知らない人に向けて書く。
- なぜ：それが要った理由（Issue の症状、仕様の節、ロードマップの位置）。
- 見出し：その変更の中身を表す短い名前（「Verus 取り込み」「構文解析器」「権限の検査」「Verus への書き下し」など）。マイルストーンのコードネーム（M0、M2 前半、F1 など）は使わない。
- 分類：変わった部分（下の「分類の鍵」）。
- 判断待ち：その PR が既定で実装した `needs review` の Issue と、その既定。

ファイルの何行目を変えたか、という水準の報告は書きません。

読んだり動かしたりする途中で、直したほうがよい問題（読めない診断、誤った出力、壊れたコマンド）を見つけたら、`ebykn-issue` スキルで Issue にします。スライドを作る作業の中では直しません。作った Issue は最後の「スライドを作る途中で見つけた問題」のページに載せ、呼び出し元への報告にも番号を含めます。

## 4. 修正前と修正後の実例を作る

範囲の PR のうち、トランスパイラ（`crates/`、`vendor/` のフォーク、prelude）を変えたものは、実際に動かした結果を載せます。それ以外の PR も、動かして見せられるもの（ゲートの失敗、テスト、ログ）があれば載せます。

```bash
S=$HOME/.cache/ebykn/scratch/release-$ID   # 回ごとに一意。/tmp（tmpfs）には作らない
mkdir -p "$S/wt"
git worktree add --detach "$S/wt/<PR>-before" <マージコミット>^1
git worktree add --detach "$S/wt/<PR>-after"  <マージコミット>
(cd "$S/wt/<PR>-before" && ebykn-limited cargo build -p ebykn)
(cd "$S/wt/<PR>-after" && ebykn-limited cargo build -p ebykn)
```

- 修正前は PR の直前のコミット（マージコミットの first parent）、修正後は PR のコミット。どちらも `ebykn` をビルドして、同じ入力に同じコマンドをかける。
- ワークツリーとビルドは `$S`（`~/.cache/ebykn/scratch/` の下）に作る。scratchpad は `/tmp` の下にあり、このマシンの `/tmp` は tmpfs（メモリの上）なので、ビルドが消すまで RAM を使う。修正前と修正後でビルドが 2 つになるので、なおさら置かない。
- 重い処理（cargo、`ebykn check` / `verify` / `build`、verus、typst）は `ebykn-limited` を前に付け、1 つずつ走らせる。並べて同時に走らせない。
- ビルドはワークツリーごとに、それぞれの `target/` で行う。`CARGO_TARGET_DIR` を共有しない。共有すると、別のコミットのクレート（`ebykn_check` など）が混ざった `ebykn` ができることがある（最初のリリースで起きた）。`ebykn` は、環境変数 `EBYKN_PRELUDE` を設定していなければ、ビルドしたワークツリーの prelude を使う。`EBYKN_PRELUDE` は設定せず、バイナリだけを別の場所に写して使わない。
- 入力の `.eyk` は、PR の golden やテストの入力から、違いが 1 枚に収まる大きさのものを選ぶか、小さく書く。入力も実行したものをそのまま載せる。
- `ebykn verify` は固定した Verus で動かす（そのコミットで `ebykn-limited cargo xtask verus-install`）。
- 修正前のコミットにそのコマンドがなければ、その出力（「不明な引数です」など）を載せて、そう書く。
- コマンドがまだなく、ライブラリにだけ機能があるときは、その関数を呼ぶ使い捨てのプログラムを `$S` に書いて動かし、そう書く。
- レビューで塞いだ穴（黙って誤る、黙って verified になる）は、PR の途中のコミット（`gh pr view --json commits`）で再現できれば、修正前として載せる。
- 終わったら worktree を、`target/` と `build/` ごと消す（`git -C <release を切っているワークツリー> worktree remove --force "$S/wt/<PR>-before"`。今のディレクトリに頼らない）。消すのは、自分が作ったパスを 1 つずつ名前で指定したものだけにする。`git worktree list` をパターン（`scratch` を含むパスなど）で絞って消さない。同じリポジトリには、ほかのセッションのワークツリーもあり、最初のリリースではそれを消してしまった。

## 5. 原稿を書き、yomiyasu で推敲する

スライドの文章を、先に `$DIR/manuscript.md` に書きます。1 枚を 1 つの `##` にし、中身は箇条書きにします。

構成：

1. 表紙：識別子、範囲、日付、PR と Issue の数だけ。内容の要約は書かない。
2. 更新のハイライト：今回の更新全体で、利用者（Ebykn を書く人）から見て何ができるようになったか、どの誤り（誤って通っていた・通らなかったもの）が直ったかを、短い箇条で 3〜5 個。変更の名前や PR の題を並べない。
3. 入った変更の一覧：PR、Issue、内容、分類。develop に入った順。10 行を超えたら複数のページに分ける（`overview` が `per-page` で分ける）。
4. PR ごとに数枚：何を、なぜ、実例、判断待ち。1 つの PR のページはすべて同じ見出しにし、ページごとの題はキャプションにする（原稿では `## 7. Verus 取り込み（1/5）：何を・なぜ（PR #13）` のように書く）。1 枚に載せる量は「質の基準」の密度の目安に従い、超えるならページを増やす。
5. 試し方：作者が利用者として試すことを案内する（下の「試し方を作る」）。ビルドとテストが通ること、`verus-verify`・`golden-verify` など CI が見ている検査は載せない。
6. Ebykn Book の読み方：`mdbook build book` と `book/book/index.html`、`mdbook serve book --open`。いま本文がある章を添える。
7. 判断待ちの Issue：番号、優先度、決めること、今の既定。
8. スライドを作る途中で見つけた問題：手順 3 で作った Issue の番号と内容。なければ「なし」と書く。

### 試し方を作る

作者は、orca のワークスペース `release`（表示名「release（作者の確認用）」。`ebykn-release` の手順 7 が作り、リリースのたびに早送りする）で試します。書いてあるとおりに打てば試せるように、次の順に書きます。

スライドでは、ワークスペースを名前で示します。パスは、ワークスペースがすでにあるときだけ、orca の一覧から取って書きます（推測で書かない）。初回のリリースではまだないので、名前だけを書きます。スライドを作るのは `ebykn-release` の手順 3 で、ワークスペースを作る手順 7 より前だからです。待たずに進めます。

```bash
orca=${ORCA_CLI_COMMAND:-orca}
repo=$(dirname "$(git rev-parse --path-format=absolute --git-common-dir)")
if ! "$orca" worktree list --repo "path:$repo" --limit 10000 --json > "$DIR/worktrees.json"; then
  echo "orca worktree list が失敗した"; cat "$DIR/worktrees.json"; exit 1
fi
[ "$(jq -r '.ok' "$DIR/worktrees.json")" = true ] || { cat "$DIR/worktrees.json"; exit 1; }
[ "$(jq -r '.result.truncated' "$DIR/worktrees.json")" = false ] || { echo "一覧が切れている"; exit 1; }
jq -r '.result.worktrees[] | select((.branch // "") == "refs/heads/release") | .path' "$DIR/worktrees.json"
# 空ならワークスペースはまだない（スライドには名前だけを書く）
```

スライドに載せる出力は、今回のコミット（`$HEAD`）の一時のワークツリーで走らせて取ります。`ebykn-release` の手順 3 の時点では、release のワークスペースは前のリリースのままで、`playground/` には作者が書き換えたファイルがあるかもしれないので、そこでは走らせません。

```bash
S=$HOME/.cache/ebykn/scratch/release-$ID   # 手順 4 と同じ。/tmp（tmpfs）には作らない
REPO=<release を切っているワークツリー>
mkdir -p "$S/wt"
git -C "$REPO" worktree add --detach "$S/wt/try" "$HEAD"
(cd "$S/wt/try" && ebykn-limited cargo xtask verus-install && ebykn-limited cargo xtask playground)
export PATH="$S/wt/try/target/debug:$PATH"
# "$S/wt/try/playground/<例>" でコマンドを打ち、出力を写す。ebykn のコマンドにも ebykn-limited を付け、1 つずつ走らせる
# 出力の中の一時のワークツリーのパスは「…/」と省く
git -C "$REPO" worktree remove --force "$S/wt/try"   # 終わったら、作ったパスを名前で指定して消す
```

どのコマンドも、こうして今回のコミットで実際に走らせ、その出力の例を載せます（長い出力は要点の行に絞り、省いたことを書く）。

1. 準備（最初に 1 回だけ）：ワークスペースに移り、`cargo xtask verus-install` で Verus を入れ（`ebykn verify` と `ebykn build` に要る）、`cargo xtask playground` で `ebykn` をビルドして `examples/` を `playground/` に写し、`target/debug` を `PATH` に通す（`examples/README.md` の「試し方」と同じ）。`ebykn-release` の手順 7 がワークスペースで `cargo xtask playground` を走らせるので、作者が打つときには写してあることが多い。`playground/` は `.gitignore` で無視するので、書き換えても `release` の早送りの邪魔にならない。
2. `ebykn --help`。
3. `playground/` の各例：何を示す例か、どの機能を試せるか、`cd playground/<例>` からの打つコマンドと出力の例を、例ごとに書く。中身は各例の `README.md` に合わせる（README の ```` ```console ```` は CI の `examples-verify` が実行して確かめている）。
   - `examples/` にない機能（新しく入ったサブコマンドなど）を案内するときは、`ebykn init` で `playground/` の下にプロジェクトを作る形で書く。
4. 書き換えて試す：各例の README の「試してみること」から、書き味が分かる変更（契約を変えて `verify` が失敗する、権限の誤りで `check` が拒否するなど）を選び、その出力を載せる。
5. まだ試せないもの：理由と、いつ試せるか（マイルストーンか Issue の番号）。例：付録 B の stack の push / pop の verified は M3。

Ebykn が何であるかの解説（思想や言語の紹介）のページは入れません。

`yomiyasu:yomiyasu` スキルを `--doc-type slides` で呼び、スキルの手順どおり lint → 書き直し（`manuscript.final.md`）→ `run --source manuscript.md --final manuscript.final.md --doc-type slides` → `verify` まで通します。スライドに載せる文章は、verify を通した最終稿と同じにします。組むときに文章を変えたら、最終稿を直して run と verify をやり直します。

## 6. Typst で組む

`$DIR/slides.typ` を書きます。最終稿の文章を、コードの名前に `` ` `` を付け、Issue と PR とコミットをリンクにして、そのまま写します。

```typst
#import "/.claude/skills/ebykn-report-slides/template/ebykn-slides.typ": *
#show: deck.with(release: "2026-11-10.1", base: "6b4075e", head: "8824c9d", date: "2026-11-10")

#cover(stats: ("PR": [8 本], "閉じた Issue": [9 件], "判断待ち": [7 件]))
#highlights[
  - ..
]
#overview(((pr: 4, issues: (3,), areas: ("harness",), heading: [ハーネス], title: [..]), ..))
#change(pr: 20, issues: (17,), areas: ("transpiler", "golden"), heading: [権限の検査], caption: [実例の入力])[..]
#slide(title: [試し方（1/9）：準備（最初に 1 回だけ）])[..]
```

部品：

| 部品 | 使い方 |
| --- | --- |
| `deck` | 文書全体。16:9、フォント、ページ番号、検査の記録 |
| `cover`、`highlights`、`overview`、`slide` | 表紙、更新のハイライト、一覧（見出しは各 PR の 1 枚目へのリンク。`per-page` 行ごとにページを分ける）、素のスライド |
| `change` | PR ごとのスライド。`heading` を H1 に、`caption` をその下の小さな題に出す。先頭に PR と Issue へのリンクと分類を、H1 の横に変更の中での位置（2/5 など）を自動で出す |
| `pr(n)`、`issue(n)`、`commit(sha)` | GitHub へのリンク |
| `code-box(raw, title:, size:)` | コードと出力の箱。` ```eyk `、` ```console `、` ```rust ` で色分けする |
| `before-after(before:, after:, before-title:, after-title:)` | 修正前と修正後を上下に並べる。どちらかが長ければ、修正前と修正後を別のスライドにする |
| `note[..]` | 注記。「なぜ」は箇条にせず `note[*なぜ*：..]` に 1〜2 文で書く |
| `cols(..)` | 文章だけの段組み。コードの箱は入れない |

分類の鍵：`transpiler`（トランスパイラ本体）、`prelude`、`vendor`（取り込みとフォークの手入れ）、`golden`（テスト・golden）、`book`、`harness`（CI・ハーネス）、`formal`（形式モデル）、`rules`（開発ルール・文書）。

Typst の地の文では `#` を `\#` と書きます。`@`、`<`、`_`、`*` を含む名前は `` ` `` で囲みます。` ``` ` を含むコードは ` ```` ` で囲みます。

## 7. PDF を作る

```bash
ebykn-limited cargo xtask slides-install
ebykn-limited cargo xtask slides-build $DIR
```

失敗したら、出力のとおりに直します。

- はみ出し：スライドを分ける。文字を小さくして詰めない。
- 密度（箇条書きの数、コードの行数、段組みの中のコード）：スライドを分ける。
- 見出し：同じ PR のページの `heading` と一覧の `heading` を揃え、同じ PR のページを続けて並べ、コードネームを外す。
- 折り返したコードの行：要点の行に絞るか、`size: 10pt` にする。出力の長い行は手で折り返し、箱の題に「長い行は折り返した」と書く。
- リンクの数の食い違い：`link` を直接使わず、`pr`・`issue`・`commit`・`ext-link` を使う。
- Typst の警告（知らないフォントなど）：警告がなくなるように原稿を直す。
- 文字の大きさ：`size:` や `text(..)` の大きさは pt で 10pt 以上にする。`em` は大きさを確かめられないので使わない。

## 8. 目で確かめる

```bash
rm -rf $DIR/pages && mkdir $DIR/pages
pdftoppm -r 90 -png $DIR/slides.pdf $DIR/pages/p
```

全ページの画像を読み、次を見ます。ページが多いときは、4 枚ずつ 1 つの画像にまとめて読んでもかまいません（`montage <4 枚> -tile 2x2 -geometry +6+6 <出力>`）。気になるページは 1 枚ずつ読み直します。直したら 7 から作り直し、もう一度全ページを見ます。

- 文字やコードが箱や列からはみ出していない。コードの行が途中で切れていない。
- 空のページ、中身が少なすぎるページ、詰め込みすぎのページがない。
- 読めない大きさの文字がない（地の文 12pt 以上、コード 10pt 以上）。
- PR・Issue・コミットのリンクがある（青い下線）。一覧の題からスライドに飛べる。
- 修正前と修正後の色（赤と緑）と見出しが合っている。
- 字形のない文字（絵文字や `⊢` などの記号）が、豆腐の箱（□）で出ていない。固定したフォントにない文字は警告なしに箱になり、`slides-build` も通るので、目でしか見つからない。

## 9. 渡す

元のファイルを 1 つにまとめます。

```bash
tar -czf $DIR/slides-src.tar.gz -C $DIR manuscript.md manuscript.final.md slides.typ
```

`$DIR/slides.pdf` と `$DIR/slides-src.tar.gz` の絶対パス、ページ数、構成の要約、手順 3 で作った Issue の番号を、呼び出し元（`ebykn-release` かコーディネーター）に返します。呼び出し元が置き場を指定したら、2 つのファイルをそこに写します。PDF と元のファイルは Release に添付し、リポジトリにはコミットしません。

## 質の基準

作者の意見で決まったことを、ここに足していきます。次のスライドを作る前に読み、全部を満たしてから渡します。

- [ ] 表紙は識別子・範囲・日付・件数だけ。内容の要約や Ebykn の解説のページはない。
- [ ] 2 枚目が「更新のハイライト」。利用者から見てできるようになったこと、直った誤りを 3〜5 個の短い箇条で書き、変更の名前を並べていない。
- [ ] 一覧は表紙とハイライトの次。1 ページ 10 行までで、超えたら分けている。
- [ ] 1 枚の密度（`slides-build` が検査する）：箇条書きは 5 項目まで（入れ子を含む）。コードの箱は横幅いっぱいに置き、段組みに入れない。6 行を超えるコードの箱は 1 枚に 1 つまで。コードの行は 1 枚に合わせて 16 行まで。
- [ ] 余白がある。長いコードと出力は 1 ページに 1 つずつにし、ページ数が増えるのはかまわない。
- [ ] 1 つの PR / Issue に 1 つの見出し。そのすべてのページの H1 が同じで、ページごとの題はキャプションにしている。同じ変更のページは続けて並び、H1 の横に位置（2/5 など）がある（`slides-build` が検査する）。
- [ ] 見出しは変更の中身を表す短い名前で、コードネーム（M0、F1 など）を使っていない。別の変更と同じ見出しにしていない（`slides-build` が検査する）。
- [ ] 一覧の行も同じ見出しを使い、見出しから各変更の 1 枚目に飛べる（`slides-build` が検査する）。
- [ ] PR ごとに「何を」と「なぜ」がある。コンテキストを知らない人が読んで分かる。
- [ ] コミットメッセージを並べていない。ファイルの行番号の水準の報告をしていない。
- [ ] トランスパイラを変えた PR には、修正前と修正後を実際に動かした入力とコマンドと出力がある。コミットはリンク付きの短い SHA で示す。
- [ ] 出力を省いたら「…（省略）」、手で折り返したら「長い行は折り返した」と書いている。
- [ ] その PR が既定で実装した判断待ちの Issue と、その既定がある。あとから判断が出ていれば、判断と実装の状態がある。
- [ ] 作る途中で見つけた問題は `ebykn-issue` で Issue にし、最後のページと報告に番号を載せた。
- [ ] 試し方は、作者が利用者として試すこと（Book、`ebykn` のコマンド、`examples/`）を案内している。ビルドとテストや、CI が見ている検査は載せていない。
- [ ] 試し方のコマンドは、どのディレクトリで打つかを含め、そのまま打てる形。どれも今回のコミットで走らせ、出力の例を付けた。まだ試せないものは、理由といつ試せるかを書いた。
- [ ] スライドの文章は yomiyasu（`--doc-type slides`）の verify を通した最終稿と同じ。
- [ ] `slides-build` が通り、全ページを画像で確かめた。
