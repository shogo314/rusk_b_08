# テストの実行

共通の入口は`tools/test.py`です。リポジトリのルートで実行します。選択した段階の成否と所要時間を表示し、一つでも失敗すれば終了コード1を返します。

## 実行対象

| コマンド | 対象 |
|---|---|
| `python3 tools/test.py` | ローカル、サニタイザー、公開問題の全段階 |
| `python3 tools/test.py --suite local` | fmt・Clippy・通常のCargoテスト・最適化無効版の回帰検証 |
| `python3 tools/test.py --suite sanitizers` | ASan・UBSan・LeakSanitizer |
| `python3 tools/test.py --suite public` | AOJ・Library Checkerの公開テスト |

各段階の失敗後も、独立した段階は実行します。ビルドが失敗した場合は、依存する検証を止めて古い実行ファイルの検証を防ぎます。必須ツールがない段階も失敗になります。文書サイトのstrictビルドは[別の検証](documentation.md)です。

## 環境を用意する

Rust stable、Python 3.12以降、Git、GCCのC++20環境を使います。公開問題の検証ツールは次で用意します。

```bash
python3 -m venv .venv-verify
.venv-verify/bin/python -m pip install -r requirements-verify.txt
python3 tools/test.py
```

共通入口は`.venv-verify/bin`を自動でPATHへ加えます。依存関係の指定は`requirements-verify.txt`を参照してください。公開問題は初回の取得・生成にネットワーク接続、時間、ディスク容量が必要です。

C++を並列にコンパイルするため、メモリに余裕がない場合はCargoのテスト並列数を絞れます。対象を減らす指定ではありません。

```bash
RUST_TEST_THREADS=4 python3 tools/test.py --suite local
```

サンドボックスで検証する場合、公開問題のチェッカー生成は`~/.cache/online-judge-tools/`へ書き込む場合があります。LeakSanitizerもptrace下では動きません。エージェントの作業では`AGENTS.md`に従い、必要な段階を承認済みコマンドで制限外から実行します。承認を避ける目的でリーク検査を省略しません。

## 機能を絞って調べる

```bash
cargo test --locked --test tuples
cargo test --locked --test documentation
cargo test --locked --test atcoder abc240_c
RUSK_TEST_NO_OPT=1 cargo test --locked --test documentation
```

統合テストはRusk → C++20 → 実行結果まで検査します。`tests/support/mod.rs`が変換、C++コンパイル、入力、終了コードの共通補助です。`RUSK_TEST_NO_OPT`は対応するテストのRusk変換へ`--no-opt`を加えます。共通入口で再実行する対象一覧は`tools/test.py`にあります。

| 対象 | 主なテスト |
|---|---|
| CLIのヘルプ・引数・終了コード・ファイル名 | `cli` |
| 共有・コピー・型・関数・構造 | `e2e`、`records`、`tuples`、`generics`、`includes`、`enums` |
| 制御・パターン・最小最大 | `blocks_labels`、`beta05`、`beta06`、`minmax` |
| 関数値・走査・データ構造 | `callables`、`higher_order`、`data_structures`、`beta04_structures` |
| extern・標準宣言・ABI・信頼元 | `extern_generics`、`extern_callbacks`、`library_migration` |
| IR・効果・契約・診断経路 | `pipeline`、`effects`、`bindings`、`readonly`、`noescape`、各`*_routes`、`heap_aliases` |
| 最適化の意味と確保数 | `optimization`、`effect_optimization` |
| バッファ境界・EOF・不正入力 | `buffered_io` |
| 文書から直接読む例 | `documentation`、`extern_generics` |

ファイル名に残る版名は既存のテスト対象名です。現在の仕様を版ごとに分ける意味ではありません。内部の不正なIR・効果合成・具体化などは、対応するソースモジュールの単体テストでも検証します。

## サニタイザー

```bash
cargo build --locked
python3 tools/check_sanitizers.py
```

ASan・UBSan・リーク検査で、文字・配列要素・捕捉環境・外部型などの共有と寿命を調べます。対象の正本は`tools/check_sanitizers.py`の`CASES`です。古い開発記録の件数を現在の対象一覧として使わないでください。

## examplesの検証範囲

AtCoderの解答100問は保存済み公式サンプル279件と照合します。`tests/atcoder.rs`は解答ファイル・登録・サンプル一覧の対応も検査します。各解答を一度C++へコンパイルし、サンプル出力を空白区切りのトークンで比較します。

AOJの41解答とLibrary Checkerの10解答は、`examples/`以下の`*.test.rusk`を共通入口が自動列挙します。追加した公開解答も対象になります。

それ以外の実例は、統合テストやサニタイザーから明示的に読み込みます。`examples/`内のすべてのファイルを自動で実行する仕組みではありません。通常の実例を追加する場合は、対応する検証への登録も確認してください。過去の全ソース一括変換比較は一時的な監査であり、恒常的な全件実行を意味しません。

## 公開問題との接続

`.verify-helper/config.toml`と`tools/verify_rusk.py`がRuskをverification-helperへ接続します。解答の先頭に問題URLを記載します。

```text
// verification-helper: PROBLEM https://judge.yosupo.jp/problem/aplusb
in a,b:Int;
wt a+b;
```

処理系をreleaseビルドし、RuskからC++へ変換して`g++ -std=c++20 -O2`でコンパイルします。ケースの取得と判定はonline-judge-toolsに任せ、Library Checkerでは公式のチェッカーも使います。個別に調べる場合は次を使えます。

```bash
export PATH="$PWD/.venv-verify/bin:$PATH"
oj-verify run examples/aoj/alds1_1_b.test.rusk
oj-verify run examples/library_checker/unionfind.test.rusk
```

Rusk・C++資源の依存ファイルに加え、処理系・文法・標準ライブラリ・検証スクリプトも依存として登録します。変更後は再検証され、変更がなければ成功履歴から省略される場合があります。ケースと生成物は`.verify-helper/cache/`、成功履歴は`.verify-helper/timestamps.local.json`にあります。Gitには含めません。

CIの`Test`ワークフローも共通入口を使い、公開ケースをキャッシュします。ローカルの公開ケース検証はオンライン提出・ACの確認とは別です。verification-helperの既定の実行時間制限は1ケース60秒で、各サイトの環境と制限時間を再現するものではありません。

## 文書例

`tests/documentation.rs`は導入・チュートリアル・型の一覧・構造体・演算子・関数・捕捉・配列・アルゴリズム・データ構造の実行例と、fixed・readonly・noescapeの例を文書から直接読み取ります。契約の例では必要な呼び出しを補い、コメントの違反を有効にした拒否・診断の経路も検査します。`tests/extern_generics.rs`はexternの三つの実行例を読み取ります。

すべての文書の構文断片を自動検証するものではありません。掲載例を変更した場合は、実行可能な例・断片・意図的な失敗のどれかを明らかにし、対応するテストも更新します。[文書の編集方針](documentation.md)を参照してください。
