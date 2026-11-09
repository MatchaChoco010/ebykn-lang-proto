//! 報告スライドの道具の固定値。CI の slides ジョブは、このファイルの中身をキャッシュの鍵にする
//! （固定値を変えたときだけ取り直す）。

/// Typst の版。上げるときは `TYPST_SHA256` も書き換え、見本（スキルの `example/`）を組み直して目で確かめる。
pub const TYPST_VERSION: &str = "0.15.1";
pub const TYPST_ASSET: &str = "typst-x86_64-unknown-linux-musl.tar.xz";
pub const TYPST_SHA256: &str = "a6d077d0a95eed5a2eba715b2dae06be954f624ccbf85758a03f389ded33118c";
/// 資産を展開してできるディレクトリ。
pub const TYPST_UNPACKED: &str = "typst-x86_64-unknown-linux-musl";

/// フォントを取る notofonts/noto-cjk のコミット（タグ `Sans2.004`）。
pub const FONTS_COMMIT: &str = "523d033d6cb47f4a80c58a35753646f5c3608a78";
/// 取るフォント（リポジトリの中のパスと sha256）。
pub const FONTS: &[(&str, &str)] = &[
    (
        "Sans/SubsetOTF/JP/NotoSansJP-Regular.otf",
        "dff723ba59d57d136764a04b9b2d03205544f7cd785a711442d6d2d085ac5073",
    ),
    (
        "Sans/SubsetOTF/JP/NotoSansJP-Bold.otf",
        "1b0edfb500b73a4fa8a4fcaae1bbbd403994e08e73e3e0da37e70d3853f42c5f",
    ),
    (
        "Sans/Mono/NotoSansMonoCJKjp-Regular.otf",
        "4d01725be822d144cf9a56ade981e6fb920cd7a610b8fc24cc601a920beea5b9",
    ),
    (
        "Sans/Mono/NotoSansMonoCJKjp-Bold.otf",
        "dfdffe149bc6cbf52860dabd8b8dadbca40ae87a4fbe143b55c0258db2dadfb8",
    ),
];
