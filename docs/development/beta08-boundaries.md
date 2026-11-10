---
search:
  exclude: true
---

# β08の移行境界と残す基盤の監査

<!-- rusk-docs: history -->

!!! note "過去の開発記録"
    このページの計画・件数・パス・測定値は記録当時のものです。現在の構成は[開発ガイド](index.md)、対応範囲は[言語リファレンス](../reference/status.md)を参照してください。

[開始時点の棚卸し](beta08-inventory.md)の34項目を、現在のコードの入口・通常の宣言への移行先・残す処理に対応付ける。これは移行境界の記録であり、β08全体の完了判断には[作業計画](beta08.md)の検証結果・[全体性能比較](beta08-performance.md)と文書の整合確認も必要である。検証欄は各境界を調べるテストの所在を示す。

## 型・名前解決・構文

| ID | 現在の入口・移行先 | 残す処理と理由 | 検証 |
|---|---|---|---|
| N01 | `compiler/externs.rs`・`generics.rs`の通常の型・関数登録 | 署名・型パラメータ・候補の照合と具体化は、任意の宣言に共通の型検査 | `extern_generics`、`generics` |
| N02 | Vecと反復型・32操作は`lib/vec.rusk` | 型名による専用登録と要素型の列挙・広域置換は削除。配列構文の出力型はN07で区別 | `library_migration`、`extern_generics`、`beta06` |
| N03 | `Compiler::call/resolve_callable`は通常の候補解決 | 修飾enum構築、Tupleの自動操作、入力の導出要求は言語上の構造を扱う。container・collection・higher_orderの専用呼び出し経路は削除 | `enums`、`library_migration`、`higher_order` |
| N04 | `generics.rs`の署名・期待型からの引数ヒント | `__read__`・`__vec_new__`・`__vec_repeat__`は構文が結果型を指定する呼び出し規約。Some・Dict/Setの専用ヒントは削除 | `generics`、`extern_generics`、`library_migration` |
| N05 | 型系列の共通重複検査、通常の関数候補 | `__Rusk`は生成名の保護、Tupleはタプル構文の型構築子として予約。Vec等の型名・Some等の関数名の予約一覧は削除 | `compiler::generics::namespace_tests`、`library_migration` |
| N06 | `generics.rs`の照合・AST代入・具体化キャッシュと上限 | 任意のジェネリック宣言に必要。未宣言Iterを認める名前の特例は削除。ASTの識別子代入はC++本文の一律置換とは異なる | `generics`、`enums`、`compiler::externs::tests` |
| N07 | 演算子・配列・範囲・for・代入・短絡から通常の操作を解決 | 一回の評価、短絡、初期化、制御フロー、解放順序は構文の意味。配列リテラルはVecの型文脈と構築・追加の規約を使うが、本体は通常の宣言。`??`・`?!`は通常の操作、bare Noneは宣言のimplicit_callへ移行 | `bindings`、`tuples`、`blocks_labels`、`beta05`、`library_migration` |
| N08 | `ast.rs`のリテラルと`patterns/coverage.rs`の被覆検査 | Int・Bool・Char・Str・Unitのリテラル、Bool条件と数値・文字列のパターンは言語の意味。実際のC++表現・共有分類は宣言とIRの情報を使う | `e2e`、`enums`、`pipeline` |

配列の要素型を求める`vector_element`と`Vec<T>`の構築は配列構文の既定の型を表す。VecのC++型・要素コピー・反復・検索等の本体をコンパイラへ戻すための例外ではない。範囲・for・添字も操作名の規約を使うが、標準ライブラリ以外の型の実装は通常の署名から選ぶ。Tupleのリテラル・フィールド番号・構築・射影は積型の言語機能として区別する。

## ライブラリ本体と構造・捕捉の基盤

