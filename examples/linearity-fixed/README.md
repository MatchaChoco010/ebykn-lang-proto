# linearity-fixed：権限の誤りを直した版

[`linearity-broken`](../linearity-broken/) の 2 つの誤りを直した版です。権限の検査を通り、Verus の検証も通ることを確かめられます。

## 中身

[`src/lib.eyk`](src/lib.eyk) の 2 つの関数は、どちらも確保した節点をちょうど 1 回だけ解放します。

- `read_then_free`：値を `r` に読んでから `p.free()` で解放し、`r` を返す。
- `free_once`：`p.free()` は 1 回だけ。解放で受け取った値 `n` から読む。

`(*p).val` は、権限 `p@` を持っている間だけ、権限付きの読み出しに書き下されます。`p.free()` は `Ptr::new` の権限を消費します。どちらも `build/verus/src/lib.rs` で確かめられます。

## 試せる機能

- §6.2 の権限の検査を通った後の、書き下しと検証。
- `ensures r == v` が、権限付きの読み出しと解放を通して示されること。

## 打つコマンドと期待する出力

```console
$ ebykn verify
verified linearity-fixed::read_then_free
verified linearity-fixed::free_once
results 2 verified, 0 errors
```

## 試してみること

- `let r = (*p).val;` を `let n = p.free();` の後ろに動かし、解放した後に読むとどうなるかを見る。
