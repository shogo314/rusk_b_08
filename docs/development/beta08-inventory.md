# β08の専用処理と移行依存関係

調査基準はβ08開始コミット`3a59755`。以下は[作業計画](beta08.md)の段階1の成果で、移行の実装状況ではない。処理系のソースはこの調査では変更していない。

## 分類と調査範囲

同じ実装の各オーバーロードは一行にまとめ、呼び出し入口・型の登録・本体生成・解析・最適化を別々に追った。`src/`の型名・関数名による分岐、`include_str!`、文字列置換、C++生成を調べ、`lib/`の宣言と関連テストを対応させた。テスト用の文字列や診断表示だけの名前は移行対象に数えない。今後、新しい依存が見つかったらこの一覧を更新する。

- **移行**：通常のRusk定義、extern定義、宣言に付随する共通の情報へ移す。
- **基盤候補**：言語の意味や安全性を実装する処理として残す案。ファイル全体の例外ではない。
- **要設計**：移せる操作と、処理系が生成すべき仕組みの境界を先に決める。

依存欄の記号は、後述する共通基盤を指す。検証欄は既存の回帰テストの所在であり、未実装のジェネリックexternまで検証済みという意味ではない。テスト名は`tests/`以下のファイル名から`.rs`を省略した。

## 型・名前解決と呼び出し入口

| ID | 現在の処理と所在 | 分類・移行先／残す理由 | 依存 | 検証 |
|---|---|---|---|---|
| N01 | `compiler.rs::compile`の外部型・関数登録、`rusk.par::ExternType/ExternFn` | 移行：具体型限定の登録を外部ジェネリクスと共通化。通常の型検査は残す | G1、G2 | `generics`、`e2e`＋外部型・関数の新規例 |
| N02 | `compiler.rs::check_type/specialize_vector`のVec型の専用登録と要素制限 | 移行済み：Vec・配列反復型と31操作を通常のジェネリックexternへ移行。型・操作の専用登録、要素型の列挙、広域置換を削除し、コピーも要素の通常の操作を解決する | G1、G2、G4 | `e2e`、`records`、`enums`、`beta06`、`library_migration`、`extern_generics` |
| N03 | `compiler.rs::call`のenum→container→通常解決→collection→higher_orderという専用経路 | 移行：ライブラリ関数は共通の候補解決へ。enumの言語構文との接続だけはN07と分けて残す | G1〜G5 | `generics`、`higher_order`、`beta05`、`enums` |
| N04 | `compiler.rs::return_selected`、`containers.rs::container_argument_hint`のread・Vec構築・Some・Dict操作に固有の型ヒント | 移行：宣言の型パラメータと期待型に基づく推論へ | G1、G2 | `generics`、`beta05`、`enums`＋期待型だけで決まるextern |
| N05 | `compiler.rs::compile`、`enums.rs::register_enum`の標準型・構築関数の予約名 | 要設計：内部生成名の保護は残す。標準型・関数の予約は重複宣言と構文上の必要性を個別に確認 | G1、G5 | `generics`、`records`、`enums`の不正宣言 |
| N06 | `generics.rs`の型引数の照合・AST代入・具体化キャッシュ・再帰上限 | 基盤候補：任意の宣言の具体化。Rusk ASTの識別子代入はC++本体の広域置換とは別で、G1に拡張する | G1、G2 | `generics`、`enums`の型変数とvariant名の衝突 |
| N07 | `compiler.rs::expr/statement/binary_name`、`expressions.rs`、`tuples.rs::assign_tuple_value`の演算子・配列・範囲・for・代入・短絡の展開 | 基盤候補：構文から操作を呼ぶ規約と評価順序は残す。呼び出す標準操作の実装は移行。`coalesce`のOption型・variant 0への直接依存と`?!`のOption限定検査はG5で分離する対象 | G5 | `bindings`、`tuples`、`blocks_labels`、`beta05`、`optimization` |
| N08 | `ast.rs`のリテラル型、`compiler.rs::literal`、`patterns/coverage.rs`のBool・Int・Str等の被覆判定 | 基盤候補：リテラルとパターンの言語仕様。基本型の登録・表現情報との境界を明記する | G4、G5 | `enums`、`pipeline`、`e2e` |

## ライブラリ本体と補助コード

