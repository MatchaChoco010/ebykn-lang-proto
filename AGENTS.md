# 開発ルール

Ebykn（`.eyk` / `.eykp` を Verus のソースに書き下すトランスパイラ。Rust 製、コマンドは `ebykn`）の段階 1 プロトタイプです。人間とエージェントの両方がこのルールに従います。

要点：

- 正典は `docs/` の 3 文書。仕様が最優先。矛盾と未決事項は `OPEN.md` に記録し、既定に従って進む。
- ブランチは 3 本。すべての作業は `develop` に入り、`release` はリリースを切った時点の `develop`、`main` は作者が確かめたものだけ。
- 作業は orca のワークツリーの `feature/<Issue 番号>-<短い名前>` で行い、`develop` に PR を出す。コーディネーターのワークスペースは指示を待つ。1 Issue に 1 PR。
- 人間の判断が要るものは `needs review` の Issue にし、番号を報告する。自分で確定しない。
- PR 本文は日本語で書き、yomiyasu で推敲してから投稿する。ワーカーは PR をマージしない。条件を満たす PR はコーディネーターが `develop` にマージする。作者の承認は待たない。
- 作者はリリースごとの報告スライドであとから確かめ、`release` を `main` にマージする。問題が見つかったら、直す PR を `develop` に積む。
- ゲートは、失敗できると示してから信用する。

## 正典と優先順位

| 文書 | 内容 |
| --- | --- |
| [docs/ebykn-spec.md](docs/ebykn-spec.md) | 実装仕様。何を作るか |
| [docs/ebykn-design-notes.md](docs/ebykn-design-notes.md) | 判断基準・決定の記録・却下した案 |
| [docs/ebykn-thesis.md](docs/ebykn-thesis.md) | 思想 |

- 仕様と設計ノートが矛盾したら仕様に従い、矛盾を [OPEN.md](OPEN.md) に記録する。
- 仕様の `OPEN-n` は自分で決めない。各項の既定に従い、気づいたことを `OPEN.md` に書く（仕様の冒頭）。
- 迷ったときは設計ノート §3.3 の手順で決める：仕様を読む → §3.1・§3.2 の基準で決めて `OPEN.md` に記録する → §5 に当たる案は選ばない → 作者に聞くのは、後戻りできず基準で決まらないときだけ。
- 設計ノート §5 の却下した案は再提案しない。
- 3 文書の例は実装を始める前のラフスケッチ。実装の過程で別の形が要るようになったら、例に合わせて設計を歪めない。
  - 例として扱うのは、付録や本文のコード、表の中の具体的な書き方だけ。仕様の規則と `OPEN-n` の既定には従う（§3.3 の 1 の「書いてあれば従う」は規則のこと）。規則そのものが設計として誤っていると考えるなら `needs review` にする。
  - 設計ノート §3.1・§3.2 の基準で、設計として正しい方を選ぶ（基準で決まらなければ §3.3 の 4）。
  - 例を通すための暗黙の変換や規則を足さない（#11 の、付録の `.first()` を書き下しで `.first_opt()` に置き換える案がこれに当たる）。
  - 例との食い違いは、どの例か・どの形にしたか・なぜかを `OPEN.md` に記録する。
  - golden とテストは正しい設計の形で書く。
  - `needs review` の推奨の理由に「3 文書の例と合う」を挙げない。
- 3 文書はエージェントが変更しない。`OPEN.md` に記録した例との食い違いは作者が見て、3 文書を直すかどうかを決める。

## 作業の場所

| ブランチ | 中身 | 誰が入れるか |
| --- | --- | --- |
| `main` | 作者が確かめて承認したものだけ | 作者だけ。リリースの PR（`release` → `main`）をマージコミットでマージする |
| `release` | リリースを切った時点の `develop` | コーディネーター。リリースを切るときに `develop` の先頭まで早送りするだけ |
| `develop` | すべての作業が入る統合ブランチ | コーディネーター。作者の承認なしでマージする |
| `feature/<Issue 番号>-<短い名前>` | 1 つの Issue の作業 | ワーカー。`develop` に PR を出す |

