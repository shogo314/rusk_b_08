# 標準ライブラリ

基本の標準ライブラリは自動で読み込まれます。追加のデータ構造は、各ページに示すファイルを`include`して使います。メソッド専用の定義構文はなく、通常の`a.f(x)`は`f(a, x)`と同じ候補から解決します。

型全体とコピー・比較・入出力の可否は[型の一覧](../reference/types.md)、算術・論理・最小最大などの記号は[演算子](../reference/operators.md)を参照してください。

## APIを探す

| 目的 | ページ |
|---|---|
| トークン・配列・タプル入力、EOF、行入力、出力形式 | [入出力](input-output.md) |
| Vec・Str、添字、連結、コピー、基本のソート・探索、型変換 | [配列と文字列](sequences.md) |
| Dict・Set、find、try_read、不在を返す操作 | [辞書・集合とOption](collections.md) |
| map・filter、集計、短絡探索、キー・比較関数付きソート、PriorityQueue・SegTree | [関数を使う操作とデータ構造](algorithms.md) |
| Union-Find、Fenwick Tree、区間和セグメント木 | [Ruskで記述したデータ構造](data-structures.md) |

## 型から探す

| 型・値の種類 | 主な操作 | 詳細 |
|---|---|---|
| Int・Bool | 演算、比較、真偽変換、明示変換、入出力 | [演算子](../reference/operators.md)・[入出力](input-output.md) |
| Char・Str | 添字、文字の更新、連結、探索、型変換 | [配列と文字列](sequences.md) |
| Vec | 構築、追加・削除、共有取得、コピー、ソート・探索、走査 | [配列](sequences.md)・[関数を使う操作](algorithms.md) |
| タプル | フィールド取得・分解、比較、入出力 | [タプル](../reference/records.md)・[入出力](input-output.md) |
| Option | Some・None、存在判定、共有取得、既定値、match | [Option](../reference/expressions.md) |
| Dict・Set | 登録・検索・削除、キー・値の取得、走査 | [辞書・集合](collections.md) |
| Range・遅延反復 | for、map・filter、to_vec、all・any、fold・scan | [反復型](../reference/types.md)・[走査と集計](algorithms.md) |
| UnionFind・Fenwick・SumSegTree | 併合、点更新、区間集計 | [基本データ構造](data-structures.md) |
| PriorityQueue・SegTree | 優先度付き取り出し、演算を指定する区間集計 | [関数付きデータ構造](algorithms.md) |

型引数と必要な操作の条件は各ページに記載します。構造体・enum・外部型に用意される操作と、利用者が追加する操作は[型の一覧](../reference/types.md#user-types)を参照してください。

## 共有とコピー

引数・戻り値・コンテナへの格納は共有が基本です。`copy`とコピー宣言`let x = value;`は内側までコピーします。`sorted`は外側だけを新しくし、要素は共有します。`fold`は初期値をコピーし、`scan`は初期値と各結果をコピーして保存します。操作ごとの差は各APIに記載します。

型が格納できることと、その型を比較・出力・ハッシュ・コピーできることは別です。`Vec<T>`はUnitや外部型も格納できますが、使う操作に必要な関数は通常の候補解決で見つかる必要があります。構造体の比較・出力は自動生成しません。

## 基本型とフック

| 名前 | 用途 |
|---|---|
| `Int` | 検査付き符号付き64ビット整数 |
| `Bool` | `true` / `false`。標準出力は`Yes` / `No` |
| `Char`・`Str` | ASCIIの文字・文字列 |
| `Unit` | 値を返さない処理の結果。値は`{}`などで作る |
| `copy(x)` | 型に対応する独立したコピー |
| `__assign__(target, source)` | 対象の外側の同一性を保つ更新。`=`からも呼ばれる |
| `__read__`・`__write__` | `in`・`wt`から呼ばれる型別の入出力 |
| `assert(condition)` | 偽なら位置付きの実行時エラー |

算術・比較・真偽変換・添字・反復も通常のフックに接続します。[対応表](../reference/semantics.md)と[extern](../development/extern.md)を参照してください。暗黙の数値変換はありません。条件式での真偽変換は別の規則です。

## 実装の分担

`find`、整数の`min`・`max`・`abs`、最小・最大演算子の既定処理などは通常のRusk関数です。配列・辞書・集合・反復・ソートなどはジェネリックexternとC++ランタイムを使います。extern本体をC++で書くことと、コンパイラが関数名ごとに本体を生成することは区別します。

共有ABI、型・操作の参照、構造の導出、効果情報の検査を共通の基盤で扱います。配置は[開発ガイド](../development/index.md)、コンパイラとの分担は[処理系とライブラリの境界](../development/architecture.md)を参照してください。

未実装の数値型・スライス・走査APIなどは[将来の方針](../future.md)にまとめています。