| ID | 現在の処理と所在 | 分類・移行先／残す理由 | 依存 | 検証 |
|---|---|---|---|---|
| L01 | `higher_order.rs::higher_order`のmap/filterと`lazy_cpp/lazy_copy_*`の遅延型・キャッシュ・コピー・変更検出 | 移行：外部ジェネリック型と関数。遅延評価とキャッシュの意味を維持 | G1〜G4 | `higher_order`、`callables`、`beta04_structures` |
| L02 | 同関数のall/any/fold/scan/to_vec | 移行：Ruskまたはexternの通常のアルゴリズム。短絡、初期値のコピー、scanのスナップショットを維持 | G1〜G4 | `higher_order`、`beta04_structures` |
| L03 | 同関数のfirst_true・sort_by_key・sort_by_comp | 移行：二分探索・ソート本体、比較・キー関数の呼び出し、キーの一度だけの評価をライブラリへ | G1〜G4 | `higher_order`、`beta04_structures` |
| L04 | `higher_order.rs::iteration/callback/copy_function`の型名による要素推定、静的関数照合、コピー関数参照 | 移行：宣言から反復要素・呼び出し・コピーを解決。静的な型整合性の検査自体は共通基盤として残す | G2〜G4 | `callables`、`higher_order`＋別名の反復型 |
| L05 | `higher_order.rs::native_function`、`containers.rs::cached_native`の生成関数登録・名前と型によるキャッシュ | 移行：外部ジェネリクスの具体化に統合し、専用レジストリを撤去 | G1、G2 | `generics`＋同じ具体化の複数利用・宣言順序 |
| L06 | `containers.rs::specialize_container/container_cpp`のDict/Set型・キー制限・コピーと代入の登録 | 移行：外部型と通常の操作。ハッシュ・等値・コピーの適用条件を宣言側から解決 | G1、G2、G4 | `beta05`、`enums`、`data_structures` |
| L07 | `containers.rs::register_collection_iterator/container_guards`の所有元・変更世代・完了状態・キー出力 | 移行：Dict/Setの外部反復型・関数。キーを保護し、削除後の共有値と寿命を維持 | G1〜G4 | `beta05`、`higher_order` |
| L08 | `containers.rs::collection_operation`のlen/empty/bool/clear/contains/erase/get/insert/set/添字/keys/values | 移行：通常の関数宣言。添字での既定値挿入、キーのコピー、値の共有を維持 | G1、G2、G4 | `beta05`の共有・キー保護・参照実装との比較 |
| L09 | `containers.rs::key_hash_cpp/prepare_container_copies/container_copy_cpp`のキー演算・再帰コピーと宣言生成 | 移行：操作参照と型に依存したextern定義。単なる型参照だけでは足りない | G2、G4、G6 | `beta05`、`enums`の論理型ごとのcopy |
| L10 | `containers.rs::container_constructor/default_function`のSome/None/Option/Dict/Set/try_read構築と`$fn`置換 | 移行中：Some/None/Option/try_readを通常のジェネリック宣言へ移し、EOF判定と入力の解決をライブラリへ接続。Dict/Set構築と既定値の選択は残る | G1、G2、G5 | `beta05`、`enums`、`buffered_io` |
| L11 | `containers.rs::read_function`の多次元Vec・Tupleの入力本体と`$fn`置換 | 要設計：本体はライブラリへ。任意次元引数・任意長Tupleのフィールド展開を有限個の専用分岐なしで表す方法が必要 | G1、G2、G6 | `beta05`、`tuples`、`buffered_io`＋不正な次元・負の長さ |
| L12 | `enums.rs::specialize_enum/option_element/option_operation`のOption名判定・helper自動登録・is_some/is_none/bool/not/value | 操作を移行済み：構築・検査・値取得を通常の宣言へ移し、自動登録を撤去。構文に残る型・役割判定はN07/G5で整理 | G1、G2、G4、G5 | `enums`のOption互換、`beta05`、`effects` |
| L13 | `records.rs::prepare_record/prepare_record_copies/record_copy_cpp`、`enums.rs`のcopy/assignと再帰コピー生成 | 要設計：レイアウト生成と自動操作を分離。任意のフィールド・payloadのcopy参照、順序、外側の同一性を共通の導出機構で表す案 | G4、G6 | `records`、`enums`、`heap_aliases` |
| L14 | `tuples.rs::automatic_value/prepare_tuple_functions/tuple_cpp`の基本型限定の比較・出力の自動生成 | 要設計：Tupleレイアウトとは別に、フィールド演算から操作を導出する。型の列挙による可否判定も整理 | G4、G6 | `tuples`、`minmax`、`records` |
| L15 | `records.rs::record_value/field`、`tuples.rs::specialize_tuple/tuple_value/tuple_item`、`enums.rs`のvariant構築と`patterns/` | 基盤候補：ユーザー定義の積型・和型、フィールド射影、パターン照合。L12〜L14の標準操作本体を含めて例外にしない | G5、G6 | `records`、`tuples`、`enums`、`pipeline` |
| L16 | `callables.rs`のlambda・関数値・捕捉用構造体・invoke生成 | 基盤候補：静的関数の型と捕捉環境は言語機能。externへの参照・呼び出しはG3として共通化 | G3、G4 | `callables`、`bindings`、`noescape`、`readonly` |