- GitHub の既定のブランチは `develop`。`gh pr create` で `--base` を省くと `develop` に向くが、手順では `--base develop` を明示する。PR の本文の `Closes #N` は、`develop` へのマージで Issue を閉じる。`release` → `main` の PR では効かないので書かない。
- `main`・`release`・`develop` はずっと使い続け、履歴を書き換えない（force push しない）。どれにも直接コミットしない。
- 問題が見つかったら、直す変更を新しい PR として `develop` の先頭に積む。過去の履歴は直さない（「見つけた問題」）。
- コーディネーターのワークスペースは、ユーザーの指示を受けるために待機させる。作業は orca のワークツリーで行う。
- orca のワークツリーの元は `origin/develop`。ワークツリー名は `<種類>-<Issue 番号>-<短い名前>`（例：`harness-3-vibe-lang-structure`、`fix-12-linearity-loop`）。例外は、作者の確認用のワークスペース `release` だけ（「リリースと作者の確認」）。元は `origin/release` で、ブランチは `release`。
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

- 基準と既定で進められるもの → `OPEN.md` に記録して進む。
- `needs review` にするもの → 使ってみて見つけた使い勝手の悪さ・冗長さ、既定のない仕様の矛盾、基準で決まらない判断。
- 判断を待つ間も、書いた既定で作業を続ける。

## 見つけた問題

問題は、主に次の経路で見つかる。

| 経路 | 誰が見つけるか |
| --- | --- |
| dogfooding（「dogfooding」） | Ebykn を実際に使うエージェント |
| レビュー（「マージ」） | コーディネーターが起動したレビューのサブエージェント |
| 作者の質問と確認（「リリースと作者の確認」） | 作者。コーディネーターが受ける |
| 報告スライドの作成 | 範囲の PR を動かして実例を作るエージェント |

どの経路で見つかっても、扱いは同じ：

1. [ebykn-issue](.claude/skills/ebykn-issue/SKILL.md) のスキルで Issue にする。本文には、見つけた経路と、分かればどのリリース（`report/YYYY-MM-DD.N`）で見つかったかを書く。
2. 直す PR を `develop` に積む。過去の履歴もリリースのタグも直さない。直したものは次のリリースに入る。
3. 作者の判断が要るものだけを `needs review` にする（「人間の判断が要るもの」）。仕様の矛盾や未決事項で、基準と既定で決まるものは、`needs review` にせず `OPEN.md` に記録して進む。バグは `OPEN.md` に書かず、いつも Issue にする。
4. 同じ種類の問題がまた起こり得る構造なら、「再発防止」のとおり検出する仕組みも足す。

- レビューの指摘のうち、そのレビューの PR の中で直すものは Issue にしない。PR の範囲の外で見つかったものを Issue にする。
- 見つかった問題は、リリースを止めない。リリースは、切った時点の `develop` のまま進める。例外は、`develop` の先頭の CI が赤いときと、見つかった問題のせいで報告スライドの PDF を作れないときで、このときは止まって報告する。報告スライドを作る途中で見つかった Issue は、そのリリースの作者への報告に含める。

## PR

手順（[.claude/skills/ebykn-pr/SKILL.md](.claude/skills/ebykn-pr/SKILL.md) と同じ）：

1. `cargo xtask ci` を通す。
2. PR 本文を日本語で書き、ファイルに保存する。変更点、確認したこと、決めた既定、関連 Issue（`Closes #N`）を書く。
3. yomiyasu スキル（`yomiyasu:yomiyasu`）を呼び、PR 向けの設定（`--doc-type pr`）で推敲する。スキルの手順どおり lint → 書き直し → `run --final ... --doc-type pr` → `verify` まで通し、verify を通した最終稿を変えずに使う。推敲は必須。
4. `gh pr create --base develop --head feature/<Issue 番号>-<短い名前> --title "<日本語の題>" --body-file <最終稿>`。
5. `gh pr checks <番号> --watch` で CI を見届ける。落ちたら直して push し、`ci-required` が緑になるまで繰り返す。
6. ワーカーは PR をマージしない。マージは次の「マージ」に従ってコーディネーターが行う。

