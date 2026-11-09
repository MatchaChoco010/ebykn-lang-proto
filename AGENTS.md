# 開発ルール

Ebykn（`.eyk` / `.eykp` を Verus のソースに書き下すトランスパイラ。Rust 製、コマンドは `ebykn`）の段階 1 プロトタイプです。人間とエージェントの両方がこのルールに従います。

要点：

- 正典は `docs/` の 3 文書と、元の仕様を上書きする現行仕様（`docs/current-spec.md`）。仕様が最優先で、現行仕様に書いた項目は元の仕様より優先する。矛盾と未決事項は `OPEN.md` に記録し、既定に従って進む。
- ブランチは 3 本。すべての作業は `develop` に入り、`release` はリリースを切った時点の `develop`、`main` は作者が確かめたものだけ。
- 作業は orca のワークツリーの `feature/<Issue 番号>-<短い名前>` で行い、`develop` に PR を出す。コーディネーターのワークスペースは指示を待つ。1 Issue に 1 PR。
- 人間の判断が要るものは `needs review` の Issue にし、番号を報告する。自分で確定しない。
- PR 本文は日本語で書き、yomiyasu で推敲してから投稿する。ワーカーは PR をマージしない。条件を満たす PR はコーディネーターが `develop` にマージする。作者の承認は待たない。
- リリースは毎朝 6 時に Orca のオートメーションが切る。作者は都合のよいときに報告スライドで確かめ、`release` を `main` にマージする。作者にレビューを催促しない。問題が見つかったら、直す PR を `develop` に積む。
- ゲートは、失敗できると示してから信用する。
- このマシンは作者が毎日使う。重い処理は `ebykn-limited` の下で 1 つずつ走らせ、ビルドを `/tmp` に置かない（「マシンの資源」）。

## 正典と優先順位

| 文書 | 内容 |
| --- | --- |
| [docs/ebykn-spec.md](docs/ebykn-spec.md) | 実装仕様。何を作るか |
| [docs/ebykn-design-notes.md](docs/ebykn-design-notes.md) | 判断基準・決定の記録・却下した案 |
| [docs/ebykn-thesis.md](docs/ebykn-thesis.md) | 思想 |
| [docs/current-spec.md](docs/current-spec.md) | 現行仕様。元の仕様（`docs/ebykn-spec.md`）からの差分を、仕様の節の順に書く。エージェントが更新する |

- `docs/current-spec.md` に書いてある項目は、`docs/ebykn-spec.md` より優先する。書いていない項目は `docs/ebykn-spec.md` のまま。
- 作者の判断で仕様が変わったら、その変更を入れる PR で `docs/current-spec.md` を更新する。基準で決めたことのうち元の仕様を変えるもの（元の仕様と違う形にした、元の仕様が定めていないことを決めた）、判断待ちの既定で実装が元の仕様と違うもの、仕様にない診断のコード、実測で変わったことも書く。まだ `develop` に入っていない項目には「実装中（PR #N）」を添え、入れる PR で外す。
- `OPEN.md` との分け方：`docs/current-spec.md` は仕様の差分（規範。何を作るか）。`OPEN.md` は実装の読み方（仕様の文言から一意に読めない点をどう読んだか）、未決の既定、Verus の実測の記録。同じことを両方に書かない。
- 仕様と設計ノートが矛盾したら仕様に従い、矛盾を [OPEN.md](OPEN.md) に記録する。
- 仕様の `OPEN-n` は自分で決めない。各項の既定に従い、気づいたことを `OPEN.md` に書く（仕様の冒頭）。
- 迷ったときは設計ノート §3.3 の手順で決める：仕様を読む → §3.1・§3.2 の基準で決めて `OPEN.md` に記録する（元の仕様を変えるなら `docs/current-spec.md` に書く）→ §5 に当たる案は選ばない → 作者に聞くのは、後戻りできず基準で決まらないときだけ。
- 設計ノート §5 の却下した案は再提案しない。
- 3 文書の例は実装を始める前のラフスケッチ。実装の過程で別の形が要るようになったら、例に合わせて設計を歪めない。
  - 例として扱うのは、付録や本文のコード、表の中の具体的な書き方だけ。仕様の規則と `OPEN-n` の既定には従う（§3.3 の 1 の「書いてあれば従う」は規則のこと）。規則そのものが設計として誤っていると考えるなら `needs review` にする。
  - 設計ノート §3.1・§3.2 の基準で、設計として正しい方を選ぶ（基準で決まらなければ §3.3 の 4）。
  - 例を通すための暗黙の変換や規則を足さない（#11 の、付録の `.first()` を書き下しで `.first_opt()` に置き換える案がこれに当たる）。
  - 例との食い違いは、どの例か・どの形にしたか・なぜかを `docs/current-spec.md` の「付録の例との食い違い」に記録する。
  - golden とテストは正しい設計の形で書く。
  - `needs review` の推奨の理由に「3 文書の例と合う」を挙げない。
