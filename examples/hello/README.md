# hello：最小のプロジェクト

`ebykn init hello` で作ったままのプロジェクトです。`ebykn` の基本の流れ（検査 → 書き下し → 検証）を確かめられます。

## 中身

- [`Ebykn.toml`](Ebykn.toml)：パッケージ名と、固定した Verus の版（`[verus]`）。`ebykn check` は、この版が `ebykn` の固定値と同じかを確かめます。
- [`src/lib.eyk`](src/lib.eyk)：契約（`requires` / `ensures`）を持つ関数 `double` が 1 つ。

```eyk
pub fn double(x: u32) -> (r: u32)
    requires x <= 1000,
    ensures r == 2 * x,
{
    2 * x
}
```

## 試せる機能

- `ebykn check`：§6 の検査をし、`build/verus/` に Verus のソースを書き下し、`verus --no-verify` で型を確かめます。
- `ebykn verify`：Verus で検証し、項目ごとの状態（`verified` など）と件数を出します。

## 打つコマンドと期待する出力

```console
$ ebykn check
ebykn: check passed (lowered to `$PWD/build/verus`; the self-contained check §6.6 comes with `.eykp` in M3)
$ ebykn verify
verified hello::double
results 1 verified, 0 errors
```

`$PWD` は、この例のディレクトリの絶対パスです。書き下した Verus のソースは `build/verus/src/lib.rs` にあります。

## 試してみること

- `requires x <= 1000,` の行を消して `ebykn verify` を実行し、`2 * x` の桁あふれを Verus がどう報告するかを見る。
- `ensures r == 2 * x,` を `ensures r == 3 * x,` に変えて、契約を満たさない本体がどう報告されるかを見る。
- `ebykn --lang ja check` で、診断を日本語で出す。
