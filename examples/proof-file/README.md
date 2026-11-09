# proof-file：証明ファイルと剥がし一致

実装と契約を `src/lib.eyk` に、証明を `proof/lib.eykp` に分けて書いた例です。`ebykn check` が、証明ファイルから証明を取り除く（剥がす）と `src/lib.eyk` に一致することを確かめます（仕様 §6.1 の剥がし一致検査）。`ebykn verify` は、証明ファイルのあるモジュールを証明ファイルから書き下して検証します（§7.5）。

## 中身

- [`src/lib.eyk`](src/lib.eyk)：仕様の関数 `double`、本体のない補題 `double_monotonic`、契約を持つ関数 `twice`。`.eyk` には証明を書けません。
- [`proof/lib.eykp`](proof/lib.eykp)：`src/lib.eyk` の写しに、証明を足したもの。足したのは、補題の本体（`{ assert a + a <= b + b; }`）と、`twice` の本体の `assert x + x <= 2000;` です。

`proof/<p>.eykp` は `src/<p>.eyk` の証明です。剥がすときは、証明の文（`assert`、`proof { }` など）、補題の本体、証明の属性などを取り除き、コメントの違いと、末尾のカンマの違い（契約の後の本体の `{` の前、波括弧・角括弧の閉じの前）は無視して比べます（丸括弧の中のカンマは、`(a,)` と `(a)` が別のものなので、書いたとおりに比べます）。空白は、トークンの区切りを変えない範囲で無視します（`a && b` と `a & &b` は別のものとして比べます）。属性の順序は、組み込みの属性だけの並びのときに無視します。

## 試せる機能

- `ebykn check`：剥がし一致検査。一致しなければ **E0304** で、最初に食い違った位置を両方のファイルで示します。ループの句の削除や並べ替えは **E0305** です。
- `ebykn verify`：本体のある補題は、Verus の結果で `verified` になります。証明ファイルがなければ、本体のない補題は `admit`（証明なしの前提）です。

## 打つコマンドと期待する出力

```console
$ ebykn check
ebykn: check passed (lowered to `$PWD/build/verus`; the self-contained check §6.6 comes with `.eykp` in M3)
$ ebykn verify
verified proof_file::double
verified proof_file::double_monotonic
verified proof_file::twice
results 2 verified, 0 errors
```

証明ファイルで契約を変えると、剥がしても一致しません。`twice` の `requires x <= 1000` を `proof/lib.eykp` でだけ `2000` に変えてみます。

```console
$ sed -i 's/x <= 1000$/x <= 2000/' proof/lib.eykp
$ ebykn check
proof/lib.eykp:12:19: E0304: stripping the proofs from this file does not give its source in `fn twice`: here it has `2000`, but `src/lib.eyk:9:19` has `1000`; the proof file may change only proofs, not the code or the contracts (run `ebykn sync` to bring changes of `src/` into the proof file)
ebykn: check failed with 1 error(s)
$ ebykn --lang ja check
proof/lib.eykp:12:19: E0304: このファイルから証明を剥がしても、ソースと一致しません。`fn twice` のここは `2000`、`src/lib.eyk:9:19` は `1000` です。証明ファイルで変えてよいのは証明だけで、コードと契約は変えられません（`src/` の変更を証明ファイルに取り込むには `ebykn sync` を実行します）
ebykn: 検査が 1 件のエラーで失敗しました
```

証明ファイルをどけると、`src/lib.eyk` から書き下すので、補題 `double_monotonic` は証明なしの前提（`admit`）になります。

```console
$ mv proof/lib.eykp lib.eykp.off
$ ebykn verify
verified proof_file::double
admit proof_file::double_monotonic
verified proof_file::twice
results 2 verified, 0 errors
ebykn: 1 item(s) are assumed without proof (admit: lemma without a body; unchecked; external: #[test], not seen by Verus; macro: items made by a derive or attribute macro of a dependency, not verified by Verus): proof_file::double_monotonic (admit)
```

## 試してみること

- `proof/lib.eykp` の `twice` に文を足す（`let y = x;` など）と、足した文の位置が E0304 で示される。
- `twice` の中にループを書き、`src/lib.eyk` と `proof/lib.eykp` の両方に同じ `invariant` を書く。`proof/lib.eykp` にだけ `invariant` を足すのは通り、`src/lib.eyk` にある句を消すと E0305 になる。
- `proof/lib.eykp` に、`src/lib.eyk` にない補題（本体付きの `lemma fn`）を足す。証明ファイルだけの補題は剥がすので通る。