- 3 文書はエージェントが変更しない。`docs/current-spec.md` に記録した例との食い違いは作者が見て、3 文書を直すかどうかを決める。

## 作業の場所

| ブランチ | 中身 | 誰が入れるか |
| --- | --- | --- |
| `main` | 作者が確かめて承認したものだけ | 作者だけ。リリースの PR（`release` → `main`）をマージコミットでマージする |
| `release` | リリースを切った時点の `develop` | リリースを切るエージェント（毎朝のオートメーションか、コーディネーター）。`develop` の先頭まで早送りするだけ |
| `develop` | すべての作業が入る統合ブランチ | コーディネーター。作者の承認なしでマージする |
| `feature/<Issue 番号>-<短い名前>` | 1 つの Issue の作業 | ワーカー。`develop` に PR を出す |

- GitHub の既定のブランチは `develop`。`gh pr create` で `--base` を省くと `develop` に向くが、手順では `--base develop` を明示する。PR の本文の `Closes #N` は、`develop` へのマージで Issue を閉じる。`release` → `main` の PR では効かないので書かない。
- `main`・`release`・`develop` はずっと使い続け、履歴を書き換えない（force push しない）。どれにも直接コミットしない。
- リポジトリは public で、3 本のブランチを GitHub の ruleset で守っている。ruleset は誰も迂回できない（迂回できる人を置いていない）。

| ブランチ | ruleset |
| --- | --- |
| `develop` | 削除と force push を禁止。PR が必須で、マージの方法は squash とマージコミットだけ。`ci-required` が必須で、PR の head が `develop` の先頭に追いついていること（strict） |
| `release` | 削除と force push を禁止。PR は要らず、早送りの直接の push は通る（`ebykn-release` の手順 6） |
| `main` | 削除と force push を禁止。PR が必須で、マージの方法はマージコミットだけ。`ci-required` が必須（strict ではない。`release` は `main` のマージコミットを含まないので、追いつかせると `release` に余計なコミットが入る） |

- リポジトリの設定：自動マージを許す。マージした PR の head のブランチを GitHub が自動で消す（`release` は ruleset が削除を禁じているので消えない）。update-branch を許す（「常に更新を勧める」設定で、PR の head がベースに遅れていると、画面に「Update branch」のボタンが出る。`release` → `main` の PR では押さない。「作者の確認」）。rebase のマージは使えない。
- `develop` を strict にしたのは、PR の CI がほかの PR のマージより前に走ると、その結果のままマージした `develop` が赤くなりうるため（PR #57 と #89 の README の件）。
- 問題が見つかったら、直す変更を新しい PR として `develop` の先頭に積む。過去の履歴は直さない（「見つけた問題」）。
- コーディネーターのワークスペースは、ユーザーの指示を受けるために待機させる。作業は orca のワークツリーで行う。
- orca のワークツリーの元は `origin/develop`。ワークツリー名は `<種類>-<Issue 番号>-<短い名前>`（例：`harness-3-vibe-lang-structure`、`fix-12-linearity-loop`）。例外は 2 つ（「リリースと作者の確認」）。作者の確認用のワークスペース `release` は、元が `origin/release` で、ブランチは `release`。毎朝のリリースのオートメーションが走るたびに作るワークツリーは、名前とブランチを Orca が付ける（元は `origin/develop`）。
- orca はブランチ名に `<GitHub ユーザー名>/` を付けるので、ワーカーは作業の最初にブランチ名を変える（`git branch -m feature/<Issue 番号>-<短い名前>`）。
- 1 つの Issue に 1 つの PR。

## Issue

Issue には 3 つの軸を独立に付ける。付けるまでが起票です。

| 軸 | ラベル | 意味 |
| --- | --- | --- |
| 種類 | `bug` / `enhancement` / `documentation` / `epic` / `usability` / `spec` / `harness` / `formal` / `dogfooding` | 何の Issue か |
| 優先度 | `P0` / `P1` / `P2` | 壊れ方 |
| 順序 | `blocker` | 他の open な Issue がこれに依存している |

優先度は壊れ方だけで決める。「重要そう」「欲しい」は入れない。

