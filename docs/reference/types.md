# 型の一覧

このページは、現在のRuskで使える型をまとめたものです。標準ライブラリは自動で読み込まれます。追加のデータ構造には`include`が必要です。

表中の`T`・`K`・`V`などは型引数を表す記号です。標準で使える名前と、[利用者が定義する型](#user-types)を分けて記載します。

## 基本型

| 型 | 値の例・構築 | 範囲と用途 |
|---|---|---|
| `Int` | `42`、`-7`、`Int()`は0 | 符号付き64ビット。−2⁶³〜2⁶³−1。オーバーフロー・ゼロ除算を検査 |
| `Bool` | `true`、`false`、`Bool()`はfalse | 真偽値。標準出力は`Yes`・`No` |
| `Char` | `'a'`、`'\n'`、`Char()`はNUL | ASCIIの1文字。コードは0〜127 |
| `Str` | `"abc"`、`Str()`は空文字列 | 変更可能なASCII文字列。文字への共有・更新が可能 |
| `Unit` | `{}`、値を返さない関数の結果 | 値に意味を持たない処理の結果。標準の`Unit()`はない |

整数リテラルは十進数です。浮動小数点、128ビット整数、任意精度整数、Unicode専用型は現在提供していません。`()`は値の式には使えません。パターンの`()`はUnitに一致します。

```rusk
fn accept(value: Unit) -> Int { return 7; }
let number: Int := 42;
let flag: Bool := true;
let letter: Char := 'A';
let word: Str := "abc";
let nothing: Unit := {};
wt number, flag, letter, word, accept(nothing);
```

出力は`42 Yes A abc 7`です。[数値と演算子](operators.md)、[文字列と型変換](../library/sequences.md)、[入力と出力](../library/input-output.md)も参照してください。

## 配列・タプル・Option・辞書・集合

| 型・型構文 | 作り方の例 | 内容・注意点 |
|---|---|---|
| `Vec<T>` | `[1, 2]`、`let a: Vec<Int> := [];` | 同じ型の要素を共有する可変長配列。`[x; n]`は各要素をコピー |
| `(A, B, ...)` | `(1, "abc")` | 型と個数が固定された2要素以上のタプル。格納は共有 |
| `Option<T>` | `Some(1)`、`let x: Option<Int> := None;` | 値の存在・不在を表す標準enum |
| `Dict<K, V>` | `Dict::<Str, Int>()` | ハッシュ辞書。キーをコピーし、値を共有して格納 |
| `Set<T>` | `Set::<Int>()` | ハッシュ集合。要素をキーとしてコピーして格納 |

型注釈では`Vec<Int>`、関数呼び出しの明示型引数では`Dict::<Str, Int>()`のように書きます。タプルでは位置ごとに異なる型を使えます。配列はすべての要素が同じ型である必要があります。

DictのキーとSetの要素は、Int・Bool・Char・Str、およびそれらを入れ子にしたタプルに限ります。配列・構造体・enumは標準のキーとして使えません。ハッシュ型の走査順は保証しません。

```rusk
let rows: Vec<Vec<Int>> := [[1, 2], [3, 4]];
let pair: (Int, Str) := (5, "five");
let selected: Option<Int> := Some(pair[0]);
let counts := Dict::<Str, Int>();
counts["apple"] += 1;
let seen := Set::<(Int, Int)>();
seen.insert((0, 1));
wt rows[1], pair, selected ?? 0;
wt counts["apple"], seen.contains((0, 1));
```

出力は`3 4 5 five 5`、`1 Yes`です。詳細は[配列](../library/sequences.md)、[タプル](records.md)、[Option](expressions.md)、[Dict・Set](../library/collections.md)を参照してください。

## 操作の対応表

「要素による」は、使う操作に対応する要素型の関数が必要という意味です。型を格納できることと、その型をコピー・比較・入出力できることは別です。

| 型 | コピー | `==`・`!=` | `<`・`<=`・`>`・`>=` |
|---|---|---|---|
| Int・Char・Str | 対応 | 対応 | 対応 |
| Bool | 対応 | 対応 | 単体では未対応 |
| Unit | 対応 | 標準なし | 標準なし |
| Vec | 要素による | 要素による | 要素による辞書順 |
| タプル | 各フィールドによる | 各フィールドによる | 各フィールドによる辞書順 |
| Option | payloadの型による | 標準なし | 標準なし |
| Dict | キー・値の型による | 標準なし | 標準なし |
| Set | キーの型に対応 | 標準なし | 標準なし |

配列・タプル内のBoolの順序はfalseがtrueより先です。Bool単体に`<`を提供する意味ではありません。比較できる配列要素には標準のソートを使え、比較関数・キー関数を指定するソートも使えます。

| 型 | `in`の標準入力 | `wt`の標準出力 | 条件式で真になる場合 |
|---|---|---|---|
| Int | 符号付き十進整数 | 十進整数 | 0以外 |
| Bool | Yes/No・true/false・1/0 | Yes/No | true |
| Char | 空白を飛ばして1文字 | 1文字 | コードが0以外 |
| Str | 空白区切りのトークン | 文字列 | 空でない |
| Unit | 標準なし | 標準なし | 標準なし |
| Vec | 次元ごとの長さと要素の入力操作が必要 | 要素の出力操作が必要 | 空でない |
| タプル | 各フィールドの引数なし入力操作が必要 | 各フィールドの出力操作が必要 | 標準なし |
| Option | 標準なし | 標準なし | Someである。中身が0やfalseでも真 |
| Dict・Set | 標準なし | 標準なし | 空でない |

条件式は型別の真偽変換を使います。代入・引数・戻り値に一般の暗黙変換を行うものではありません。標準の操作がない型にも、対応する通常の関数を定義して操作を追加できます。

## 範囲と反復型

通常は式の結果から型を推論して使います。範囲とイテレータは配列ではなく、要素を走査するための値です。

| 実在する型 | 作られる場所 | 用途 |
|---|---|---|
| `Range` | `start..end` | 開始を含み終了を含まない、刻み幅1の整数範囲 |
| `RangeIter` | Rangeの`__iter__` | 整数範囲の進行状態 |
| `Iter<Vec<T>>` | 配列の`__iter__` | 元の配列を保持し、要素を共有して取り出す |
| `Iter<Dict<K,V>>` | 辞書の`__iter__` | コピーしたキーと共有する値のタプルを取り出す |
| `Iter<Set<T>>` | 集合の`__iter__` | コピーしたキーを取り出す |
| `Mapped<I,F,T,R>` | `map` | 型Iの反復と型Fの関数を保持し、TをRへ遅延変換する |
| `Filtered<I,F,T>` | `filter` | 型Iの反復と型Fの関数を保持し、条件を満たすTを共有して通す |

| 値の型 | 標準のforの入力 | 標準のコピー |
|---|---|---|
| Range | 対応 | 提供しない。必要なら端点から範囲を作る |
| RangeIter・Iter<Vec<T>> | 直接の入力には未対応 | 進行状態をコピー。配列の所有元は共有 |
| Iter<Dict<K,V>>・Iter<Set<T>> | 対応。現在の位置から走査 | 進行状態をコピー。コンテナの所有元は共有 |
| Mapped・Filtered | 対応。残りを走査 | 元の反復・関数・保持値のコピー操作が必要 |

forは入力に`__iter__`を呼びます。RangeIterとIter<Vec<T>>は、取得済みの状態を再びforへ渡す標準の`__iter__`を提供していません。低水準の`__iter_has_next__`・`__iter_next__`では操作できます。Strも直接のfor走査には対応せず、整数範囲と添字で走査します。これらの範囲・反復型に比較・入出力・真偽変換の標準操作はありません。

`I`は反復型、`F`は関数値の型、`T`は入力要素型、`R`は出力要素型です。`Iter`は上記の配列・辞書・集合に対応する型パターンが宣言されており、任意の`Iter<T>`が自動的に使えるわけではありません。

mapの結果型Rとfilterの要素型TはUnitにできません。mapの入力要素はUnitも扱えます。反復状態をコピーする場合は、その型と捕捉値・保持値に必要なコピー操作を要求します。元のコンテナを保持する反復型のコピーは、その所有元を共有します。

```rusk
fn total(values: Range) -> Int {
    let result := 0;
    for x in values { result += x; }
    return result;
}
let odd := (0..6).filter((x: Int) => x % 2 == 1);
let doubled := odd.map((x: Int) => x * 2);
wt total(1..4), doubled.to_vec();
```

出力は`6 2 6 10`です。Rangeの端点は作成時の値を保持します。遅延反復の別名は進行状態を共有し、再利用すると残りから進みます。型ごとに使える走査操作、消費・コピー・構造変更の規則は[関数を使う操作](../library/algorithms.md)と[捕捉・遅延走査](closures.md)を参照してください。

## 利用者が定義する型と関数値 { #user-types }

`struct`・`enum`・`extern type`で新しい名前付きの型を定義できます。以下は定義方法の分類であり、標準型の名前の一覧とは区別します。

| 種類 | 宣言・利用方法 | 自動で用意する操作 |
|---|---|---|
| 構造体 | `struct`で名前・フィールドを定義 | フィールドに対応するコピー・代入。比較・出力は自動生成しない |
| enum | `enum`でvariantとpayloadを定義 | payloadに対応するコピー・代入。一般の比較・出力・真偽変換は自動生成しない |
| 型引数付き構造体・enum | 定義に`<T>`などを付け、具体的な型で利用 | 具体化したフィールド・payloadに従う |
| 関数値・クロージャ | 具体的な関数名、または`(x: Int) => ...` | 固有の型と呼び出し。捕捉のコピー・代入は捕捉値の操作による |
| 外部型 | `extern type`でC++表現を宣言 | コピー・比較・入出力などは必要に応じて関数を定義 |

例えば、次の`Point`と`Holder<T>`は、このプログラムが宣言する型です。

```rusk
struct Point { x: Int; y: Int; }
struct Holder<T> { value: T; }
enum State<T> { Ready(T), Waiting }
let point := Point { x: 2, y: 3 };
let held := Holder::<Int> { value: point.x };
let state: State<Int> := State::Ready(held.value);
wt match state { Ready(x) => x, Waiting => 0, };
let add_y := (x: Int) => x + point.y;
wt add_y(4);
```

出力は`2`、`7`です。名前付きの型はトップレベルに定義します。関数値の固有の型名を書く構文はなく、変数では推論し、関数・構造体では型引数を通して受け取ります。

[構造体・タプル・型引数](records.md)、[enum](enums.md)、[関数値と捕捉](closures.md)、[extern](../development/extern.md)に詳しい規則があります。

## includeして使うデータ構造

次はリポジトリに付属するRuskのライブラリです。自動では読み込まれません。

| 実在する型 | includeするファイル | 用途 |
|---|---|---|
| `UnionFind` | `examples/beta02/library/union_find.rusk` | 素集合の併合・同一集合の判定 |
| `Fenwick<T>` | `examples/beta02/library/fenwick.rusk` | 点更新・区間和 |
| `SumSegTree<T>` | `examples/beta02/library/sum_segment_tree.rusk` | 加算を使う区間集計 |
| `PriorityQueue<T,F>` | `examples/beta04/library/priority_queue.rusk` | 比較関数Fで優先順位を決めるヒープ |
| `SegTree<T,F>` | `examples/beta04/library/segment_tree.rusk` | 演算関数Fと単位元を指定する区間集計 |

パスはリポジトリ直下に利用プログラムを置く場合の指定です。構築・操作・コピーの条件は[基本データ構造](../library/data-structures.md)と[関数付きデータ構造](../library/algorithms.md)を参照してください。

## 型引数・注釈と制限

型引数の表記は、型の位置では`Vec<Int>`、ジェネリック関数の呼び出しでは`try_read::<Int>()`、構造体構築では上の定義例の`Holder::<Int> { value: 1 }`です。空配列・None・空コンテナなどで型が決まらない場合は、型注釈や明示型引数を付けます。

明示型引数を指定できるのは、型引数を宣言した関数です。Intをコピーする標準の`copy`は具体的な関数なので、`copy(number)`と呼びます。

```rusk
let number: Int := 7;
wt copy(number);
let from_input := try_read::<Int>();
wt from_input ?? 0;
```

入力が`9`なら、出力は`7`、`9`です。

`readonly`・`mutable`・`fixed`・`rebindable`は変数・引数への注釈です。`Vec<readonly Int>`のような型表記や、型引数のトレイト制約は現在提供していません。再帰型、空の構造体、スライス、型消去した共通関数型も未対応です。未実装の方針は[将来の方針](../future.md)を参照してください。
