# Rusk

Ruskは競技プログラミング向けの言語です。ソースを単一のC++20ソースへ変換し、生成したC++をコンパイルして実行・提出します。このサイトは現在のβ08開発版の使い方、仕様、実装上の制限を説明します。

```rusk
in n: Int;
in a(n): Vec<Int>;
let best := 0;
for x in a { best >?= x; }
wt a.sum(), best;
```

入力が`4`と`3 1 4 2`なら、出力は`10 4`です。トップレベルに実行文を書き、`main`関数は定義しません。

## はじめる

1. [導入と実行](getting-started/index.md)で処理系をビルドし、C++を生成する。
2. [チュートリアル](getting-started/tutorial.md)で入力、配列、共有、関数、Optionを試す。
3. [言語リファレンス](reference/index.md)と[標準ライブラリ](library/index.md)で規則とAPIを調べる。

## Ruskの基本

`let b := a;`は同じオブジェクトを共有し、`let b = a;`はコピーします。宣言後の`a = value`は共有中の中身を更新し、`a := value`はその名前の共有先を変更します。整数にもこの規則が適用されます。

関数の引数・戻り値、配列・フィールドへの格納も共有を基本とします。必要な場所でコピーを明示し、`readonly`・`fixed`・`noescape`で変更や共有の持ち出しを制限できます。

## 目的から探す

| 目的 | ページ |
|---|---|
| 型と、コピー・比較・入出力の対応 | [型の一覧](reference/types.md) |
| 演算子、優先順位、短絡と更新 | [演算子](reference/operators.md) |
| 文法を厳密に調べる | [文法一覧](reference/grammar.md) |
| 共有・コピー・更新・評価順 | [基本規則](reference/index.md)、[厳密な規則](reference/semantics.md) |
| 型の定義、タプル、型引数、ファイル分割 | [構造体とinclude](reference/records.md) |
| 関数とクロージャ | [関数](reference/functions.md)、[捕捉の詳細](reference/closures.md) |
| 分岐、ループ、パターン | [制御構文](reference/control-flow.md)、[パターン](reference/patterns.md) |
| 入力、出力、配列、辞書、データ構造 | [標準ライブラリ](library/index.md) |
| 使える機能と制限 | [対応範囲](reference/status.md)、[エラー](reference/errors.md) |
| これからの方向性 | [将来の方針と未決定事項](future.md) |
| 処理系の実装・extern・テスト | [開発ガイド](development/index.md) |

現在の仕様と未実装の設計を分けて記載します。版ごとの経緯や測定時点の結果を調べる場合は[開発記録](development/history.md)を参照してください。