## 型情報・効果・最適化・生成順序

| ID | 現在の処理と所在 | 分類・移行先／残す理由 | 依存 | 検証 |
|---|---|---|---|---|
| A01 | `ir_types.rs::ir_types`のVec/Iter/RangeIter/Dict/Set・基本型の名前による分類 | 移行：登録した型に付随する表現・要素・共有先の情報からIR型を作る。同じC++型でもRuskの型同一性を保つ | G1、G4 | `pipeline`、`effects`、`heap_aliases`＋別名型 |
| A02 | `native_effects.rs::contract`の基本型制限・Vector要素・Option variant形状、`effects.rs::project_type`のStr→Char | 要設計：契約検証と共有先射影は残す。標準型の形状・役割との対応は宣言に接続し、不適合な宣言を拒否する | G4 | `effects`、`effect_optimization`、`enums`＋契約不整合 |
| A03 | `compiler/effects.rs::declared_native_effects`と`compiler/cpp.rs::cpp_context`の埋め込みファイル名による信頼判定 | 移行：読込元の信頼情報を宣言・具体化へ伝える。任意の利用者externは未知として扱う | G1、G4 | `optimization`の不透明extern、`effects`＋契約詐称拒否 |
| A04 | `effects.rs`の戻り値解析、`readonly.rs::aggregate`等のInt/Bool/Char/Unitによる非集約判定 | 移行：型の表現・共有情報に接続。解析手順と保守的な未知の扱いは残す | G4 | `effects`、`readonly`、`noescape`、`heap_aliases` |
| A05 | `backend/intrinsics.rs::lower/lower_element`の名前と型による算術・比較・copy/assign・入出力・bool/len・反復・添字の直接生成 | 移行：宣言に対応する意味情報と共通の生成規則へ。境界検査・overflow・エラー位置・所有付き結果を保つ | G4 | `optimization`、`effect_optimization`、`buffered_io` |
| A06 | `backend/scalar_aliases.rs`の基本型分類と`__assign__`判定、`backend/cpp.rs`の基本型の直接保持 | 移行：表現と操作の意味情報へ。安全な保持・借用を証明するアルゴリズム自体は残す | G4 | `optimization`、`effect_optimization`、`heap_aliases` |
| A07 | `effects/`、`readonly/`、`bindings.rs`、`backend/owned_aliases*`・`storage.rs`の効果・寿命・所有元・区間解析 | 基盤候補：任意のプログラムの安全性と保持方法の証明。型の判定はA01〜A06で分離 | G4 | `bindings`、`readonly_routes`、`noescape_routes`、`effect_optimization` |
| A08 | `hir_validate.rs`・`lir.rs`のリテラル/Bool条件/Unit返却の整合性、`hir_flow.rs`・`lir_cleanup.rs`の制御と解放 | 基盤候補：IRの意味と整合性。基本型名の出現すべてをライブラリ本体として削除しない | G5 | `pipeline`、`blocks_labels`、各モジュールの単体テスト |
| A09 | `compiler/cpp.rs::cpp_context`の型の前方宣言→本体→コピー定義、`backend/program.rs`の宣言・署名・ABI | 要設計：依存順序・重複抑止とABIは残す。container/lazy固有の連結順序を共通化 | G1〜G3、G6 | `generics`、`records`、`enums`、`callables`＋前方参照 |
| A10 | `main.rs::run`のcore/collections読込、IOマーカー置換、scalar/containersヘッダーの追加 | 要設計：標準preludeの読込は残せる。ランタイム断片の組立ては共通の依存登録に寄せ、Vecの型置換と混同しない | G2 | `includes`、`buffered_io`、`documentation` |

`lib/core.rusk`の通常のRusk関数、具体的なextern、`lib/collections.rusk`のOption enum、`lib/{io,scalar,containers}.hpp`のランタイムは、すでにライブラリ側の実装である。C++で書かれていること自体は移行理由にならない。coreの基本型・Range等は通常の宣言を生かし、compiler/backendに残る名前依存との接続を整理する。`source.rs`のinclude解決・重複抑止・循環検出も共通の読込基盤として残す。

## 足りない共通基盤