| 優先度 | 判定 | Ebykn での例 |
| --- | --- | --- |
| P0 | 黙って誤る | `ebykn status` が verified と表示するのに前提が抜けている。書き下しで意味が変わる。通すべきでない `.eykp` が剥がし一致検査を通る。検証の失敗が成功として報告される |
| P1 | 落ちる・書けない | 正しいプログラムが拒否される。診断が行動につながらない。生成した Verus が構文エラーになる |
| P2 | 機能追加 | 今動いているものは何も壊れていない |

着手順は機械的に決まる：

```text
1. 全 P0
2. blocker の P1
3. 残りの P1
4. 部分木を選び、その blocker から
```

- `blocker` は「その部分木で最初にやるもの」で、「今すぐやるもの」ではない。
- `epic` は索引。本文には現在地と子 Issue への索引だけを置き、経緯はコメントに書く。作業の単位は sub-issue。ロードマップは #1。
- Issue の作り方は [.claude/skills/ebykn-issue/SKILL.md](.claude/skills/ebykn-issue/SKILL.md)。テンプレートは `.github/ISSUE_TEMPLATE/`。
- コミットや PR の本文で、閉じないことを言うときに `#N` を閉じる動詞（close / fix / resolve）の後に書かない。GitHub は否定を読まずに閉じる。

## 人間の判断が要るもの

人間の判断が要るものは、すべて `needs review` ラベルの Issue にする。本文だけで判断できるよう、文脈を全部載せる。見出しは設計ノート §2 の「決めるべき事項の提示」と同じ：

1. 何を決めるか
2. なぜ判断が要るか
3. 採ると何が変わるか
4. 案（各案の利点と欠点。案は複数）
5. 推奨（確定しない）
6. 判断が出るまでの既定

作ったら、コーディネーター（ワーカーなら worker_done の報告）を通して、ユーザーに番号を報告する。

設計ノート §3.3 の 4 との分け方：

- 基準と既定で進められるもの → `OPEN.md` に記録して進む。元の仕様を変えるなら `docs/current-spec.md` に書く。
- `needs review` にするもの → 使ってみて見つけた使い勝手の悪さ・冗長さ、既定のない仕様の矛盾、基準で決まらない判断。
- 判断を待つ間も、書いた既定で作業を続ける。

## 見つけた問題

問題は、主に次の経路で見つかる。

| 経路 | 誰が見つけるか |
| --- | --- |
| dogfooding（「dogfooding」） | Ebykn を実際に使うエージェント |
| レビュー（「マージ」） | コーディネーターが起動したレビューのサブエージェント |
| 作者の質問と確認（「リリースと作者の確認」） | 作者。`release` のワークスペースのエージェントか、コーディネーターが受ける |
| 報告スライドの作成 | 範囲の PR を動かして実例を作るエージェント |

どの経路で見つかっても、扱いは同じ：

1. [ebykn-issue](.claude/skills/ebykn-issue/SKILL.md) のスキルで Issue にする。本文には、見つけた経路と、分かればどのリリース（`report/YYYY-MM-DD.N`）で見つかったかを書く。
2. 直す PR を `develop` に積む。過去の履歴もリリースのタグも直さない。直したものは次のリリースに入る。
3. 作者の判断が要るものだけを `needs review` にする（「人間の判断が要るもの」）。仕様の矛盾や未決事項で、基準と既定で決まるものは、`needs review` にせず `OPEN.md` に記録して進む（元の仕様を変えるなら `docs/current-spec.md` に書く）。バグは `OPEN.md` に書かず、いつも Issue にする。
4. 同じ種類の問題がまた起こり得る構造なら、「再発防止」のとおり検出する仕組みも足す。

- レビューの指摘のうち、そのレビューの PR の中で直すものは Issue にしない。PR の範囲の外で見つかったものを Issue にする。
- 見つかった問題は、リリースを止めない。リリースは、切った時点の `develop` のまま進める。例外は、`develop` の先頭の CI が赤いときと、見つかった問題のせいで報告スライドの PDF を作れないときで、このときは止まってコーディネーターに知らせる。報告スライドを作る途中で見つかった Issue は、そのリリースの Release の本文と、リリースの PR のコメントに載せる。

## PR

手順（[.claude/skills/ebykn-pr/SKILL.md](.claude/skills/ebykn-pr/SKILL.md) と同じ）：

