# dogfooding の記録

設計ノート §1.2 の用途を、その時点の `develop` の `ebykn` で実際に書いた記録です（AGENTS.md の「dogfooding」、ロードマップ #1 の D）。見つけたバグと使い勝手の問題は Issue にし、ここには番号と、Issue にしなかったこと（M3 以降の機能待ちなど）を書きます。

## 置き場の決まり

- 1 回ごとに `eval/<YYYY-MM-DD>-<N>/` を作ります。日付は日本時間で、`N` はその日の何回目か（1 から）です。
- 各回の `README.md` に、次を書きます。
  - 対象の `develop` のコミットと、固定した Verus の版
  - 書いたプログラムと、§1.2 のどの用途に当たるか
  - 各プログラムの `ebykn check` / `ebykn verify` / `ebykn build` の結果と、実行の結果
  - 見つけた Issue の番号と優先度
  - Issue にしなかったこと（M3 以降の機能待ち、自分の書き誤り、Verus と同じ制約など）と、その理由
- 書いたプログラムは、それぞれ `Ebykn.toml` と `src/` を持つ Ebykn のプロジェクトにして、その回のディレクトリに置きます。`build/` は追跡しません（各プロジェクトの `.gitignore`）。
- バグの最小の再現は、その回の `repro/<名前>/` にプロジェクトとして置き、Issue の本文にも同じコードを載せます。
- 記録は、その回の時点の結果です。CI は記録を実行し直しません。Issue が直ったかは次の回で確かめ、前の回の記録は書き換えません。
- ルートの `Cargo.toml` の `[workspace]` の `exclude` に `eval` を入れてあります。入れないと、`ebykn` が書き出す `build/verus/` がこのリポジトリのワークスペースの中にあるとみなされ、cargo が止まります（#78）。

## 試し方

```console
$ cargo build -p ebykn
$ cargo xtask verus-install
$ cd eval/2026-11-09-1/rccell
$ ../../../target/debug/ebykn verify
```

`ebykn build` が検証の失敗で止まるプロジェクト（#80）は、`build/verus/` で Verus なしの `cargo build` を打つと実行ファイルを作れます。

## 回

- [2026-11-09-1](2026-11-09-1/README.md)：双方向リスト、親ポインタ付きの木、グラフのアリーナ、`Rc<RefCell<T>>` の置き換え