| 記号 | 必要な仕組み | 最初に決めること |
|---|---|---|
| G1 | 外部ジェネリック型・関数の宣言、型推論、具体化・キャッシュ | 既存の具体的な`extern type Iter<Int>`と型変数宣言を区別する。通常のジェネリクスと候補解決・重複検査を共有 |
| G2 | 構造化したC++型・宣言参照、生成名・定義順序 | `{T}`等が表す本体型と`Obj<T>`を含むABI型を区別。C++の波括弧・文字列・コメントと衝突させず、任意の語の広域置換をしない |
| G3 | 静的関数・クロージャのexternからの呼び出し | 捕捉環境、型、呼び出し位置、保持・返却を扱う。型だけの置換ではアルゴリズムを移せない |
| G4 | 型の表現・共有先・操作の意味・効果契約・信頼元の共通情報 | 外部C++型とRuskの論理型を分離。未知のexternを安全と仮定せず、標準名に依存しない解析・直接生成を可能にする |
| G5 | 構文から操作を解決する規約と期待型 | 演算子・反復・構築・入力の規約と、標準関数の専用実装の境界。Optionの`??`等の短絡・共有も保つ |
| G6 | 任意個のフィールド・payload・次元に対する導出／展開 | 構造体・enum・Tupleの自動copy等と、多次元入力を表す方法。必要性と最小の仕組みを決め、無条件に大きな言語拡張にしない |

## この調査から決める着手順

最初はG1・G2の小さな外部型と関数を、複数の型で具体化して実行する。型のC++表現と外側の`Obj<...>`を区別し、同じC++表現の異なるRusk型を混同しないことを対照にする。ここでは信頼契約・直接展開・コールバックを一度に追加しない。

次にVec移行に必要な操作参照・型情報（G2・G4）を接続する。高階処理はG3、Dict/Setはハッシュ・等値・コピーの解決、Tuple入力と自動操作はG6がそろってから移す。G6が未解決のまま、それらを「コンパイラにしか書けない例外」と確定しない。

段階1時点で例外の候補にするのはN06〜N08、L15〜L16、A07〜A08の言語・解析部分と、A09〜A10の共通のABI・依存順序・読込部分である。**Vec・Dict・Set・Option・Range・高階アルゴリズムを、その名前を理由に残す例外は設けない。** 自動操作の導出境界とG6は、段階2以降の設計事項として残る。

## 検証と更新の方法

各移行では対応行の入口・登録・生成本体・解析情報・最適化を見直し、旧経路を削除してから移行済みとする。各行に挙げた既存テストに加え、別名の利用者extern、複数の具体化、同じC++型の異なるRusk型、無効な型引数・契約を確認する。段階の区切りでは作業計画の全検証と性能比較を実施する。

この棚卸しはソースを読んだ結果で、単純な文字列検索による網羅性の証明ではない。処理の追加・分割・見落としの発見時にはIDを保持して行を追加し、最終整理で各行の移行先または残す根拠を確定する。

## β08での更新

