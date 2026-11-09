# linearity-broken：権限の誤りを見せる例

`Ptr` の権限の規則（仕様 §6.2 の線形性）の誤りを、わざと 2 つ入れた版です。`ebykn check` が、Verus に渡す前に誤りを見つけて止めることを確かめられます。直した版は [`linearity-fixed`](../linearity-fixed/) です。

## 中身

[`src/lib.eyk`](src/lib.eyk) に関数が 2 つあります。どちらも `Ptr::new` で節点を確保し、その権限 `p@` を関数の中で持ちます。

- `read_then_free`：値を読んだあと、解放し忘れる。権限を残したまま関数の終わりに着くので **E0505** です。
- `free_once`：`p.free()` を 2 回呼ぶ。1 回目の解放で権限はなくなるので、2 回目は **E0504** です。

## 試せる機能

- §6.2 の権限の検査（E0503〜E0512）と、その診断。
- `--lang ja` による日本語の診断。

## 打つコマンドと期待する出力

```console
$ ebykn check
src/lib.eyk:8:1: E0505: the permission(s) `p@` are still owned here; free, put back, move or `leak` them (only `gives` return values may remain)
src/lib.eyk:19:13: E0504: cannot free it with `.free()` because the permission `p@` is not held here
ebykn: check failed with 2 error(s)
$ echo $?
1
$ ebykn --lang ja check
src/lib.eyk:8:1: E0505: 権限 `p@` をここでまだ所有しています。解放・`put`・移動・`leak` のどれかをします（残してよいのは `gives` の戻り値だけです）
src/lib.eyk:19:13: E0504: 権限 `p@` をここでは持っていないので、`.free()` で解放できません
ebykn: 検査が 2 件のエラーで失敗しました
```

検査で止まるので、`build/verus/` には何も書き下しません。

## 試してみること

- 診断に従って直し、[`linearity-fixed`](../linearity-fixed/src/lib.eyk) と見比べる。
- ほかの規則を破ってみる。たとえば `let n = p.free();` の前に `let p = 5u64;` と書く（権限を持っている変数を束縛し直す）。どの診断が出るかを見る。
