# 例のプロジェクト

すぐ試せる Ebykn のプロジェクトです。各ディレクトリが `Ebykn.toml` を持つ 1 つのプロジェクトで、`README.md` に、何を示す例か、試せる機能、打つコマンドと期待する出力を書いてあります。

| 例 | 示すこと |
| --- | --- |
| [hello](hello/) | `ebykn init` の形の最小のプロジェクト。`check` と `verify` の基本の流れ |
| [linearity-broken](linearity-broken/) | 権限の誤り（解放し忘れ E0505、二重解放 E0504）を `ebykn check` が見つけて止める |
| [linearity-fixed](linearity-fixed/) | `linearity-broken` を直した版。権限の検査と検証を通る |
| [stack](stack/) | 付録 B の stack。`owns` / `spec` と不変条件。いまは `push` / `pop` が verified にならない |
| [main-program](main-program/) | `src/main.eyk` のプログラムを `ebykn build` で実行ファイルにする。まだ何も出力できない（Issue #54） |

## 試し方

release のワークスペース（作者の確認用）などで、リポジトリの直下から次を実行します。

```sh
cargo xtask verus-install   # 固定した Verus を入れる（初回だけ。入っていれば何もしない）
cargo xtask playground      # ebykn をビルドし、examples/ を playground/ に写す
export PATH="$PWD/target/debug:$PATH"
cd playground/hello
ebykn verify
```

- `playground/` は git が追跡しません（`.gitignore`）。自由に書き換えてかまいません。書き換えても、release のワークスペースの早送りの邪魔になりません。
- `cargo xtask playground` は、すでにあるファイルを上書きしません。もう一度実行すると、新しく増えた例だけを写します。元の形に戻すには、そのファイルかディレクトリを消してから実行し直します。
- `ebykn` は `target/debug/ebykn` にできます。`export PATH=..` は、そのターミナルの間だけ有効です。
- 各例の `build/` は `ebykn` が書き出すものです（各例の `.gitignore` で無視します）。初回の `ebykn check` / `verify` は vstd をビルドするので、1 分ほどかかります。

## 例を足す・直すとき

- 機能が増えたら、それを示す例をここに足し、上の表にも載せます。
- README の ```` ```console ```` のブロックは、CI（`cargo xtask examples-verify`）が実際に実行して、出力を比べます。`$ ` で始まる行がコマンド、続く行がその出力（標準エラーを含む）です。例のディレクトリの絶対パスは `$PWD` と書きます。`$ echo $?` は直前のコマンドの終了コードを出します。
- 形は `cargo xtask check-examples` で確かめられます（Verus なしで、実行はしない）。決まりの詳細は `xtask/src/gates/examples.rs` の先頭にあります。
