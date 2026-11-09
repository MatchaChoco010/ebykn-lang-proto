# shared-accounts：Rc<RefCell<T>> の置き換え

口座を何人もの持ち主で共有し、二つの口座のあいだで残高を移すプログラムです。設計ノート §1.2 の「`Rc<RefCell<T>>` の置き換え」に当たります。dogfooding の 1 回目（`eval/2026-11-09-1/rccell/`）で書いたものを、そのまま例にしました。

Rust で書くなら、口座を `Rc<RefCell<Account>>` で共有し、振替では二つの口座を `borrow_mut()` で同時に借ります。同じ口座を二度渡すと、二度目の `borrow_mut()` が実行時に panic します。この例では、口座のハンドルを `Copy` な `Ptr<Account>` にし、権限は `Bank` の領域 `accts` が持ちます。同じ口座を二度借りないことは、`transfer` の `requires from != to` という静的な義務になり、Verus が呼び出し側で確かめます。

## 中身

[`src/main.eyk`](src/main.eyk) には次のものがあります。

- `Bank`：実行時のフィールド `count` と、権限の領域 `owns { accts: Region<Ptr<Account>> }` を持つ。
- `has` / `bal`：契約で使う `closed spec fn`。`pub` の関数の契約から非公開の消去フィールドは読めないので、これを通す。
- `open`：口座を作って権限を領域に `put` し、ハンドルを返す。返したハンドルが前からある口座と違うことも `ensures` に書く。
- `balance`：`borrow` で権限を借りて残高を読む。
- `transfer`：`borrow mut` を入れ子にして、二つの口座を同時に可変で借りる。
- `demo`：口座を二つ開いて 30 を移し、残高が 70 と 35 になることを `ensures` に書いた、検証される呼び出し側。
- `main`：`demo` の結果を `println!` で表示する `unchecked fn`（検証される関数の本体の `println!` は Verus が拒否する。#54）。

## 試せる機能

- `owns` の領域 `Region<Ptr<T>>` と、`put` / `borrow` / `borrow mut`。
- `borrow mut` の入れ子で、同じ領域から二つの権限を同時に借りる。
- `ebykn verify` で全項目が verified になり、`ebykn build` で実行ファイルを作れる。実行ファイルは `build/verus/target/debug/shared-accounts` にできます。

## 打つコマンドと期待する出力

```console
$ ebykn verify
verified shared-accounts::Bank::has
verified shared-accounts::Bank::bal
verified shared-accounts::Bank::new
verified shared-accounts::Bank::open
verified shared-accounts::Bank::balance
verified shared-accounts::Bank::transfer
verified shared-accounts::demo
unchecked shared-accounts::main
results 5 verified, 0 errors
ebykn: 1 item(s) are assumed without proof (admit: lemma without a body; unchecked; external: #[test], not seen by Verus): shared-accounts::main (unchecked)
$ ebykn build
$ echo $?
0
$ ./build/verus/target/debug/shared-accounts
alice = 70, bob = 35
$ echo $?
0
```

## 試してみること

- `transfer` の `requires` から `from != to,` を消して `ebykn verify` を実行する。内側の `borrow mut` で同じ口座の権限を二度取り出せることを示せず、`transfer` が `failed` になる。
- `open` の `put self@accts[a];` を消して `ebykn check` を実行する。作った権限が残ったまま関数を抜けるので、`E0505` で止まる。
- `demo` の `ensures r.0 == 70` を `r.0 == 71` に変えて `ebykn verify` を実行し、`demo` がどうなるかを見る。
- 口座を `Rc<Cell<Account>>` と `Region<Cell<Account>>` で書くと、今は `&Cell<Account>` で領域を添字にできない（#83）。
