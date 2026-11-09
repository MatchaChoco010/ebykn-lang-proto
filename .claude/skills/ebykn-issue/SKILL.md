---
name: ebykn-issue
description: ebykn-lang-proto で Issue を作る手順。重複を探し、種類・優先度・blocker を付け、bug / needs review / usability のテンプレートで本文を書き、epic の sub-issue にして番号を報告する。バグ、判断待ち、使い勝手の問題を記録するときに使う。
---

# Issue を作る

分類の規則は AGENTS.md の「Issue」と「人間の判断が要るもの」。

## 1. 重複を探す

```bash
gh issue list --state all --search "<語>" --limit 50
```

同じものがあれば、新しく作らずにその Issue の本文を最新の状態に書き直す（経緯はコメントへ）。

## 2. 3 つの軸を決める

- 種類：`bug` / `enhancement` / `documentation` / `epic` / `usability` / `spec` / `harness` / `formal` / `dogfooding`
- 優先度：壊れ方だけで決める。P0 = 黙って誤る、P1 = 落ちる・書けない、P2 = 機能追加。
- `blocker`：他の open な Issue がこれに依存しているときだけ。

人間の判断が要るものには `needs review` を付ける。基準と既定で進められるものは Issue にせず `OPEN.md` に記録する。元の仕様を変えるなら `docs/current-spec.md` に書く。

## 3. 本文を書く

日本語で、本文だけで分かるように書く。テンプレートは `.github/ISSUE_TEMPLATE/` にある。

| テンプレート | 使うとき | 既定のラベル |
| --- | --- | --- |
| `bug.md` | 実装が仕様と食い違う | `bug` |
| `needs-review.md` | 作者の判断が要る | `needs review` |
| `usability.md` | 使ってみて書きにくい・冗長 | `usability`、`needs review` |

テンプレートの見出しを残して本文をファイルに書き、ラベルを付けて作る。

```bash
gh issue create --title "<日本語の題>" --body-file <本文> --label bug --label P1
```

`needs review` の本文には、何を決めるか・なぜ判断が要るか・採ると何が変わるか・各案の利点と欠点・推奨（確定しない）・判断が出るまでの既定を必ず書く。

## 4. epic に付ける

属する epic（ロードマップは #1）があれば sub-issue にする。

```bash
gh api -X POST repos/{owner}/{repo}/issues/<epic 番号>/sub_issues -F sub_issue_id=$(gh api repos/{owner}/{repo}/issues/<番号> --jq .id)
```

epic の本文には現在地と子への索引だけを置く。

## 5. 報告する

作った Issue の番号と題を報告する。`needs review` はコーディネーター（ワーカーなら worker_done）を通してユーザーに伝える。
