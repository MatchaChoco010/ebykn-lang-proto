# 開発ルール

Ebykn（`.eyk` / `.eykp` を Verus のソースに書き下すトランスパイラ。Rust 製、コマンドは `ebykn`）の段階 1 プロトタイプです。人間とエージェントの両方がこのルールに従います。

要点：

- 正典は `docs/` の 3 文書。仕様が最優先。矛盾と未決事項は `OPEN.md` に記録し、既定に従って進む。
- 作業は orca のワークツリーで行う。main のワークスペースは指示を待つ。1 Issue に 1 PR。
- 人間の判断が要るものは `needs review` の Issue にし、番号を報告する。自分で確定しない。
- PR 本文は日本語で書き、yomiyasu で推敲してから投稿する。ワーカーは PR をマージしない。条件を満たす PR はコーディネーターが squash でマージし、開発ルールや仕様を変える PR などは作者の承認を待つ。
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
- 3 文書はエージェントが変更しない。

## 作業の場所

- main のワークスペースは、ユーザーの指示を受けるために待機させる。作業は orca のワークツリー（作業用ブランチ）で行う。
- main で解決してよいのは、ブランチのマージで生じた問題のように main でしか解決できないものだけ。main に直接コミットしない。
- orca のワークツリー名は `<種類>-<Issue 番号>-<短い名前>`（例：`harness-3-vibe-lang-structure`、`fix-12-linearity-loop`）。ブランチ名は orca が付ける `<GitHub ユーザー名>/<ワークツリー名>` をそのまま使う。
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

## PR

手順（[.claude/skills/ebykn-pr/SKILL.md](.claude/skills/ebykn-pr/SKILL.md) と同じ）：

1. `cargo xtask ci` を通す。
2. PR 本文を日本語で書き、ファイルに保存する。変更点、確認したこと、決めた既定、関連 Issue（`Closes #N`）を書く。
3. yomiyasu スキル（`yomiyasu:yomiyasu`）を呼び、PR 向けの設定（`--doc-type pr`）で推敲する。スキルの手順どおり lint → 書き直し → `run --final ... --doc-type pr` → `verify` まで通し、verify を通した最終稿を変えずに使う。推敲は必須。
4. `gh pr create --base main --head <ブランチ> --title "<日本語の題>" --body-file <最終稿>`。
5. `gh pr checks <番号> --watch` で CI を見届ける。落ちたら直して push し、`ci-required` が緑になるまで繰り返す。
6. ワーカーは PR をマージしない。マージは次の「マージ」に従ってコーディネーターが行う。

### マージ

コーディネーター（main ワークスペース）は、次をすべて満たす PR を squash でマージする。取り込みのコミットを含む PR は例外で、マージコミットでマージする（「外部から取り込むコード」）。

- `ci-required` が緑。
- PR を書いたワーカーとは別に、コーディネーターが起動したサブエージェントがコードレビューし、指摘が残っていない。指摘はワーカーに送って直させ、直したら再レビューする。
- 次の「作者の承認を待つ PR」に当たらない。

作者の承認を待つ PR：

- `docs/ebykn-*.md`（思想・仕様・設計ノート）を変える。
- 開発ルールを変える（`AGENTS.md`、`.claude/`、`.github/ISSUE_TEMPLATE/`、`.github/pull_request_template.md`）。
- ゲートを弱める（`ci-required` から外す、ゲートを消す・緩める、対象外を足す）。
- `needs review` の Issue に関わる。

マージ後、コーディネーターはワーカーを release し、ワークツリーを削除し、リモートのブランチを消す。マイルストーンを閉じる前に、そのマイルストーンでマージした PR の一覧を作者に報告する。

## コマンド

| コマンド | 内容 |
| --- | --- |
| `cargo xtask precommit` | `cargo fmt --check` と全ゲート。pre-commit フックが呼ぶ。速く保つ |
| `cargo xtask ci` | `cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、全ゲート。PR の前に通す |
| `cargo xtask gates` | 全ゲート。個別には `check-file-size` / `check-md-links` / `check-book` / `check-gate-tests` / `check-verus-pin` / `check-prelude-exports` |
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
- CI（`.github/workflows/ci.yml`）の必須チェックは `ci-required` だけ。全ジョブを `needs` に持つ集約ジョブで、ジョブを足したら `needs` にも足す。private リポジトリで Actions の分数を使うので、ジョブはキャッシュを使い、トリガーは `pull_request` と `main` への `push` だけにする。

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

- バグは Issue と、それに紐づく修正 PR にする。
- 使い勝手の悪さや冗長さは `needs review` の Issue にする（テンプレート `usability.md`）。
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
- 取り込みのコミットを含む PR は squash せず、マージコミットでマージする（取り込みだけのコミットを main に残すため）。
- フォークとして手を入れる場合は、取り込みとは別のコミットで、必要な箇所だけにする。変更した箇所には `EBYKN:` で始まるコメントを付ける。上流を取り込み直すときは、取り込みのコミットとの差分を当て直す。
- 自分たちの部分は、できるだけ `vendor/` の外に置く（`verus_syn` なら `crates/ebykn_syntax`）。
