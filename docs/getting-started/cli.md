# コマンドライン

以下の`rusk`は、ビルドした`./target/release/rusk`を表します。`compile`・`dependencies`は入力ファイルを一つ受け取ります。`compile`のオプションはファイル名の前後に指定できます。

```text
rusk compile [OPTIONS] FILE
rusk dependencies FILE
rusk --help
rusk compile --help
rusk dependencies --help
rusk --version
```

## C++を生成する

```bash
rusk compile main.rusk -o main.cpp
```

| オプション | 意味 |
|---|---|
| `-o PATH` / `--output PATH` | 出力先。省略時はカレントディレクトリの`a.cpp` |
| `--emit STAGE` | 出力する段階。省略時は`cpp` |
| `--no-opt` | 標準で有効な最適化を無効化して比較する |
| `--help` / `-h` | 指定したコマンドの使い方を表示する |

`compile`はC++コンパイラを起動しません。実行ファイルの作成は別途`g++`などで行います。構文・型検査などに失敗すると診断を標準エラーへ表示し、正常終了しません。以前生成したファイルが残る場合があるため、失敗後に古い生成物を実行しないようにしてください。

## ヘルプ・引数・終了コード

`rusk --help`はコマンドの一覧、`rusk compile --help`・`rusk dependencies --help`は各コマンドの引数とオプションを表示します。`rusk help compile`も使えます。`rusk --version`は処理系のパッケージバージョンを表示します。

`--emit=hir`・`--output=main.hir`のように値を`=`で指定できます。同じ出力先・出力段階を複数回指定した場合は最後の値を使い、`--no-opt`の重複指定も許可します。

パスに空白がある場合はシェルで引用符を付けます。`-`で始まるファイル名は、オプションを終える`--`の後に置けます。

```bash
rusk compile --emit hir --output "generated file.hir" "main program.rusk"
rusk compile -o main.cpp -- -source.rusk
```

| 終了コード | 意味 |
|---|---|
| `0` | 変換・依存一覧の取得、またはヘルプ・バージョン表示の成功 |
| `1` | ソースの構文・型検査、ファイルの読み書きなどの失敗 |
| `2` | 引数不足、不明なコマンド・オプション、不正な`--emit`の値など、コマンドラインの指定ミス |

ヘルプ・バージョンは標準出力、引数エラーと処理中の診断は標準エラーに表示します。引数なしの実行は、標準エラーにヘルプを表示して終了コード2を返します。引数エラー時はソースを読み込まず、生成ファイルを作りません。

## 中間表現を調べる

```bash
rusk compile main.rusk --emit hir -o /tmp/main.hir
rusk compile main.rusk --emit lir -o /tmp/main.lir
```

| 段階 | 主な内容 |
|---|---|
| `cst` | 入力ソースの構文構造 |
| `ast` | ASTの構造。演算子の優先順位などを反映 |
| `hir` | 型や参照先を解決した中間表現 |
| `lir` | 基本ブロックと分岐を持つ中間表現 |
| `cpp` | 実行用のC++ソース |

`--emit`を変えても既定の出力ファイル名は`a.cpp`です。CST・ASTの出力は調査用であり、型検査が成功したことの代わりにはなりません。各段階の役割は[変換パイプライン](../development/pipeline.md)に説明しています。

## includeの依存先を調べる

```bash
rusk dependencies main.rusk
```

読み込み対象のRuskソースと、`extern symbol`の`code_file`・`code_files`で参照するC++ファイルのパスを出力します。依存ファイルの確認に使うコマンドで、プログラム全体の型検査や実行は行いません。ファイル分割の書き方は[include](../reference/records.md)を参照してください。
