# main-program：ebykn build で実行できるプログラム

`src/main.eyk` をクレートルートにしたプロジェクトです。検証したプログラムを `ebykn build` で実行ファイルにし、実行できることを確かめられます。

まだ標準出力に何も出せません（Issue #54）。`println!` も `std::process::exit` も、固定した Verus が「not supported」で拒否するためです。そのため、この例のプログラムは何も出力せず、終了コード 0 で終わります。

## 中身

[`src/main.eyk`](src/main.eyk)：

- `double`：契約を持つ関数（[`hello`](../hello/) と同じもの）。
- `main`：`double(21)` を呼び、`assert!(y == 42)` で結果を確かめる。Verus は `assert!` の条件も証明するので、検証が通れば、実行してもこの `assert!` は外れません。

## 試せる機能

- クレートルートが `src/main.eyk` のプロジェクト（実行ファイルのクレート）。
- `ebykn build`：検査と書き下しのあと、`cargo verus build` で検証して実行ファイルを作る。実行ファイルは `build/verus/target/debug/main-program` にできます。

## 打つコマンドと期待する出力

```console
$ ebykn verify
verified main-program::double
verified main-program::main
results 2 verified, 0 errors
$ ebykn build
$ echo $?
0
$ ./build/verus/target/debug/main-program
$ echo $?
0
```

`ebykn build` は、成功すると何も出しません。

## 試してみること

- `assert!(y == 42)` を `assert!(y == 43)` に変えて `ebykn verify` を実行し、`main` がどうなるかを見る。
- `main` に `println!("{}", y);` を足して、`ebykn build` が何を報告するかを見る（Issue #54）。