1. `cargo xtask ci` を通す。
2. PR 本文を日本語で書き、ファイルに保存する。変更点、確認したこと、決めた既定、関連 Issue（`Closes #N`）を書く。
3. yomiyasu スキル（`yomiyasu:yomiyasu`）を呼び、PR 向けの設定（`--doc-type pr`）で推敲する。スキルの手順どおり lint → 書き直し → `run --final ... --doc-type pr` → `verify` まで通し、verify を通した最終稿を変えずに使う。推敲は必須。
4. `gh pr create --base develop --head feature/<Issue 番号>-<短い名前> --title "<日本語の題>" --body-file <最終稿>`。
5. `gh pr checks <番号> --watch` で CI を見届ける。落ちたら直して push し、`ci-required` が緑になるまで繰り返す。`develop` に rebase して push し直したときも、CI を見届け直す（必須のチェックは head のコミットごとに結び付くので、前の緑は新しい head に効かない）。コーディネーターが update-branch で追いつかせたあとの PR を直すときは、リモートの feature ブランチに足されたマージのコミットを取り込んでから push する（[ebykn-pr](.claude/skills/ebykn-pr/SKILL.md) の 5）。
6. ワーカーは PR をマージしない。マージは次の「マージ」に従ってコーディネーターが行う。

### マージ

コーディネーターは、次をすべて満たす PR を `develop` にマージする。作者の承認は待たない。

- `ci-required` が緑。
- PR を書いたワーカーとは別に、コーディネーターが起動したサブエージェントがコードレビューし、指摘が残っていない。指摘はワーカーに送って直させ、直したら再レビューする。自動マージを予約したあとでワーカーに PR を戻すとき（直しや rebase を頼むとき）は、先に `gh pr merge <番号> --disable-auto` で予約を外し、再レビューのあとで予約し直す。どのエージェントも同じアカウントなので、予約したまま直しを push すると、GitHub は予約を外さず、再レビューなしでマージする。

マージの仕方：

- squash でマージする。コミットの題は GitHub が付ける既定のまま変えない。題の末尾には `(#N)` が付く（題の本体は PR の題。PR のコミットが 1 つなら、そのコミットの題）。リリースのときに、この `(#N)` と GitHub の記録を突き合わせる。
- 取り込みのコミットを含む PR は例外で、マージコミットでマージする（「外部から取り込むコード」）。題は既定の `Merge pull request #N from ...` のまま。
- `develop` は strict なので、PR の head が `develop` の先頭に追いついていなければ、緑でもマージできない。衝突がなければ（`mergeStateStatus` が `BEHIND`）、コーディネーターが `gh pr update-branch <番号>` で追いつかせ、`gh pr merge <番号> --squash --auto` で自動マージを予約する。update-branch のあとに CI がもう一度走り、`ci-required` が緑になると GitHub がマージする。
- `develop` と衝突したら（`DIRTY`）、update-branch に頼らず、ワーカーが feature ブランチを `develop` に rebase して push し直し、CI を見届け直す。
- `needs review` の Issue の判断は、マージを止めない。既定で実装してマージし、作者の判断が既定と違えば、作り直しの PR を積む。
- 開発ルールやゲートを変える PR も、同じ条件でマージする。3 文書をエージェントが変更しない規則はそのまま。

`release` へは早送りだけで進め、`main` へは作者がマージコミットでマージする（次の「リリースと作者の確認」）。`release` → `main` の PR では update-branch を使わない（`main` のマージコミットが `release` に入る）。

マージ後（自動マージを予約したなら、PR の状態が `MERGED` になったのを `gh pr view <番号> --json state` で確かめてから）、コーディネーターはワーカーを解放し（orca の release）、ワークツリーを削除する。リモートのブランチは GitHub が消す。マージが済む前にブランチを消すと、PR が閉じてしまう。

マージ後、コーディネーターは PR の `closingIssuesReferences`（`gh pr view <番号> --json closingIssuesReferences`）の Issue が閉じたかを確かめ、閉じていなければ `PR #<番号> で develop に入った` とコメントして閉じる。コマンドは [.claude/skills/ebykn-pr/SKILL.md](.claude/skills/ebykn-pr/SKILL.md) の 6。

## リリースと作者の確認

コーディネーターとワーカーは手を止めずに作ってマージし続け、作者は都合のよいときに、あとからまとめて確かめる。リリースの手順は [.claude/skills/ebykn-release/SKILL.md](.claude/skills/ebykn-release/SKILL.md)。

### 毎朝のリリース

