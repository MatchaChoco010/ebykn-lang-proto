# main-program：ebykn build で実行できるプログラム

`src/main.eyk` をクレートルートにしたプロジェクトです。`ebykn verify` で検証したプログラムを `ebykn build` で実行ファイルにし、実行できることを確かめられます。

結果の表示は、検証しない `unchecked fn show` に置きました。検証される関数の本体に `println!` を書くと、固定した Verus が「not supported」で拒否するためです（#54）。`show` は Verus が検証しないので、`ebykn verify` は `show` を証明なしの前提として出します。`.eyk` でのマクロの扱いは #53 で判断を待っています。

## 中身

[`src/main.eyk`](src/main.eyk) には関数が 3 つあります。

- `double`：契約を持つ関数（[`hello`](../hello/) と同じもの）。
- `show`：`println!` で結果を表示する `unchecked fn`。検証しない。
- `main`：`double(21)` を呼び、`assert!(y == 42)` で結果を確かめてから `show(y)` を呼ぶ。Verus は `assert!` の条件も証明するので、検証が通れば、実行してもこの `assert!` は外れません。

## 試せる機能

- クレートルートが `src/main.eyk` のプロジェクト（実行ファイルのクレート）。
- `unchecked fn`：検証しない関数。`ebykn verify` は `unchecked` の状態で出し、標準エラーの最後に証明なしの前提として一覧にする。
- `ebykn build`：検査と書き下しのあと、`cargo verus build` で、検証せずに実行ファイルを作る（検証に失敗した関数があってもビルドする）。実行ファイルは `build/verus/target/debug/main-program` にできます。検証は `ebykn verify` で行います。

## 打つコマンドと期待する出力

```console
$ ebykn verify
verified main-program::double
unchecked main-program::show
verified main-program::main
results 2 verified, 0 errors
ebykn: 1 item(s) are assumed without proof (admit: lemma without a body; unchecked; external: #[test], not seen by Verus; macro: items made by a derive or attribute macro of a dependency, not verified by Verus): main-program::show (unchecked)
ebykn: `verified` for an executable function does not include termination: it means the function meets its contract whenever it returns. `decreases` shows only that the loop or recursion it is written on is well-founded; code that reaches a loop or recursion without `decreases` may not terminate
$ ebykn build
ebykn: built `build/verus/target/debug/main-program` without verifying it; run `ebykn verify` to verify (`build --verified` comes in M5)
$ echo $?
0
$ ./build/verus/target/debug/main-program
double(21) = 42
$ echo $?
0
```

`ebykn build` は、成功すると、作った実行ファイルの場所と、検証していないことを標準エラーに出します。

## 試してみること

- `assert!(y == 42)` を `assert!(y == 43)` に変えて `ebykn verify` を実行し、`main` がどうなるかを見る。
- `show` から `unchecked` を外すと、Verus が `println!` を拒否する様子が見られる（#54、#53）。
