# 例のプロジェクト

すぐ試せる Ebykn のプロジェクトです。各ディレクトリが `Ebykn.toml` を持つ 1 つのプロジェクトで、`README.md` に、何を示す例か、試せる機能、打つコマンドと期待する出力を書いてあります。

| 例 | 示すこと |
| --- | --- |
| [hello](hello/) | `ebykn init` の形の最小のプロジェクト。`check` と `verify` の基本の流れ |
| [linearity-broken](linearity-broken/) | 権限の誤り（解放し忘れ E0505、二重解放 E0504）を `ebykn check` が見つけて止める |
| [linearity-fixed](linearity-fixed/) | `linearity-broken` を直した版。権限の検査と検証を通る |
| [stack](stack/) | 付録 B の stack。`owns` / `spec` と不変条件。いまは `push` / `pop` が verified にならない |
| [shared-accounts](shared-accounts/) | `Rc<RefCell<T>>` の置き換え。口座を `Ptr` のハンドルで共有し、二つの口座を `borrow mut` の入れ子で同時に借りる。全項目が verified で、`ebykn build` で実行できる |
| [main-program](main-program/) | `src/main.eyk` のプログラムを `ebykn build` で実行ファイルにして実行する。表示は検証しない `unchecked fn` に置く（#54） |
| [proof-file](proof-file/) | 証明を `proof/lib.eykp` に分けて書き、剥がすと `src/lib.eyk` に一致することを `ebykn check` が確かめる（E0304・E0305）。`verify` は証明ファイルから書き下して検証する |

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
- README の ```` ```console ```` のブロックは、CI（`cargo xtask examples-verify`）が実際に実行して、出力を比べます。`$ ` で始まる行がコマンド、続く行がその出力（標準エラーを含む）です。例のディレクトリの絶対パスは `$PWD` と書きます。`$ echo $?` は直前のコマンドの終了コードを出します。コマンドは 1 つずつ新しい `sh` で走るので、`cd`・`export` とヒアドキュメント（`<<`）は使えません。
- 形は `cargo xtask check-examples` で確かめられます（Verus なしで、実行はしない）。決まりの詳細は `xtask/src/gates/examples.rs` の先頭にあります。
