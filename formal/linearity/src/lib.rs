//! 仕様 §6.2（権限スコープと線形性）の参照モデル。`formal/README.md` を参照。

pub mod check;
pub mod sem;
pub mod sound;
pub mod syntax;

// 以下は検証しない普通の Rust（判定の表を作る）。
pub mod enumerate;
pub mod oracle;