| ID | 現在の入口・移行先 | 残す処理と理由 | 検証 |
|---|---|---|---|
| L01 | map/filter、状態・キャッシュ・コピー・代入は`lib/lazy_iterators.rusk`とそのC++資源 | 型と本体の専用生成は削除。効果・保持は通常の参照と宣言情報に接続 | `library_migration`、`higher_order`、`callables` |
| L02 | all/any/fold/scan/to_vecは`lib/algorithms.rusk` | 通常の反復・コピー・invoke参照を使い、アルゴリズムの専用生成は削除 | `library_migration`、`higher_order`、`extern_generics` |
| L03 | first_true・sort_by_key・sort_by_compは`lib/algorithms.rusk` | 通常のコピー・比較・invokeを解決。型名によるキー制限と専用本体生成は削除 | `library_migration`、`higher_order` |
| L04 | `externs.rs`の型付きcall/invoke参照と通常の署名照合 | 捕捉環境を含む静的な型一致の検査は共通基盤。反復要素の型名からの推測は削除 | `extern_callbacks`、`extern_generics`、別名のFeed/Cursorの移行例 |
| L05 | `generic_instances`と構造型の自動操作登録 | 専用のcontainerキャッシュは削除。`native_functions.rs`は構造型の薄い導出宣言を登録し、ライブラリ本体は生成しない | `generics`、`records`、`enums` |
| L06 | Dict/Setの型・キー条件・コピー・代入は`lib/hash_collections.rusk` | コンパイラの型特殊化・本体生成は削除。物理表現と効果の検証はA01・A02で区別 | `beta05`、`library_migration`、`data_structures` |
| L07 | ハッシュ反復型・操作・guardは通常のexternとC++資源 | 所有元・世代・完了状態・キー出力の専用生成は削除 | `library_migration`、`beta05`、`higher_order` |
| L08 | Dict/Setの26宣言と既定値補助は通常のライブラリ | 操作名で分岐する本体生成は削除。共有・キー保護・コピー順はライブラリの実装として維持 | `library_migration`、`beta05` |
| L09 | ハッシュ・等値は型に渡す値返却の関数参照と論理型の本体分岐 | キーの適用条件も宣言で検査。C++物理型でのTuple等値と再帰キー本体生成は削除 | `library_migration`、`extern_generics` |
| L10 | Some/None/Option/Dict/Set/try_readと既定値の選択は通常の宣言 | 型名からの構築本体生成と`$fn`の置換は削除 | `library_migration`、`beta05`、`enums` |
| L11 | Tuple・多次元入力の本体は標準のderiveテンプレート | 入力構文が要求する結果型と次元数から導出を要求する処理は残す。利用者の通常のreaderを先に解決し、入力順や本体は宣言で実装 | `library_migration`、`tuples`、`buffered_io` |
| L12 | Optionの操作は通常のextern、短絡はN07の共通規約 | Optionという名前・variant 0・payload 0による検査と自動登録は削除。任意のenumの意味情報は型検査する | `compiler::enum_operations::tests`、`library_migration` |
| L13 | copy・assignの本体は`lib/collections.rusk`のrecord/enum役割 | 言語上の自動操作の存在・要求時導出と、署名・レイアウト・共通ABIのアダプタだけ残す。フィールド・payloadの順序、コピー・代入本体は標準テンプレート | `records`、`enums`、`library_migration`、`heap_aliases` |
| L14 | Tuple出力・6比較は標準の役割とフィールド操作参照 | 自動操作を選ぶ優先順位と論理Tupleの構造検査は残す。出力・比較のC++アルゴリズムと基本型名の列挙は削除 | `tuples`、`library_migration`、`minmax` |
| L15 | record・Tuple・enumの構築、射影、pattern照合 | 任意の利用者の積型・和型を表現する言語・IR基盤。コピーや比較の本体は含めない | `records`、`tuples`、`enums`、`pipeline` |
| L16 | lambda、関数値、非公開の捕捉環境とinvoke ABI | 関数値の型と捕捉は言語機能。externへの参照は共通の型付き参照を使い、関数名の特例を追加しない | `callables`、`extern_callbacks`、`noescape`、`readonly` |

## 型情報・解析・生成とABI