### マージ

コーディネーターは、次をすべて満たす PR を `develop` にマージする。作者の承認は待たない。

- `ci-required` が緑。
- PR を書いたワーカーとは別に、コーディネーターが起動したサブエージェントがコードレビューし、指摘が残っていない。指摘はワーカーに送って直させ、直したら再レビューする。

マージの仕方：

- squash でマージする。コミットの題は GitHub が付ける既定のまま変えない。題の末尾には `(#N)` が付く（題の本体は PR の題。PR のコミットが 1 つなら、そのコミットの題）。リリースのときに、この `(#N)` と GitHub の記録を突き合わせる。
- 取り込みのコミットを含む PR は例外で、マージコミットでマージする（「外部から取り込むコード」）。題は既定の `Merge pull request #N from ...` のまま。
- `develop` と衝突したら、ワーカーが feature ブランチを `develop` に rebase して push し直す。
- `needs review` の Issue の判断は、マージを止めない。既定で実装してマージし、作者の判断が既定と違えば、作り直しの PR を積む。
- 開発ルールやゲートを変える PR も、同じ条件でマージする。3 文書をエージェントが変更しない規則はそのまま。

`release` へは早送りだけで進め、`main` へは作者がマージコミットでマージする（次の「リリースと作者の確認」）。

マージ後、コーディネーターはワーカーを解放し（orca の release）、ワークツリーを削除し、リモートのブランチを消す。

マージ後、コーディネーターは PR の `closingIssuesReferences`（`gh pr view <番号> --json closingIssuesReferences`）の Issue が閉じたかを確かめ、閉じていなければ `PR #<番号> で develop に入った` とコメントして閉じる。コマンドは [.claude/skills/ebykn-pr/SKILL.md](.claude/skills/ebykn-pr/SKILL.md) の 6。

## リリースと作者の確認

コーディネーターとワーカーは手を止めずに作ってマージし続け、作者はあとからまとめて確かめる。手順は [.claude/skills/ebykn-release/SKILL.md](.claude/skills/ebykn-release/SKILL.md)。

- リリースは基本 1 日 1 回切る。作者が見つけた問題を直したあとなど、1 日に何度切ってもよい。識別子は `YYYY-MM-DD.N`（日本時間の日付と、その日の何回目か）。
- 切るときは、`develop` の先頭のコミットを決めて CI が緑であることを確かめ、前回のタグからそこまでに `develop` に入った PR と、それが閉じた Issue を集め、報告スライドの PDF を作る（`ebykn-report-slides` スキル）。スライドはリリースごとに 1 つ作り、前のものを上書きしない。
- 取り消せない操作は最後にまとめる。PDF ができてから注釈付きのタグ `report/YYYY-MM-DD.N` を付けて Release（prerelease）に PDF を添付し、そのあとで `release` をそのコミットまで早送りする。`release` の先頭は、いつも報告済みのタグになる。言語のリリースのタグ（`v0.1.0` など）とは名前空間を分ける。
- `release` → `main` の PR がなければ作り、あれば本文の「確認待ちのリリース」の一覧を最新にして、今回のリリースのコメントを足す。本文には、いちばん新しい報告済みのタグのコミットを書く。
- 作者が動作を確かめるための orca のワークスペース `release` を置く。`release` ブランチを取り出したワークツリーで、初めてリリースを切るときにコーディネーターが作り、以後リリースを切るたびに `origin/release` まで早送りする（`git merge --ff-only`。スキルの手順 7）。作業ツリーに変更があれば、作者が試しに書いたものかもしれないので、捨てずに早送りをやめ、作者に報告する。
- 作者には、リリースの識別子、PR、Release、判断待ちの `needs review` の Issue、報告スライドを作る途中で見つかった Issue、`release` のワークスペースの名前とパスを報告する。
- 作者は数日おきにスライドを読み、気になったところをコーディネーターに質問し、`release` のワークスペースでターミナルを開いて動作を確かめる。問題がなければ、リリースの PR を `main` にマージコミットでマージする。報告していない範囲を入れないよう、`gh pr merge <番号> --merge --match-head-commit <本文にあるコミット>` でマージする（GitHub の画面でマージするなら、PR の先頭がそのコミットであることを確かめる）。確認待ちのリリースが複数あれば、まとめて入る。squash や rebase をすると、次のリリースの PR で同じ変更が差分にもう一度出る。
- 作者が問題を伝えたら、コーディネーターは「見つけた問題」のとおり Issue と直す PR を `develop` に積み、もう一度リリースを切る。スライドはその差分になる。
- 作者が確かめなかった日は、`release` は `main` にマージされないまま、次のリリースの分が積み上がる。