- リリースは、Orca のオートメーションが毎朝 6:00（日本時間）に起こすエージェントが切る。Orca は起動しているときだけオートメーションを走らせるので、Orca が起動していない日は切らない。逃した回は、短い猶予を過ぎたら取り戻さない。
- オートメーションはリポジトリの外にある Orca の設定で、コーディネーターが作る。作り方とプロンプトの全文はスキルの「毎朝のリリースのオートメーション」。プロンプトには、知らせの宛先としてコーディネーターの Run の id を書き込む。コーディネーターが替わったら、プロンプトを直す。
- エージェントは、走るたびに `origin/develop` から作る新しいワークツリーで `ebykn-release` を実行する。`develop` が前回のリリースから進んでいなければ、何もせずに終わる。
- 作者が見つけた問題を直したあとなどは、コーディネーターが同じ手順で手で切ってもよい。1 日に何度切ってもよい。識別子は `YYYY-MM-DD.N`（日本時間の日付と、その日の何回目か）。オートメーションが走っている間は、手で切らない（最新の回が終わったかを `orca automations runs --id <id> --limit 1 --json` で確かめる）。
- 切るときは、`develop` の先頭のコミットを決めて CI が緑であることを確かめ、前回のタグからそこまでに `develop` に入った PR と、それが閉じた Issue を集め、報告スライドの PDF を作る（`ebykn-report-slides` スキル）。スライドはリリースごとに 1 つ作り、前のものを上書きしない。
- 取り消せない操作は最後にまとめる。PDF ができてから注釈付きのタグ `report/YYYY-MM-DD.N` を付けて Release（prerelease）に PDF を添付し、そのあとで `release` をそのコミットまで早送りする。`release` の先頭は、いつも報告済みのタグになる。言語のリリースのタグ（`v0.1.0` など）とは名前空間を分ける。
- `release` → `main` の PR がなければ作り、あれば本文の「確認待ちのリリース」の一覧を最新にして、今回のリリースのコメントを足す。作者がマージするまで、毎日のリリースがこの 1 つの PR にたまる。本文には、いちばん新しい報告済みのタグのコミットを書く。
- 作者が動作を確かめるための orca のワークスペース `release` を置く。`release` ブランチを取り出したワークツリーで、初めてリリースを切るときに作り、以後リリースを切るたびに `origin/release` まで早送りする（`git merge --ff-only`。スキルの手順 7）。作業ツリーに変更があれば、作者が試しに書いたものかもしれないので、捨てずに早送りをやめ、リリースの PR のコメントと手順 9 の知らせに書く。
- 終わったら（途中で止まったときも）、エージェントはコーディネーターの Run に結果を知らせる（`orca orchestration send --to run:<Run の id>`。スキルの手順 9）。コーディネーターは、見つかった Issue を直す作業を割り振り、止まった回は残りの手順を自分で続け（スキルの「止まった回を続ける」）、使い終わったワークツリーを片付ける。
- 無人で最後まで進むには、取り消せない手順（タグと `release` の push、Release の作成、リリースの PR の作成・更新・コメント）を確認なしで行える権限が要る。権限をどうするか（Orca の UI でオートメーションの権限のモードを設定するか、プロジェクトの設定で許可するか）は作者が決める。決めるまでは、エージェントは権限の判定で拒否されたらそこで止まって知らせ、コーディネーターが残りを行う。エージェントもコーディネーターも、権限の設定のファイルを変えない。
- 知らせが届かない回（Orca の終了、スリープ、権限の確認待ちなど）は、ワークツリーが残る。コーディネーターは、知らせが届かなかった日と手で切る前に見回り（スキルの「見回り」）、続きを行ってワークツリーを片付ける。

### 作者に催促しない

- リリースのたびに作者へ報告しない。結果は Release と、リリースの PR の本文とコメントに残すだけにする。PR を見るよう促したり、確認を急かしたりしない。作者が確かめるのは、作者の都合のよいとき。
- リリースが止まって作者の操作が要るとき（前回のリリースの PR が squash で入った、`release` のワークスペースに作者のコミットが残っているなど）も、催促しない。エージェントは手順 9 の知らせに書き、リリースの PR まで進んだ回は PR のコメントにも書く（前回の PR が squash で入ったときは、手順 1 で止まるので、PR はまだない）。コーディネーターは、作者が次に話しかけたときに伝える。
- 作者が確かめなかった日は、`release` は `main` にマージされないまま、次のリリースの分が積み上がる。

### 作者の確認

- 作者は、都合のよいときにスライドを読み、`release` のワークスペースでエージェントを起動して質問し、ターミナルで動作を確かめる。そのエージェントは [.claude/skills/ebykn-release-workspace/SKILL.md](.claude/skills/ebykn-release-workspace/SKILL.md) に従う。質問に答え、作者が直したいことを Issue にしてコーディネーターに知らせる。`release` のワークスペースでは実装しない。
- 問題がなければ、作者がリリースの PR を `main` にマージコミットでマージする。報告していない範囲を入れないよう、`gh pr merge <番号> --merge --match-head-commit <本文にあるコミット>` でマージする（GitHub の画面でマージするなら、PR の先頭がそのコミットであることを確かめる）。確認待ちのリリースが複数あれば、まとめて入る。`main` の ruleset はマージコミットだけを許す（squash や rebase をすると、次のリリースの PR で同じ変更が差分にもう一度出るため）。`ci-required` が緑でなければ、ruleset がマージを止める。
- 作者は、リリースの PR の「Update branch」のボタンを押さない。`main` に最初にマージしたあとは、ボタンが出る。押すと `main` のマージコミットが `release` に入る。早送りなので ruleset は通すが、force push を禁じているので戻しにくく、次のリリースの手順 6（`release` の早送り）が止まる。リリースの PR の本文にも書いてある。
- 作者が問題を伝えたら、コーディネーターは「見つけた問題」のとおり Issue と直す PR を `develop` に積む。直したものは次の朝のリリースに入る。急ぐときは、コーディネーターが手でリリースを切る。スライドは前回のタグからの差分になる。