| ID | 現在の入口・移行先 | 残す処理と理由 | 検証 |
|---|---|---|---|
| A01 | `compiler/ir_types.rs`は構造型と宣言のrepresentation/retainsを参照 | 任意の型の物理表現・共有先をIRへ渡す基盤。Vec/Dict/Set/Mapped等の名前で分類しない | `compiler::type_metadata::tests`、`pipeline`、`effects` |
| A02 | `native_effects`の型付き契約・別名・子の効果と保存先の合成 | 信頼契約の署名と射影の検証、固定点、未知と循環の保守的な扱いは安全性の基盤。C++本文を見て純粋と仮定しない | `compiler::effects::mapped_tests`、`native_effects`の単体テスト、`readonly`、`noescape` |
| A03 | `Source`と宣言・具体化のOrigin | 由来の検証は信頼境界。表示ファイル名、利用者の属性、C++資源の内容で標準の権限を得られない | `compiler::effects::tests`、`includes`、`extern_generics` |
| A04 | 効果・readonlyの値分類はIRの表現・共有情報を参照 | 非集約値・不明な共有先を分類する共通の解析。基本型名の列挙は削除 | `compiler::type_metadata::tests`、`readonly`、`noescape`、`effects` |
| A05 | 直接生成は標準のlower系情報と検査済みのenum操作 | 宣言情報を使う共通の生成、境界・overflow・診断位置・所有付き/借用の選択は残す。backendの関数名別の本体表は削除 | `optimization`、`effect_optimization`、`compiler::effects::tests` |
| A06 | スカラー保持は表現情報、代入はoperationの意味情報 | 直接保持・共有先の変更を証明する解析と共通ABIは残す。`__assign__`という綴りだけでは判定しない | `compiler::type_metadata::tests`、`optimization`、`heap_aliases` |
| A07 | 効果・所有元・private領域・寿命・区間のIR解析 | 任意のRuskプログラムの安全性と保持の証明。一般のnative内部のCFG・ヒープを解析する仕組みには拡張しない | `readonly_routes`、`noescape_routes`、`bindings`、`effect_optimization` |
| A08 | HIR/LIRの検証、制御フローと解放 | Bool条件、Unit返却、評価順序、分岐・ループ・解放位置の整合性は言語・IRの意味 | `pipeline`、`blocks_labels`、HIR/LIR各モジュールの単体テスト |
| A09 | 外部シンボル・型・関数の依存、前方宣言、共有/Unit ABI | 任意の宣言を生成する順序・重複抑止と呼び出し規約。コンテナ・遅延型ごとの連結処理は削除 | `includes`、`extern_generics`、`extern_callbacks`、`records` |
| A10 | `lib/prelude.rusk`・`runtime.rusk`と共通Sourceローダー | 標準Sourceの導入と実行ファイルの資源カタログは梱包・信頼の入口。IOマーカー置換・ヘッダー追記と専用のソース組立ては削除 | `includes`、`buffered_io`、`documentation` |

## 通常のexternとして残る解析の限界

通常のRusk本体からの推論、標準由来の検査済み契約、契約のない不透明な本体を区別する。全ての型付きcall/invoke参照は共通の署名解決と依存登録を使う。契約がないexternや、子の効果が未知・循環している宣言はOpaqueとして扱う。標準のC++実装であっても、関数名や本文の見た目で安全な効果を補わない。

遅延copy・状態代入には、子の結果が返却・保存される先を対応付けて所有元の共有を伝える。一方、内部の反復状態・一時値をC++だけで管理するアルゴリズム、集約の代入やハッシュ操作等には、不透明のままの操作がある。これはライブラリ本体の専用生成を残すこととは異なり、通常のexternと同じ保守的な解析の制限である。未知の操作を使うreadonly/noescapeの呼び出しを安全と扱わない。

引数効果は引数全体のmay-setであり、native内の書き換えで後続の射影が別の引数を指し得る場合は、変更・構造変更・保持を引数間にも広げる。引数への保存先をRuskの局所的なprivate領域とは証明しない。独立したコピーと共有を区別するための情報は接続するが、C++本文を解析する一般のデータフロー・ヒープ解析を追加したとはしない。

既存の仕様から広げない制限として、完全な双方向型推論、再帰する構造型、任意のC++依存の推測は提供しない。具体化・構造展開・依存・別名の作業量は上限を持ち、上限を超えた場合はRuskの位置で診断する。詳しい記法・上限・信頼条件は[externの実装規則](extern.md)を参照する。

## 全体の完了監査

この表の分類に加え、fold返却の修正を含む通常595件・最適化無効376件・サニタイザー75例・公開問題51解答と文書の全体検証を確認した。`41322ce`の全テストCIと文書公開も成功した。全性能記録の処理系・入力・出力・回数の対応、試作当時の未完了表記と現在の実装、差分とコミットも照合した。段階ごとの証拠とCIへのリンクは[作業計画](beta08.md)に、測定条件と差・限界は[性能比較](beta08-performance.md)に記録している。
