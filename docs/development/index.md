# 開発ガイド

処理系はRustとParolで実装し、CST → AST → HIR → LIR → C++20の順で変換します。名前・型・具体化と、共有・効果・契約の検査を行った後にC++を生成します。

## 目的から探す

| 目的 | ページ |
|---|---|
| 言語処理系とライブラリの責任範囲 | [処理系とライブラリの境界](architecture.md) |
| 宣言登録、型検査、具体化、IR、寿命、C++生成 | [変換パイプライン](pipeline.md) |
| 外部型・関数、C++参照、ABI、標準専用の契約 | [externの規則](extern.md) |
| ローカル・CI・公開問題・文書例の確認 | [テストの実行](verification.md) |
| 最適化の条件、測定の手順と限界 | [性能と最適化](performance.md) |
| 文書の分類、掲載例、サイトのビルド・公開 | [ドキュメントの編集と公開](documentation.md) |
| 未実装の方針と検討案 | [将来の方針](../future.md) |
| 以前の計画・検証・測定結果 | [開発記録](history.md) |

## ソースの構成

| 場所 | 役割 |
|---|---|
| `src/main.rs` | コマンドの実行と変換の呼び出し |
| `src/cli/` | clapによる引数・サブコマンド・出力段階の定義 |
| `rusk.par`・`build.rs` | 文法とパーサー生成 |
| `src/frontend/` | 構文解析、AST、ファイル・埋め込み資源の読込 |
| `src/compiler/` | 宣言登録、型検査、ジェネリック具体化、導出、HIRの構築 |
| `src/ir/hir/`・`src/ir/lir/` | IR定義、変換、整合性検査と解放の整理 |
| `src/analysis/` | 束縛、効果、共有元、readonly/noescapeと診断経路 |
| `src/native/` | externのABI、信頼契約、効果の合成、直接生成情報 |
| `src/backend/` | LIRからC++、保持場所・所有・借用・値保持の選択 |
| `src/stdlib/` | 埋め込み標準資源のカタログ |
| `lib/` | 標準のRusk宣言とC++ランタイム |
| `tests/` | 外部の振る舞いを調べる統合テストと共通補助 |
| `examples/` | 実例と公開問題の解答 |
| `tools/` | 検証・ベンチマーク・公開問題接続 |
| `docs/`・`design/` | 現行仕様・開発ガイドと、設計方針・未決定事項 |

## ライブラリの構成

`lib/prelude.rusk`を共通のソースローダーで読み込みます。標準ライブラリも通常の宣言を登録し、利用者の関数と同じ候補解決・具体化を使います。

| ファイル | 内容 |
|---|---|
| `core.rusk` | 基本型、演算、入出力、基本のRusk関数 |
| `collections.rusk` | Option、共通操作、構造展開を使う導出テンプレート |
| `vec.rusk`・`hash_collections.rusk` | 配列、辞書、集合と反復型のジェネリックextern |
| `lazy_iterators.rusk`・`algorithms.rusk` | 遅延反復、探索・集約・ソート |
| `runtime.rusk` | C++資源と依存の宣言 |
| `base.hpp`・`scalar.hpp` | 基本ABIとスカラーの一時値 |
| `io.hpp`・`token_io.hpp` | バッファ入出力とトークン解析 |
| `sequence_runtime.hpp`・`containers.hpp`・`lazy_iterators.hpp` | 共有コンテナ・enum補助・反復状態の実装 |

## 変更を確かめる

変更した規則の実行結果・共有・評価順・診断を検証します。最適化やABIに触れた場合は`--no-opt`との比較、寿命に触れた場合はサニタイザーも使います。文書の変更ではリンクと掲載例を確認します。実行コマンドと対象は[テストの実行](verification.md)を参照してください。

過去の件数や性能値を現在の結果として転記せず、測定・検証したリビジョンと条件を残します。
