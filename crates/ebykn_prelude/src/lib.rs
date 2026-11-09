//! Ebykn の prelude（仕様 §7.1）。書き下しの出力は vstd を直接参照せず、ここの API だけを参照する。
//!
//! 契約は固定した Verus（ルートの `Cargo.toml` の `[workspace.metadata.verus]`）の vstd に合わせてある。
//! §7.1 と違う形にしたものは `OPEN.md` に記録してある。

mod cell;
mod ptr;
mod region;
mod seq;

pub use cell::{Cell, CellId, CellPointsTo};
pub use ptr::{PointsTo, Ptr};
pub use region::{Loc, Region};
pub use seq::SeqExt;
pub use vstd::view::View;