| 対象 | 状態 | 実装と残る仕事 |
|---|---|---|
| N02 | Vec・反復型と31操作を通常の宣言へ移行、専用登録・要素制限・広域置換を撤去 | `lib/vec.rusk`をpreludeのジェネリックexternとして読み込む。比較・探索・ソートは要素の通常の操作を解決し、未知の効果には保持と変更検査を保つ。配列反復型も`Iter<Vec<T>>`の型パターンへ接続。配列入力は要素の入力を通常の型付き参照で解決する。配列出力も要素のUnit操作を通常の参照で解決し、操作の専用登録と`lib/vec_compat.rusk`を削除。Vec型も通常の外部型として具体化し、コピー・反復初期化・代入は要素の型付きcopy参照へ接続。Unitと不透明型を一律には除外しない。構文の型ヒント・Tuple等の導出・他のコンテナとそのC++補助は別項目で引き続き移行する |
| A01 | 基本型・RangeIter・Vec・配列反復型を宣言へ接続、他のコンテナは未完了 | 標準の具体・ジェネリック外部型に`representation`を付け、IRへスカラーの種類・要素・キー等を渡す。coreの基本型とRangeIter、`Vec<T>`・`Iter<Vec<T>>`の名前による分類を削除。Dict/Set・遅延反復の既存登録は残る |
| A02 | バッファ要素・整数添字を接続、他の契約は未完了 | Str→Charという射影を、宣言されたバッファ要素へ変更。要素契約の添字はスカラー表現で検証し、ネイティブ深いコピーの分類も表現を使う。enum構築・検査付きフィールド取得の契約も宣言されたvariant・fieldを検証し、Some名・variant 0の固定を削除。その他の型・保持の射影と合成は残る |
| A04 | 集約値の判定をIRの型情報へ接続 | effectsとreadonlyのInt/Bool/Char/Unitの列挙を削除。内部に共有値を持たない型か、宣言・IRから判断する。未知の外部型と不明な共有先は保守的に扱う |
| A03 | 信頼元の共通化を実装 | `Compiler`への入力で標準・利用者の由来を区別し、関数・具体化へ保持。効果契約とbackendの信頼判定から表示ファイル名への依存を削除。生成物も別の由来として保持する |
| A09のUnit返却ABI | 共通の参照へ接続 | Unitの通常のvoid返却と、保存するObj<Unit>をアダプタで接続。実装のオブジェクトABIは宣言に明記し、型付き参照・コールバック・自動コピーから共通に利用。coreコピーの前方宣言の不整合を修正。保持済み入力の`rooted_call`と標準専用の`lower_rooted`で重複保持を避け、標準テンプレートの関数定義を参照アダプタの後へ配置する。値参照とUnitの破棄用参照は、返却オブジェクトを受けて引数の後処理が済んでから値の取得・破棄を行う。コンテナ等の依存組立ての専用処理は引き続き残る |
| A02・L04の効果参照と依存 | 共通基盤を追加、型・保持情報の移行は未完了 | `writes`・`writes_invoke`・`fails`で通常の操作と静的コールバックを解決し、LIRの効果解析後に保守的な結果を参照。標準externの型付き参照・コールバックを依存として記録し、参照先の効果が明示した契約を満たすまで外側を不透明にする。未知・循環・保持・別名を楽観的に安全としない。共有先の射影を伴う効果・保持の合成は引き続き残る |
| L13・G6 | 構造体・enumのコピー・代入本体を宣言へ移行、他の導出は未完了 | 構造体とenumのcopyが実際に選んだフィールド操作を記録し、全依存の非変更・非保持・非別名が証明できる場合だけ独立したコピーの契約を有効にする。未知の操作には元のオブジェクトを保持する。構造体・Tuple・捕捉環境・enumのコピー本体は`lib/collections.rusk`の役割を宣言したテンプレートへ移し、明示したフィールド展開と通常の型付き参照で組み立てる。variant・payloadの列挙も共通化し、コピー依存の登録・Unitの格納ABIを通常の参照へ接続。代入本体も標準の役割に接続し、コピーを型付きの参照から呼ぶ。外側の同一性と解放順序を維持し、効果は不透明のまま扱う。Tupleの比較・出力等の導出と、未使用のcopyまでフィールド操作を要求する制約は残り、[操作導出の設計案](https://github.com/shogo314/rusk_b_08/blob/main/design/beta08-aggregate-derivation.md)で移行境界を検討する |
| L10・L12 | Optionの9宣言を移行、構文・他コンテナは未完了 | Some/None/Option/is_some/is_none/bool/not/value/try_readを`lib/collections.rusk`へ移し、Option特殊化時の自動登録・操作本体の生成・構築の専用経路・Someの型ヒントを削除。Unitの値も共有して返し、入力は通常の型付き参照で解決する。`??`・`?!`に残るOption判定、Dict/Set構築と既定値の選択は引き続き移行する |
| L03 | 移行済み | first_true・sort_by_key・sort_by_compを`lib/algorithms.rusk`の通常のジェネリックexternへ移し、`higher_order.rs`の専用分岐を削除。別名の利用者定義でも実行する。キーのコピー・比較は型付き参照を使い、基本型に限定する専用判定も削除 |
| A05 | 宣言に生成情報を接続 | 基本演算・入出力・反復・添字の式をcore/vec宣言の`lower`・`lower_owned`・`lower_borrowed`へ移し、backendの関数名別テーブルを削除。所有・借用の証明は従来の解析で行う。Vecの基本操作も通常のジェネリック宣言へ接続し、広域置換を削除 |
| A06 | 表現と代入の情報を接続 | `operation:"assign"`に加え、IRのスカラー表現で直接保持・共有の候補を選ぶ。scalar_aliasesとcppの基本型名の列挙を削除。Unit返却・Bool条件等の言語上の規約はA08の検査として維持する |
| L02 | 移行済み | all/any/fold/scan/to_vecを`lib/algorithms.rusk`の通常の宣言へ移し、共通の専用生成ブロックを削除。反復・コピー・コールバックは型付き参照から解決し、利用者定義のジェネリックな反復型でも動かす |
| N01・N06・L04・L05・L16・A09 | 基盤を追加、専用経路の移行は未完了 | 外部型・関数の具体化、型・関数・コールバックの参照、未使用宣言の検査、外部関数の前方宣言・相互再帰を実装。操作本体と型・効果情報の移行はこれから |