## コマンド

| コマンド | 内容 |
| --- | --- |
| `cargo xtask precommit` | `cargo fmt --check` と全ゲート。pre-commit フックが呼ぶ。速く保つ |
| `cargo xtask ci` | `cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、全ゲート。PR の前に通す |
| `cargo xtask gates` | 全ゲート。個別には `check-file-size` / `check-md-links` / `check-md-tables` / `check-book` / `check-gate-tests` / `check-verus-pin` / `check-prelude-exports` |
| `cargo xtask verus-install` | 固定した Verus（ルートの `Cargo.toml` の `[workspace.metadata.verus]`）を `~/.cache/ebykn/verus/<版>/` に入れ、sha256 を確かめ、Verus が要求する toolchain を rustup で入れる |
| `cargo xtask verus-verify` | `ebykn_prelude` と §6.2 の参照モデル（`formal/linearity`）を `--no-cheating` で検証し、Verus なしの cargo でもビルドする。参照モデルは、定理（`xtask/src/verus.rs` の `REQUIRED`）が検証されたことを名前で確かめる |
| `cargo xtask golden-verify` | golden の各ケース（`tests/golden/*/expected/status.txt` のあるもの）に `ebykn verify` をかけて期待と比べ、書き出した `build/verus/` を Verus なしの cargo でもビルドする |
| `cargo xtask verus-probes` | 設計ノート §7.2 の前提を確かめるプローブ（`tests/verus-probes/`）を走らせ、各ファイルの先頭の期待と比べる |
| `cargo xtask formal-oracle` | 参照モデルから判定の表を作り直し、コミットされた表（`formal/oracle/`）と比べる |
| `cargo xtask formal-oracle-write` | 参照モデルから判定の表を作り直し、`formal/oracle/` に書く。モデルを変えたら作り直してコミットする |
| `cargo test --workspace` | テスト |
| `mdbook build book` | Ebykn Book を `book/book/` に作る（mdBook 0.5.4） |

- pre-commit フックは `git config core.hooksPath .githooks` で有効にする。Claude Code では SessionStart フック（`.claude/hooks/session-start.sh`）が未設定なら設定する。
- toolchain は `rust-toolchain.toml`（1.96.0）。Verus は自分の toolchain（固定値の `toolchain`）で動き、`verus-*` のコマンドがそれを指定する。Verus のクレートはルートの Cargo ワークスペースから `exclude` してある。
- Verus の版を上げるときは固定値を書き換え、`verus-install`・`verus-verify`・`verus-probes`・`formal-oracle` を通す。プローブの期待が崩れたら、依拠した前提が変わったということなので `OPEN.md` を見直す。
- CI（`.github/workflows/ci.yml`）の必須チェックは `ci-required` だけ。全ジョブを `needs` に持つ集約ジョブで、ジョブを足したら `needs` にも足す。private リポジトリで Actions の分数を使うので、ジョブはキャッシュを使い、トリガーは `pull_request` と、`main`・`develop` への `push` だけにする。`develop` への `push` は、リリースを切る前に `develop` の先頭が緑であることを確かめるため（PR の CI は、ほかの PR が先にマージされたあとの組み合わせを試していない）。

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