## コマンド

| コマンド | 内容 |
| --- | --- |
| `cargo xtask precommit` | `cargo fmt --check` と全ゲート。pre-commit フックが呼ぶ。速く保つ |
| `cargo xtask ci` | `cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、全ゲート。PR の前に通す |
| `cargo xtask gates` | 全ゲート。個別には `check-file-size` / `check-md-links` / `check-md-tables` / `check-book` / `check-gate-tests` / `check-verus-pin` / `check-prelude-exports` / `check-examples` / `check-attr-names` |
| `cargo xtask verus-install` | 固定した Verus（ルートの `Cargo.toml` の `[workspace.metadata.verus]`）を `~/.cache/ebykn/verus/<版>/` に入れ、sha256 を確かめ、Verus が要求する toolchain を rustup で入れる |
| `cargo xtask verus-verify` | `ebykn_prelude` と §6.2 の参照モデル（`formal/linearity`）を `--no-cheating` で検証し、Verus なしの cargo でもビルドする。参照モデルは、定理（`xtask/src/verus.rs` の `REQUIRED`）が検証されたことを名前で確かめる |
| `cargo xtask golden-verify` | golden の各ケース（`tests/golden/*/expected/status.txt` のあるもの）に `ebykn verify` をかけて期待と比べ、書き出した `build/verus/` を Verus なしの cargo と `ebykn build` の両方でビルドし、成功を期待する（検証に失敗した項目があっても）。`expected/build-error.txt` のあるケースは、両方がその行を出して失敗することを期待する（`ebykn build` だけが止まるケースは `expected/ebykn-build-error.txt`）。続けて、Verus が要るので `#[ignore]` にしたテスト（`xtask/src/golden.rs` の `IGNORED`）を走らせる |
| `cargo xtask examples-verify` | `examples/` の各例を `target/examples/` に写し、README の ` ```console ` のコマンドを実行して、出力を README と比べる |
| `cargo xtask playground` | `ebykn` をビルドし、`examples/` を git が追跡しない `playground/` に写す。すでにあるファイルは上書きしない |
| `cargo xtask verus-probes` | 設計ノート §7.2 の前提を確かめるプローブ（`tests/verus-probes/`）を走らせ、各ファイルの先頭の期待と比べる |
| `cargo xtask formal-oracle` | 参照モデルから判定の表を作り直し、コミットされた表（`formal/oracle/`）と比べる |
| `cargo xtask formal-oracle-write` | 参照モデルから判定の表を作り直し、`formal/oracle/` に書く。モデルを変えたら作り直してコミットする |
| `cargo xtask slides-install` | 報告スライドの道具（`xtask/src/slides/pin.rs` で固定した Typst とフォント）を `~/.cache/ebykn/` に入れ、sha256 を確かめる |
| `cargo xtask slides-build <ディレクトリ>` | 報告スライドの `<ディレクトリ>/slides.typ` を `slides.pdf` に組み、はみ出し・密度・見出し・リンクを検査する（`ebykn-report-slides` スキル） |
| `cargo xtask slides-selftest` | スキルの見本を 1 か所ずつ壊して組み、`slides-build` の検査が失敗できることを確かめる（CI の `slides` ジョブ） |
| `cargo test --workspace` | テスト |
| `mdbook build book` | Ebykn Book を `book/book/` に作る（mdBook 0.5.4） |

- pre-commit フックは `git config core.hooksPath .githooks` で有効にする。フックは、`ebykn-limited` があればその下で `cargo xtask precommit` を走らせる（「マシンの資源」）。Claude Code では SessionStart フック（`.claude/hooks/session-start.sh`）が未設定なら設定する。
- toolchain は `rust-toolchain.toml`（1.96.0）。Verus は自分の toolchain（固定値の `toolchain`）で動き、`verus-*` のコマンドがそれを指定する。Verus のクレートはルートの Cargo ワークスペースから `exclude` してある。
- Verus の版を上げるときは固定値を書き換え、`verus-install`・`verus-verify`・`verus-probes`・`formal-oracle` を通す。プローブの期待が崩れたら、依拠した前提が変わったということなので `OPEN.md` の「M0 の実測」と `docs/current-spec.md` を見直す。
- CI（`.github/workflows/ci.yml`）の必須チェックは `ci-required` だけ。全ジョブを `needs` に持つ集約ジョブで、ジョブを足したら `needs` にも足す。CI の時間を短く保つため、ジョブはキャッシュを使い、トリガーは `pull_request` と、`main`・`develop` への `push` だけにする。`develop` への `push` は、リリースを切る前に `develop` の先頭が緑であることを確かめるため（`develop` の ruleset は strict なので、PR の CI は `develop` の先頭との組み合わせを試しているが、念のため、マージしたあとの先頭そのものも確かめる）。

## マシンの資源

このマシンは、作者が毎日使う作業用のマシンです。エージェントの処理で、作者の作業を重くしない。

- 重い処理（cargo のコマンド、Verus、z3、Typst）は、`ebykn-limited` があれば、その下で走らせる（`ebykn-limited cargo xtask ci`）。`ebykn-limited` は、空きメモリと重い処理の席の数が条件を満たすまで待ってから、CPU とメモリに上限のある systemd の slice の中で走らせる。`ebykn-limited` がない環境（CI など）では、そのまま走らせる。
- 重い処理を、自分で並列に走らせない。バックグラウンドに回して次を始めたり、サブエージェントに同時に走らせたりしない。同時に走る数は `ebykn-limited` が決める。
- 使い捨てのワークツリーとビルドは `~/.cache/ebykn/scratch/<一意の名前>/` に置く。`/tmp` は tmpfs でメモリを使うので、ビルドの置き場にしない（Claude Code の scratchpad も `/tmp` の下にある）。
- 消すのは、自分が作ったパスだけ。1 つずつ名前で指定して消す。一覧（`git worktree list`、`ls` など）をパターンで絞って消さない。`git worktree prune` も使わない。同じリポジトリと `scratch/` には、ほかのセッションのワークツリーもある。

## ゲートの規則

通ったゲートは、失敗できると分かるまで何も意味しない（vibe-lang #2248 の教訓：あるゲートは壊れた入力に 5 回 ok を返し、5 回とも人間が見つけた）。

- 新しいゲートは `xtask/src/gates/` のモジュールにし、`cargo xtask` のサブコマンドとして登録する。
- 新しいゲートには、実入力を壊すと失敗することを主張するテスト（`#[test] fn fails_..`）を付ける。`check-gate-tests` が強制する。
- そのテストは、壊す操作が実際に当たったことを先に確かめる（`testutil::mutate`）。何にも当たらない変更は、通りながら何も証明しない。
- ゲートは読めない入力について黙らない。解釈できない行、閉じていないフェンス、読めないファイルは失敗にする。沈黙は安全ではなく未検査で、両者は見分けがつかない。
- ゲートは実行環境に依存しない。環境変数や、手元にだけあるツール・ファイルで結果を変えない（存在の判定はディスクではなく `git ls-files` の一覧で行う）。
- 対象外は明示的なリストで持ち、出力に出す。リストにあるのに存在しないものは失敗にする。
- テストの選択を絞る仕組みを作るなら、判定できないときは全件に倒す。「判定できなかった」と「影響なし」を同じ緑にしない。

## 再発防止

レビューやバグ修正で、同じ種類の問題が今後も起こり得る構造を見つけたら、その場限りの修正で終わらせず、決定的に検出する仕組みを足す。

- 言語の意味として判定できるものは `ebykn check` に入れる。
- リポジトリの規則は `cargo xtask` のゲートにする。
- 検証したコードと配るコードを食い違わせる書き方（cfg の名前、マクロ）は、`crates/ebykn_syntax/src/denylist.rs` の `DENYLIST` に、壊す理由と再現の例を添えて 1 項目足す（`crates/ebykn_check/tests/denylist.rs` の総当たりが自動で効く）。
- Red → Green で、違反例と非違反例をテストに固定する。

## Ebykn Book

`book/` は *The Rust Programming Language* に倣った Ebykn Book（mdBook）です。日本語で書く（英語版は作らない）。章は `book/src/SUMMARY.md` に並べ、本文のない章は下書き章（`- [題名]()`）にする。本文を書くときにファイルを作る。

本の中のコード例はすべて CI で検査する。フェンスの種別：

| フェンス | 意味 |
| --- | --- |
| ` ```eyk check ` | `ebykn check` が通る |
| ` ```eyk verify ` | `ebykn verify` が通り、直後の ` ```status ` ブロックと一致する |
| ` ```eyk fail ` | 直後の ` ```diag ` ブロックの診断で失敗する |
| ` ```eyk skip ` | 実行しない。1 行目を `// skip(#Issue 番号): 理由` にする |
| ` ```console ` | CLI の実行例 |

- 種別のない ` ```eyk ` と、言語のないフェンスは拒否する。
- `ebykn` が例を実行できるのは M2 から。それまで `check-book` は形だけを検査し、実行していない数を出力に出す。
- `skip` は意図して実行しない例（未実装の構文など）だけに使い、追跡 Issue を付ける。テストが壊れたからといって `skip` に移さない。

## 例のプロジェクト

`examples/` は、作者がすぐ試せる Ebykn のプロジェクトです（使い方は [examples/README.md](examples/README.md)）。機能が増えたら、それを示す例を `examples/` に足します。README の ` ```console ` のブロックは `examples-verify` が CI で実行するので、期待する出力は実際に動かして書きます。

## dogfooding

M2（stack が verified になる）以降、機能追加とバグ修正がマージされるたびに、設計ノート §1.2 の用途を実際に Ebykn で書く（#1 の D）。

- 見つけたものは「見つけた問題」のとおり扱う。バグは修正の PR を積む。使い勝手の悪さや冗長さは、作者の判断が要るので `needs review` の Issue にする（テンプレート `usability.md`）。
- 記録の置き場（`eval/`）は最初の回で作る。

## 形式モデル

Ebykn 自身が判定する規則の参照モデルを Verus で書いて検証し、実装との差分テストのオラクルにする（#1 の F）。対象は §6.2 の線形性、§6.1 の剥がし一致、§8.2 の合成、§9.1 の状態の伝播だけ。トランスパイル先の健全性は Verus が受け持つ。置き場は `formal/` で、今あるのは §6.2 のモデル（`formal/linearity`）と判定の表（`formal/oracle/`）。何を証明し何を証明していないかは [formal/README.md](formal/README.md) に書く。

## 文書の方針

- 文書は腐る。設計が変わったら最終形に書き直すか消す。「古い」「置き換えられた」といった注記を残さない。経緯は `git log` が唯一の記録。
- 指示されていないファイルを作らない（設計ノート §2）。作るなら PR 本文で何のファイルかを明示する。プレースホルダーだけの文書や、まだ使わない仕組みは作らない。
- `CLAUDE.md` は置かない（Claude Code は `AGENTS.md` を読む）。

## 言語

- Issue、PR、文書、コミットメッセージは日本語。
- 識別子は英語。コードのコメントは日本語でよい。

## ファイルの大きさ

ソースファイル（追跡されている `.rs` / `.md` / `.toml` / `.yml` / `.sh`）は 1 ファイル 2000 行までに保つ。`check-file-size` が検査する。対象外は `docs/ebykn-*.md` の 3 文書と、`vendor/` の下の取り込んだコードだけで、`xtask/src/gates/file_size.rs` の `EXEMPT` と `EXEMPT_DIRS` に明示してある。

## 外部から取り込むコード

外部のコードは `vendor/<名前>/` に、上流のコミットを記録して、編集せずそのまま取り込む。取り込みは取り込みだけのコミットにし、その時点で中身を上流と同じにする（ファイルを消さない、整形しない）。

| 置き場 | 上流 | コミット |
| --- | --- | --- |
| `vendor/verus_syn/` | [verus-lang/verus](https://github.com/verus-lang/verus) の `dependencies/syn` | `8bf68ee741df833ea5d0fe6a2255126fdea87e47`（`release/0.2026.11.02.8bf68ee`） |

- `vendor/` は Cargo のワークスペースのメンバーにしない（`exclude`）。path 依存で使う。`cargo fmt --check`（`--all` を付けない。`--all` は path 依存も整形する）と `clippy --workspace` は `vendor/` を整形・検査しない。
- `vendor/` は行数のゲートの対象外。ディレクトリ単位で `EXEMPT_DIRS` に明示する。
- 取り込みのコミットを含む PR は squash せず、マージコミットでマージする（取り込みだけのコミットを `develop` と、その先の `main` に残すため）。
- フォークとして手を入れる場合は、取り込みとは別のコミットで、必要な箇所だけにする。変更した箇所には `EBYKN:` で始まるコメントを付ける。上流を取り込み直すときは、取り込みのコミットとの差分を当て直す。
- 自分たちの部分は、できるだけ `vendor/` の外に置く（`verus_syn` なら `crates/ebykn_syntax`）。
