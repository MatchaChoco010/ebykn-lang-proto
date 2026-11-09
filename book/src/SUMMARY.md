# Summary

[はじめに](ch00-00-introduction.md)

# 入門

- [入門]()
  - [インストール]()
  - [Hello, Ebykn!]()
  - [`Ebykn.toml` とプロジェクトの構成]()
- [スタックを書いて検証する]()
- [Rust から変わるところ]()
  - [Rust のコードはそのまま書ける]()
  - [仕様の世界と `@`]()

# 契約を書く

- [契約]()
  - [`requires`・`ensures`・`returns`]()
  - [論理式：`forall`・`exists`・`and { }`・`==>`]()
  - [`spec fn` と純粋関数]()
  - [`decreases` と `no_panic`]()
- [権限と `Ptr`]()
  - [`Ptr` と権限 `p@`]()
  - [`*p` で読み書きする]()
  - [`uses` と `gives`]()
  - [線形性の規則]()
- [領域]()
  - [`Region` と `take`・`put`]()
  - [`borrow` ブロック]()
  - [`owns` と `spec`]()
- [`Cell` と内部可変性]()
- [型不変条件と部分集合型]()
  - [`impl Invariant`]()
  - [`impl View`]()
  - [部分集合型]()
- [trait と契約]()

# 証明と前提

- [証明ファイル]()
  - [`.eykp` と剥がし一致]()
  - [`assert`・`proof`・補題]()
  - [`ebykn sync`]()
  - [証明を AI に任せる]()
- [性質ファイル]()
  - [性質ファイルを書く]()
  - [合成と `#[override]`]()
- [`ebykn status`]()

# 実例

- [双方向リスト]()
- [glTF の検証と `FiniteMesh`]()
- [rayon の性質ファイル]()

# 付録

- [付録 A：キーワード]()
- [付録 B：診断コード]()
- [付録 C：Verus への書き下し]()
