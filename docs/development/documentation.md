# ドキュメントの編集と公開

利用者向けの仕様と開発ガイドは`docs/`、設計の理由・未実装の方針・候補は`design/`に置きます。MkDocs Materialが`docs/`からサイトを生成し、構成を`mkdocs.yml`で管理します。

## 内容の分け方

| 内容 | 置き場所と書き方 |
|---|---|
| 現在使える構文・API | reference・library。使い方、共有とコピー、制限、失敗を機能別に説明する |
| 現在の実装 | development。役割・責任・不変条件・検証方法を説明する |
| 未実装だが方針がある機能 | futureとdesign。実装済みの説明と区別し、未決定の詳細を明示する |
| 採用を決めていない案 | designの候補。提案例の構文を仕様として扱わない |
| 版ごとの経緯・件数・測定結果 | development/historyから参照する記録。時点と条件を残す |

現在のページを作業日誌にしないでください。「追加した」「移行する予定」ではなく、今どこに定義され、どの規則で動き、何が制限かを書きます。過去の記録は履歴と明示し、古いTODOや件数が現在の状態に見えないようにします。

旧URLの案内ページは移動先へリンクし、現在の仕様を二重に管理しません。機能別ページのファイル名は版名に依存させません。

## 掲載例

単独で動く例には必要な入力と期待する出力を付けます。RuskからC++20へ変換・コンパイルし、その入力で実行して出力を確認します。includeの例では、コピーしたプログラムを置くディレクトリと、必要なファイルの配置も説明します。

構文断片、標準宣言からの抜粋、意図的なエラーは、そのことが分かる説明を添えます。共有・コピー・評価順が重要なAPIでは、値だけでなく別名への影響も示します。将来の記法の例は現行の実行例に混ぜません。

`tests/documentation.rs`と`tests/extern_generics.rs`は一部の掲載例を直接読み取ります。該当ブロックの追加・削除・並べ替えでは、対応と期待出力も更新してください。テストの対象は[検証範囲](verification.md)に記載します。strictビルドはRuskコードを実行しません。

## 環境とプレビュー

```bash
python3 -m venv .venv-docs
.venv-docs/bin/python -m pip install -r requirements-docs.txt
.venv-docs/bin/python -m mkdocs serve
```

表示されたローカルURLを開きます。ページ追加時は`mkdocs.yml`のnavと関連ページへの相対リンクを更新します。履歴と互換案内ページは`not_in_nav`で主ナビゲーションの対象から外し、入口からたどれるようにします。`search.exclude`も指定し、古い説明を現在の仕様として検索させないようにします。

```bash
.venv-docs/bin/python -m mkdocs build --strict
cargo test --locked --test documentation --test extern_generics
```

生成先の`site/`と仮想環境はGitに含めません。仕様を変える場合は本文・文法・対応状況・例・設計方針を横断して更新し、実装済みになった項目を将来の未実装一覧に残さないようにします。

## 公開

`.github/workflows/docs.yml`がmainへの文書関連のpushを受けてstrictビルドし、GitHub Pagesへ公開します。対象は`docs/**`、`mkdocs.yml`、`requirements-docs.txt`、公開ワークフロー自身です。プルリクエストではビルド検査だけを行います。手動実行による再公開も可能です。

GitHub PagesのSourceはGitHub Actionsに設定します。公開先は[Rusk](https://shogo314.github.io/rusk_b_08/)です。文書の編集・ローカルビルドと、mainへのpushによる公開は別の操作です。
