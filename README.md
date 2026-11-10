# Rusk

Ruskは競技プログラミング向けの言語です。Rustで実装した処理系がRuskのソースを単一のC++20ソースへ変換します。作者自身の書きやすさと、競技プログラミングで十分な実行速度を目指しています。このリポジトリはβ08の完成版です。2026年11月11日に現在の実装範囲でfixしました。既知の制限と将来の方針は文書に記載しています。

## 試す

Rust stableと、`__int128`に対応したGCCまたはClangのC++20環境を用意します。

```bash
cargo run --locked -- compile examples/aplusb.rusk -o /tmp/aplusb.cpp
g++ -std=c++20 -O2 /tmp/aplusb.cpp -o /tmp/aplusb
printf '10 32\n' | /tmp/aplusb
```

出力は`42`です。インストールと提出用C++の生成は[導入と実行](docs/getting-started/index.md)を参照してください。

```rusk
in n: Int;
in a(n): Vec<Int>;
let total := 0;
for x in a { total += x; }
wt total;
```

入力が`4`と`3 1 4 2`なら、出力は`10`です。トップレベルに実行文を書きます。

## 現在使えるもの

検査付き64ビット整数、ASCII文字列、配列、タプル、構造体、enumとOption、パターン分岐、ブロック式、ループ、再帰・オーバーロード・型引数付き関数、捕捉付き無名関数が使えます。名前付き関数と型の定義はトップレベルに限ります。

`let b := a;`はオブジェクトを共有し、`let b = a;`はコピーします。既存の変数に対する`=`は中身を更新し、`:=`は共有先を結び直します。整数にも同じ規則を適用します。`readonly`・`fixed`・`noescape`で利用を制限し、推論した共有・効果・脱出の情報に照らして契約を検査できます。

標準ライブラリは辞書・集合、遅延map/filter、集計・探索・ソートを備えます。追加のデータ構造にはUnion-Find、Fenwick Tree、セグメント木、優先度付きキューがあります。標準の操作はRusk関数またはC++へ接続する通常のextern宣言で記述し、型・関数の具体化と参照解決を共通の基盤で扱います。

浮動小数点、広い整数、スライス、内側の名前付き関数、再帰型は未実装です。対応範囲は[機能と制限](docs/reference/status.md)、今後の方向性と未決定事項は[将来の方針](docs/future.md)を参照してください。

## ドキュメント

- [公開サイト](https://shogo314.github.io/rusk_b_08/)／[ドキュメントの入口](docs/index.md)
- [チュートリアル](docs/getting-started/tutorial.md)／[型の一覧](docs/reference/types.md)／[演算子](docs/reference/operators.md)／[言語リファレンス](docs/reference/index.md)／[標準ライブラリ](docs/library/index.md)
- [開発ガイド](docs/development/index.md)／[変換パイプライン](docs/development/pipeline.md)／[externの規則](docs/development/extern.md)
- [設計方針と今後の課題](design/README.md)／[過去の開発記録](docs/development/history.md)

利用者向けのページは現行の処理系を説明します。未実装の方針と候補は設計ノートに分け、版ごとの作業経緯・測定結果は開発記録に保存します。

## 実例と検証

[AtCoderの100問](examples/atcoder/README.md)は保存済み公式サンプル279件と照合します。[AOJの41解答](examples/aoj/README.md)と[Library Checkerの10解答](examples/library_checker/README.md)は公開ケースで検証します。これらはローカル検証で、オンライン提出のAC確認を意味しません。

```bash
python3 tools/test.py --suite local
python3 tools/test.py
```

後者はサニタイザーと公開問題の検証も含みます。依存ツール、対象の絞り方、文書例・examplesの検証範囲は[テストの実行](docs/development/verification.md)を参照してください。

文書サイトはMkDocs Materialで生成します。[編集と公開](docs/development/documentation.md)に準備とstrictビルドの手順があります。
