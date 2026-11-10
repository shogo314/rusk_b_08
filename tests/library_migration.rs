mod support;
use support::Case;

#[test]
fn vector_search_and_sort_use_element_operations_stability_and_sharing() {
    let case = Case::new(
        r#"
struct Row{key:Int;id:Int;visits:Vec<Int>;}
fn __lt__(a:Row,b:Row)->Bool{a.visits[0]+=1;return a.key<b.key;}
fn __eq__(a:Row,b:Row)->Bool{a.visits[0]+=1;return a.key==b.key;}
let visits:=[0];
let rows:=[Row{key:1,id:0,visits:visits},Row{key:0,id:1,visits:visits},Row{key:1,id:2,visits:visits}];
let ordered:=sorted(rows);for row in ordered{wt row.id;}
ordered[0].key=8;wt rows[1].key;ordered[0].key=0;
sort(rows);for row in rows{wt row.id;}
let key:=Row{key:1,id:99,visits:visits};
wt bisect_left(rows,key),bisect_right(rows,key),contains(rows,key),contains(rows,Row{key:9,id:99,visits:visits}),visits[0]>0;
let empty:Vec<Int>:=[];sort(empty);wt sorted(empty).len(),bisect_left(empty,3),bisect_right(empty,3),contains(empty,3);
let values:=[3,1,2,1];let view:=sorted(values);wt values,view;
view[0]=9;wt values;sort(values);wt values;
"#,
    );
    for options in [vec![], vec!["--no-opt"]] {
        let out = case.run_with_options("", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            out.stdout,
            b"1\n0\n2\n8\n1\n0\n2\n1 3 Yes No Yes\n0 0 0 No\n3 1 2 1 1 1 2 3\n3 9 2 1\n1 2 3 9\n"
        );
    }
    for source in [
        "let values:Vec<Int>:=[];let cursor:=values.map((x:Int)=>x);wt \"before\";sort(values);wt cursor.to_vec();",
        "struct Row{key:Int;visits:Vec<Int>;} fn __lt__(a:Row,b:Row)->Bool{a.visits[0]+=1;return a.key<b.key;} let visits:=[0];let values:=[Row{key:2,visits:visits},Row{key:1,visits:visits}];let cursor:=values.map((x:Row)=>x.key);wt \"before\";sort(values);wt cursor.to_vec();",
    ] {
        for options in [vec![], vec!["--no-opt"]] {
            let out = Case::new(source).run_with_options("", &options);
            assert!(!out.status.success());
            assert_eq!(out.stdout, b"before\n");
            assert!(
                String::from_utf8_lossy(&out.stderr).contains("container changed during iteration")
            );
        }
    }
}

#[test]
fn vector_search_and_sort_guard_calls_and_deferred_element_release() {
    let definitions = r#"
extern symbol watched {code:"Obj<RuskVec<RuskVec<std::int64_t>>> watched;bool changed=false;",}
extern fn watch(a:Vec<Vec<Int>>) {cpp_name:"watch",code:"void watch(Obj<RuskVec<RuskVec<std::int64_t>>> a,Loc){watched=a;}",}
extern fn inner()->Vec<Int>{cpp_name:"inner",code:"Obj<RuskVec<std::int64_t>> inner(Loc){return Obj<RuskVec<std::int64_t>>(new RuskVec<std::int64_t>{},[](auto* p){delete p;if(watched){watched->clear();++watched->version;}});}",}
extern fn __eq__(a:Vec<Int>,b:Vec<Int>)->Bool {cpp_name:"equal",code:"Obj<bool> equal(Obj<RuskVec<std::int64_t>> a,Obj<RuskVec<std::int64_t>> b,Loc){ACTION}",}
extern fn __lt__(a:Vec<Int>,b:Vec<Int>)->Bool {cpp_name:"less",code:"Obj<bool> less(Obj<RuskVec<std::int64_t>> a,Obj<RuskVec<std::int64_t>> b,Loc){ACTION}",}
"#;
    let mut cases = vec![];
    for action in [
        "watched->clear();++watched->version;return make_obj<bool>(true);",
        "return Obj<bool>(new bool(true),[](bool* p){delete p;watched->clear();++watched->version;});",
    ] {
        for operation in [
            "sort(a);",
            "sorted(a);",
            "contains(a,needle);",
            "bisect_left(a,needle);",
            "bisect_right(a,needle);",
        ] {
            cases.push((action, operation));
        }
    }
    for operation in [
        "contains(a,needle);",
        "bisect_left(a,needle);",
        "bisect_right(a,needle);",
    ] {
        cases.push(("for(auto& slot:*watched){if(slot==a||slot==b){slot=make_obj<RuskVec<std::int64_t>>(RuskVec<std::int64_t>{});break;}}return make_obj<bool>(false);",operation));
    }
    cases.push(("if(!changed){changed=true;(*watched)[0]=Obj<RuskVec<std::int64_t>>(new RuskVec<std::int64_t>{},[](auto* p){delete p;watched->clear();++watched->version;});}return make_obj<bool>(false);","sort(a);"));
    for (action, operation) in cases {
        let source = format!(
            "{} let a:Vec<Vec<Int>>:=[[],inner()];let needle:Vec<Int>:=[];watch(a);wt \"before\";{operation}",
            definitions.replace("ACTION", action)
        );
        for options in [vec![], vec!["--no-opt"]] {
            let out = Case::new(&source).run_with_options("", &options);
            assert!(!out.status.success());
            assert_eq!(out.stdout, b"before\n");
            let error = String::from_utf8_lossy(&out.stderr);
            assert!(
                error.contains("main.rusk:")
                    && error.contains("container changed during comparison"),
                "{operation} {action}\n{error}"
            );
        }
    }
}

#[test]
fn vector_comparisons_resolve_user_operations_and_preserve_lexicographic_order() {
    let case = Case::new(
        r#"
struct Key{value:Int;log:Vec<Int>;}
fn __eq__(a:Key,b:Key)->Bool{a.log.push_back(1);return a.value==b.value;}
fn __lt__(a:Key,b:Key)->Bool{a.log.push_back(2);return a.value<b.value;}
let log:Vec<Int>:=[];
let a:=[Key{value:1,log:log},Key{value:2,log:log}];
let b:=[Key{value:1,log:log},Key{value:3,log:log}];
wt a==b,a!=b,a<b,a<=b,a>b,a>=b,log.len();
let nested_a:Vec<Vec<Vec<Int>>>:=[[[1]],[]];
let nested_b:Vec<Vec<Vec<Int>>>:=[[[2]],[]];
let nested_c:Vec<Vec<Vec<Int>>>:=[[[1]],[]];
wt nested_a<nested_b,nested_a==nested_c;
let empty:Vec<Key>:=[];wt empty==empty,empty<empty,log.len();
"#,
    );
    for options in [vec![], vec!["--no-opt"]] {
        let out = case.run_with_options("", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"No Yes Yes Yes No No 18\nYes Yes\nYes No 18\n");
    }
    let out = Case::new(
        r#"
struct Key{value:Int;}
fn __eq__(a:Key,b:Key)->Int{return 1;}
wt [Key{value:1}]==[Key{value:1}];
"#,
    )
    .compile();
    assert!(!out.status.success());
    let error = String::from_utf8_lossy(&out.stderr);
    assert!(error.contains("Int") && error.contains("Bool"), "{error}");
}

#[test]
fn vector_comparison_dependencies_preserve_readonly_and_noescape_boundaries() {
    let pure = r#"
struct Key{value:Int;}
fn __eq__(a:Key,b:Key)->Bool{return a.value==b.value;}
fn compare(readonly noescape a:Vec<Key>,readonly noescape b:Vec<Key>)->Bool{return a==b;}
wt compare([Key{value:1}],[Key{value:1}]);
"#;
    for options in [vec![], vec!["--no-opt"]] {
        let out = Case::new(pure).run_with_options("", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"Yes\n");
    }
    for equality in [
        "fn __eq__(a:Key,b:Key)->Bool{a.value=2;return a.value==b.value;}",
        "extern fn opaque(a:Int,b:Int)->Bool{cpp_name:\"opaque_equal\",code:\"Obj<bool> opaque_equal(Obj<std::int64_t> a,Obj<std::int64_t> b,Loc){return make_obj<bool>(*a==*b);}\",} fn __eq__(a:Key,b:Key)->Bool{return opaque(a.value,b.value);}",
    ] {
        for qualifier in ["readonly", "noescape"] {
            let out=Case::new(&format!("struct Key{{value:Int;}} {equality} fn compare({qualifier} a:Vec<Key>,b:Vec<Key>)->Bool{{return a==b;}} compare([Key{{value:1}}],[Key{{value:1}}]);")).compile();
            assert!(!out.status.success());
            let error = String::from_utf8_lossy(&out.stderr);
            assert!(error.contains(qualifier), "{error}");
        }
    }
}

#[test]
fn vector_comparisons_check_mutation_in_comparison_calls_and_cleanup() {
    let definitions = r#"
extern symbol watched {code:"Obj<RuskVec<RuskVec<std::int64_t>>> watched;",}
extern fn watch(a:Vec<Vec<Int>>) {cpp_name:"watch",code:"void watch(Obj<RuskVec<RuskVec<std::int64_t>>> a,Loc){watched=a;}",}
extern fn inner()->Vec<Int>{cpp_name:"inner",code:"Obj<RuskVec<std::int64_t>> inner(Loc){return Obj<RuskVec<std::int64_t>>(new RuskVec<std::int64_t>{},[](auto* p){delete p;if(watched){watched->clear();++watched->version;}});}",}
extern fn __eq__(a:Vec<Int>,b:Vec<Int>)->Bool {cpp_name:"compare_equal",code:"Obj<bool> compare_equal(Obj<RuskVec<std::int64_t>> a,Obj<RuskVec<std::int64_t>> b,Loc){ACTION}",}
extern fn __lt__(a:Vec<Int>,b:Vec<Int>)->Bool {cpp_name:"compare_less",code:"Obj<bool> compare_less(Obj<RuskVec<std::int64_t>> a,Obj<RuskVec<std::int64_t>> b,Loc){ACTION}",}
"#;
    for action in [
        "watched->clear();++watched->version;return make_obj<bool>(false);",
        "return Obj<bool>(new bool(false),[](bool* p){delete p;watched->clear();++watched->version;});",
        "(*watched)[0]=make_obj<RuskVec<std::int64_t>>(RuskVec<std::int64_t>{});return make_obj<bool>(false);",
    ] {
        for operation in ["==", "!=", "<", "<=", ">", ">="] {
            let source = format!(
                "{} let a:=[inner()];let b:Vec<Vec<Int>>:=[[]];watch(a);wt \"before\";wt a{operation}b;",
                definitions.replace("ACTION", action)
            );
            for options in [vec![], vec!["--no-opt"]] {
                let out = Case::new(&source).run_with_options("", &options);
                assert!(!out.status.success());
                assert_eq!(out.stdout, b"before\n");
                let error = String::from_utf8_lossy(&out.stderr);
                assert!(
                    error.contains("main.rusk:")
                        && error.contains("container changed during comparison"),
                    "{operation}: {action}\n{error}"
                );
            }
        }
    }
}

#[test]
fn vector_operations_are_generic_declarations_with_independent_names() {
    // User declarations cannot assert the standard library's effect/lowering contracts.
    let source = include_str!("../lib/vec.rusk");
    let mut declarations = source[source.find("extern fn").unwrap()..]
        .lines()
        .filter(|line| {
            ![
                "effects:",
                "aliases:",
                "lower:",
                "lower_owned:",
                "lower_borrowed:",
            ]
            .iter()
            .any(|property| line.trim_start().starts_with(property))
        })
        .collect::<Vec<_>>()
        .join("\n");
    for name in [
        "__write__",
        "__read__",
        "__vec_new__",
        "__vec_repeat__",
        "len",
        "push_back",
        "__getitem__",
        "__setitem__",
        "copy",
        "__assign__",
        "__bool__",
        "reverse",
        "__add__",
        "__iadd__",
        "__iter__",
        "__iter_has_next__",
        "__iter_next__",
        "__not__",
        "Bool",
        "pop_back",
        "__eq__",
        "__ne__",
        "__lt__",
        "__le__",
        "__gt__",
        "__ge__",
        "contains",
        "bisect_left",
        "bisect_right",
        "sort",
        "sorted",
    ] {
        declarations =
            declarations.replace(&format!("fn {name}<"), &format!("fn migrated_{name}<"));
    }
    let case = Case::new(&format!(
        r#"{declarations}
let printed:Vec<Vec<Int>>:=[[1,2],[],[3]];migrated___write__(printed);wt "done";
wt migrated___read__::<Int>(2);
let empty:=migrated___vec_new__::<Int>();
wt migrated_len(empty),migrated___bool__(empty),migrated___not__(empty),migrated_Bool(empty);
migrated_push_back(empty,4);wt migrated___getitem__(empty,0);
let seed:=[1];let values:=migrated___vec_repeat__(seed,2);
seed[0]=8;values[0][0]=3;wt seed,values;
let snapshot:=migrated_copy(values);snapshot[0][0]=9;wt values,snapshot;
let joined:=migrated___add__(values,snapshot);joined[0][0]=6;wt values;
migrated___iadd__(empty,empty);wt migrated_len(empty),migrated_pop_back(empty);
migrated___assign__(values,values);wt values;
migrated_reverse(values);let cursor:=migrated___iter__(values);
while migrated___iter_has_next__(cursor){{wt migrated___iter_next__(cursor);}}
wt migrated___setitem__(empty,0,7),empty;
struct Row{{value:Int;}}
let rows:=migrated___vec_repeat__(Row{{value:2}},2);rows[0].value=5;
wt rows[0].value,rows[1].value,migrated_len(rows);
wt migrated___eq__([1,2],[1,2]),migrated___ne__([1],[2]),migrated___lt__([1],[2]),migrated___le__([1],[1]),migrated___gt__([2],[1]),migrated___ge__([1],[1]);
let ordered:=migrated_sorted([3,1,2]);migrated_sort(ordered);wt ordered,migrated_bisect_left(ordered,2),migrated_bisect_right(ordered,2),migrated_contains(ordered,2);
"#
    ));
    for options in [vec![], vec!["--no-opt"]] {
        let out = case.run_with_options("4 5\n", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            out.stdout,
            b"1 2  3done\n4 5\n0 No Yes No\n4\n8 3 1\n3 1 9 1\n6 1\n2 4\n6 1\n1\n6\n7 7\n5 2 2\nYes Yes Yes Yes Yes Yes\n1 2 3 1 2 Yes\n"
        );
    }
}

#[test]
fn key_sort_checks_structural_changes_inside_user_key_operations_and_cleanup() {
    let source = r#"
extern symbol watched_source {code:"Obj<RuskVec<std::int64_t>> watched;",}
extern type Key {cpp_name:"std::int64_t",}
extern fn make_key(source:Vec<Int>,x:Int)->Key {
    cpp_name:"watch_key",code:"Obj<std::int64_t> watch_key(Obj<RuskVec<std::int64_t>> source,Obj<std::int64_t> x,Loc){watched=source;return KEY_OBJECT;}",
}
extern fn copy(x:Key)->Key {cpp_name:"watch_key_copy",code:"Obj<std::int64_t> watch_key_copy(Obj<std::int64_t> x,Loc){COPY_CHANGE return make_obj<std::int64_t>(*x);}",}
extern fn __lt__(a:Key,b:Key)->Bool {
    cpp_name:"watch_key_less",code:"Obj<bool> watch_key_less(Obj<std::int64_t> a,Obj<std::int64_t> b,Loc){COMPARE_CHANGE return make_obj<bool>(*a<*b);}",
}
let values:=VALUES;wt "before";sort_by_key(values,(x:Int)=>make_key(values,x));
"#;
    let change = "watched->clear();++watched->version;";
    let normal = "make_obj<std::int64_t>(*x)";
    let on_drop = "Obj<std::int64_t>(new std::int64_t(*x),[](auto* value){delete value;watched->clear();++watched->version;})";
    for (copy, compare, key, values) in [
        (change, "", normal, "[2,1]"),
        ("", change, normal, "[2,1]"),
        ("", "", on_drop, "[2,1]"),
        ("", "", on_drop, "[2]"),
    ] {
        let case = Case::new(
            &source
                .replace("COPY_CHANGE", copy)
                .replace("COMPARE_CHANGE", compare)
                .replace("KEY_OBJECT", key)
                .replace("VALUES", values),
        );
        for options in [vec![], vec!["--no-opt"]] {
            let out = case.run_with_options("", &options);
            assert!(!out.status.success());
            assert_eq!(out.stdout, b"before\n");
            let error = String::from_utf8_lossy(&out.stderr);
            assert!(
                error.contains("main.rusk:") && error.contains("container changed during callback"),
                "{error}"
            );
        }
    }
}

#[test]
fn key_sort_snapshots_keys_once_and_accepts_user_comparison_operations() {
    let declarations = include_str!("../lib/algorithms.rusk")
        .replace("fn first_true<", "fn boundary_search<")
        .replace("fn all<", "fn every_item<")
        .replace("fn any<", "fn some_item<")
        .replace("fn fold<", "fn reduce_items<")
        .replace("fn scan<", "fn prefixes<")
        .replace("fn to_vec<", "fn collect_items<")
        .replace("fn sort_by_comp<", "fn order_items<")
        .replace("fn sort_by_key<", "fn order_keys<");
    let case = Case::new(&format!(
        r#"{declarations}
struct Row{{key:Int;id:Int;}}
let values:=[Row{{key:1,id:0}},Row{{key:0,id:1}},Row{{key:1,id:2}}];
let shared:=[0];let visits:=0;
order_keys(values,(x:Row)=>{{visits+=1;shared[0]=x.key;shared}});
for value in values{{wt value.id;}}wt visits;
struct Key{{value:Int;}}
fn __lt__(a:Key,b:Key)->Bool{{return a.value<b.value;}}
order_keys(values,(x:Row)=>Key{{value:-x.key}});
for value in values{{wt value.id;}}
let tuples:=[(1,2),(0,4),(1,0)];order_keys(tuples,(x:(Int,Int))=>x);wt tuples;
"#
    ));
    for options in [vec![], vec!["--no-opt"]] {
        let out = case.run_with_options("", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"1\n0\n2\n3\n0\n2\n1\n0 4 1 0 1 2\n");
    }
}

#[test]
fn binary_search_is_an_ordinary_declaration_and_works_under_an_unrelated_name() {
    let declaration = include_str!("../lib/algorithms.rusk")
        .replace("fn first_true<", "fn boundary_search<")
        .replace("fn all<", "fn every_item<")
        .replace("fn any<", "fn some_item<")
        .replace("fn to_vec<", "fn collect_items<")
        .replace("fn fold<", "fn reduce_items<")
        .replace("fn scan<", "fn prefixes<")
        .replace("fn sort_by_comp<", "fn order_items<")
        .replace("fn sort_by_key<", "fn order_keys<");
    let case = Case::new(&format!(
        r#"{declaration}
let threshold:=40;let predicate:=(x:Int)=>x*x>=threshold;
wt first_true(0,100,predicate),boundary_search(0,100,predicate);
wt boundary_search(-9223372036854775808,9223372036854775807,(x:Int)=>x>=0);
let calls:=0;wt boundary_search(3,3,(x:Int)=>{{calls+=1;true}}),calls;
fn first_true(a:Int,b:Int,flag:Bool)->Int{{return if flag{{a}}else{{b}};}}
wt first_true(2,9,false);
"#
    ));
    for options in [vec![], vec!["--no-opt"]] {
        let out = if options.is_empty() {
            case.run("")
        } else {
            case.run_with_options("", &options)
        };
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"7 7\n0\n3 0\n9\n");
    }
}

#[test]
fn library_search_preserves_empty_interval_failures_and_prior_output() {
    let case = Case::new("wt \"before\";first_true(5,2,(x:Int)=>true);");
    let mut previous = None;
    for options in [vec![], vec!["--no-opt"]] {
        let out = case.run_with_options("", &options);
        let error = String::from_utf8_lossy(&out.stderr).into_owned();
        assert!(!out.status.success());
        assert_eq!(out.stdout, b"before\n");
        assert!(
            error.contains("main.rusk:1:") && error.contains("invalid binary search interval"),
            "{error}"
        );
        if let Some(previous) = previous {
            assert_eq!(previous, error);
        }
        previous = Some(error);
    }
}

#[test]
fn predicates_use_user_defined_generic_iteration_without_container_name_dispatch() {
    let declarations = include_str!("../lib/algorithms.rusk")
        .replace("fn first_true<", "fn boundary_search<")
        .replace("fn all<", "fn every_item<")
        .replace("fn any<", "fn some_item<")
        .replace("fn to_vec<", "fn collect_items<")
        .replace("fn fold<", "fn reduce_items<")
        .replace("fn scan<", "fn prefixes<")
        .replace("fn sort_by_comp<", "fn order_items<")
        .replace("fn sort_by_key<", "fn order_keys<");
    let case = Case::new(&format!(
        r#"{declarations}
struct Feed<T>{{values:Vec<T>;}}
struct Cursor<T>{{values:Vec<T>;position:Int;}}
fn __iter__<T>(s:Feed<T>)->Cursor<T>{{return Cursor::<T>{{values:s.values,position:0}};}}
fn __iter_has_next__<T>(it:Cursor<T>)->Bool{{return it.position<it.values.len();}}
fn __iter_next__<T>(it:Cursor<T>)->T{{let value:=it.values[it.position];it.position+=1;return value;}}
let source:=Feed::<Int>{{values:[1,2,3]}};
wt all(source,(x:Int)=>x>0),any(source,(x:Int)=>x==9);
let count:=0;let predicate:=(x:Int)=>{{count+=1;x==2}};
wt some_item(source,predicate),count;
count=0;wt every_item(source,(x:Int)=>{{count+=1;x<2}}),count;
let empty:=Feed::<Int>{{values:[]}};
count=0;wt all(empty,predicate),any(empty,predicate),count;
wt all(0..4,(x:Int)=>x<4),any([1,2,3].map((x:Int)=>x*2),(x:Int)=>x==4);
wt collect_items(source),reduce_items(source,0,(a:Int,b:Int)=>a+b),prefixes(source,0,(a:Int,b:Int)=>a+b);
"#
    ));
    for options in [vec![], vec!["--no-opt"]] {
        let out = case.run_with_options("", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            out.stdout,
            b"Yes No\nYes 2\nNo 2\nYes No 0\nYes Yes\n1 2 3 6 0 1 3 6\n"
        );
    }
}

#[test]
fn predicates_keep_structural_mutation_guards_even_when_they_short_circuit() {
    for operation in ["all", "any"] {
        let accepted = operation == "any";
        let source = format!(
            r#"let source:=[1,2];wt "before";{operation}(source,(x:Int)=>{{source.push_back(3);{accepted}}});"#
        );
        for options in [vec![], vec!["--no-opt"]] {
            let out = Case::new(&source).run_with_options("", &options);
            let error = String::from_utf8_lossy(&out.stderr);
            assert!(!out.status.success());
            assert_eq!(out.stdout, b"before\n");
            assert!(
                error.contains("changed") && error.contains("main.rusk:"),
                "{error}"
            );
        }
    }
}

#[test]
fn reductions_preserve_copy_hooks_snapshots_and_collection_sharing() {
    let case = Case::new(
        r#"
extern symbol observations {code:"inline unsigned long long rusk_copy_count=0;",}
extern type Token {cpp_name:"std::int64_t",}
extern fn Token(x:Int)->Token {cpp_name:"make_token",code:"Obj<std::int64_t> make_token(Obj<std::int64_t> x,Loc){return make_obj<std::int64_t>(*x);}",}
extern fn copy(x:Token)->Token {cpp_name:"copy_token",code:"Obj<std::int64_t> copy_token(Obj<std::int64_t> x,Loc){++rusk_copy_count;return make_obj<std::int64_t>(*x);}",}
extern fn value(x:Token)->Int {cpp_name:"token_value",code:"Obj<std::int64_t> token_value(Obj<std::int64_t> x,Loc){return x;}",}
extern fn copies()->Int {cpp_name:"copy_count",code:"Obj<std::int64_t> copy_count(Loc){return make_obj<std::int64_t>(rusk_copy_count);}",}
struct Acc{token:Token;}
let seed:=Acc{token:Token(5)};
let result:=scan([1,2],seed,(a:Acc,x:Int)=>{a.token:=Token(x);a});
wt copies(),value(seed.token),value(result[0].token),value(result[1].token),value(result[2].token);
let initial:=[0];let snapshots:=scan([1,2],initial,(a:Vec<Int>,x:Int)=>{a.push_back(x);a});
snapshots[1][0]=9;wt initial,snapshots[0],snapshots[1],snapshots[2];
let input:=[[3],[4]];let collected:=to_vec(input);collected[0][0]=8;wt input[0][0],collected[1][0];
let folded:=fold([1,2],initial,(a:Vec<Int>,x:Int)=>{a.push_back(x);a});wt initial,folded;
"#,
    );
    for options in [vec![], vec!["--no-opt"]] {
        let out = case.run_with_options("", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"4 5 5 1 2\n0 0 9 1 0 1 2\n8 4\n0 0 1 2\n");
    }
}

#[test]
fn comparator_sort_preserves_stability_and_is_available_as_a_user_declaration() {
    let declarations = include_str!("../lib/algorithms.rusk")
        .replace("fn first_true<", "fn boundary_search<")
        .replace("fn all<", "fn every_item<")
        .replace("fn any<", "fn some_item<")
        .replace("fn fold<", "fn reduce_items<")
        .replace("fn scan<", "fn prefixes<")
        .replace("fn to_vec<", "fn collect_items<")
        .replace("fn sort_by_comp<", "fn order_items<")
        .replace("fn sort_by_key<", "fn order_keys<");
    let case = Case::new(&format!(
        r#"{declarations}
struct Row{{key:Int;id:Int;}}
let values:=[Row{{key:1,id:0}},Row{{key:0,id:1}},Row{{key:1,id:2}}];
order_items(values,(a:Row,b:Row)=>a.key<b.key);
for value in values{{wt value.id;}}
"#
    ));
    for options in [vec![], vec!["--no-opt"]] {
        let out = case.run_with_options("", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"1\n0\n2\n");
    }
    for options in [vec![], vec!["--no-opt"]] {
        let out = Case::new(
            "let values:=[2,1];sort_by_comp(values,(a:Int,b:Int)=>{values.push_back(3);a<b});",
        )
        .run_with_options("", &options);
        assert!(!out.status.success());
        assert!(String::from_utf8_lossy(&out.stderr).contains("container changed during callback"));
    }
}

#[test]
fn vector_input_preserves_custom_readers_and_snapshots_the_length() {
    let case = Case::new(
        r#"
struct Row {value:Int;}
extern symbol input_state {code:"Obj<std::int64_t> input_length;",}
extern fn watch(n:Int) {cpp_name:"watch_input_length",code:"void watch_input_length(Obj<std::int64_t> n,Loc){input_length=n;}",}
extern fn __read__()->Row {
    cpp_name:"reader::row",
    code:"namespace reader {Obj<RuskRecord0> row(Loc loc){++*input_length;return make_obj<RuskRecord0>(RuskRecord0{make_obj<std::int64_t>(rusk_read_int(loc))});}}",
}
let n:=2;watch(n);in rows(n):Vec<Row>;wt rows.len(),rows[0].value,rows[1].value,n;
in tail:Int;wt tail;
"#,
    );
    for options in [vec![], vec!["--no-opt"]] {
        let out = case.run_with_options("7 8 9", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"2 7 8 4\n9\n");
    }
}

#[test]
fn structured_input_boxes_void_unit_and_preserves_object_unit_cleanup() {
    for (reader, expected) in [
        ("fn __read__()->Unit {}", "0\n"),
        (
            r#"extern fn __read__()->Unit {
            return_abi:"object",cpp_name:"reader::unit",
            code:"namespace reader {Obj<Unit> unit(Loc){return Obj<Unit>(new Unit{},[](Unit* p){++*input_cleanup;delete p;});}}",
        }"#,
            "3\n",
        ),
    ] {
        let case = Case::new(&format!(
            r#"
extern symbol input_state {{code:"Obj<std::int64_t> input_cleanup;",}}
extern fn watch(n:Int) {{cpp_name:"watch_input_cleanup",code:"void watch_input_cleanup(Obj<std::int64_t> n,Loc){{input_cleanup=n;}}",}}
{reader}
let count:=0;watch(count);
{{in pair:(Unit,Int),rows(2):Vec<(Unit,Int)>;wt pair[1],rows[0][1],rows[1][1],count;}}
wt count;
"#
        ));
        for options in [vec![], vec!["--no-opt"]] {
            let out = case.run_with_options("3 4 5", &options);
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            assert_eq!(
                String::from_utf8_lossy(&out.stdout),
                format!("3 4 5 0\n{expected}")
            );
        }
    }
}

#[test]
fn vector_input_reports_scalar_reader_errors_at_the_input_site() {
    for (ty, inputs) in [
        (
            "Int",
            vec![
                ("bad", "invalid integer input"),
                ("9223372036854775808", "integer input out of range"),
                ("", "unexpected end of input"),
            ],
        ),
        (
            "Char",
            vec![("", "unexpected end of input"), ("é", "non-ASCII input")],
        ),
        ("Bool", vec![("maybe", "invalid boolean input")]),
        ("Str", vec![("é", "non-ASCII input")]),
    ] {
        let case = Case::new(&format!(
            "wt \"before\";\nin values(1):Vec<{ty}>;\nwt \"after\";"
        ));
        for options in [vec![], vec!["--no-opt"]] {
            for (index, (input, message)) in inputs.iter().enumerate() {
                let out = if index == 0 {
                    case.run_with_options(input, &options)
                } else {
                    case.execute(input)
                };
                assert!(!out.status.success(), "{ty}: {input}");
                assert_eq!(out.stdout, b"before\n");
                let error = String::from_utf8_lossy(&out.stderr);
                assert!(
                    error.contains("main.rusk:2:") && error.contains(message),
                    "{ty}: {error}"
                );
            }
        }
    }
}

#[test]
fn vector_output_resolves_user_writers_and_proves_readonly_noescape_only_when_covered() {
    let pure = r#"
struct Row{ch:Char;}
fn __write__(row:Row){__write__(row.ch);}
fn show(readonly noescape rows:Vec<Row>){wt rows;}
let text:="ab";{let rows:=[Row{ch:text[0]},Row{ch:text[1]}];show(rows);}
text.push_back('!');wt text;
let matrix:Vec<Vec<Int>>:=[[],[1,2],[],[3],[]];wt matrix;
wt [true,false],["a","b"],['x','y'];
struct Number{value:Int;}
fn __write__(row:Number){__write__(row.value+1);}
fn show_numbers(readonly noescape rows:Vec<Number>){wt rows;}
show_numbers([Number{value:1},Number{value:2}]);
"#;
    let changing = r#"
struct Row{value:Int;}
fn __write__(row:Row){__write__(row.value);row.value+=1;}
let rows:=[Row{value:1},Row{value:2}];wt rows;wt rows;
"#;
    for (source, expected) in [
        (pure, "a b\nab!\n 1 2  3 \nYes No a b x y\n2 3\n"),
        (changing, "1 2\n2 3\n"),
    ] {
        let case = Case::new(source);
        for options in [vec![], vec!["--no-opt"]] {
            let out = case.run_with_options("", &options);
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            assert_eq!(out.stdout, expected.as_bytes());
        }
    }
    for restriction in ["readonly", "noescape"] {
        let source = format!(
            r#"
struct Row{{value:Int;}}
extern fn __write__(row:Row){{cpp_name:"opaque_writer",code:"void opaque_writer(Obj<RuskRecord0>,Loc){{}}",}}
fn show({restriction} rows:Vec<Row>){{wt rows;}}
show([Row{{value:1}}]);
"#
        );
        let out = Case::new(&source).compile();
        assert!(!out.status.success());
        assert!(String::from_utf8_lossy(&out.stderr).contains(restriction));
    }
}

#[test]
fn vector_output_checks_mutations_in_writers_return_cleanup_and_element_release() {
    let definitions = r#"
extern symbol output_state {code:"Obj<RuskVec<RuskVec<std::int64_t>>> watched;",}
extern fn watch(a:Vec<Vec<Int>>) {cpp_name:"watch",code:"void watch(Obj<RuskVec<RuskVec<std::int64_t>>> a,Loc){watched=a;}",}
extern fn inner()->Vec<Int>{cpp_name:"inner",code:"Obj<RuskVec<std::int64_t>> inner(Loc){return Obj<RuskVec<std::int64_t>>(new RuskVec<std::int64_t>{},[](auto* p){delete p;if(watched){watched->clear();++watched->version;}});}",}
extern fn __write__(a:Vec<Int>)->Unit {
    return_abi:"object",cpp_name:"native::write_inner",
    code:"namespace native {Obj<Unit> write_inner(Obj<RuskVec<std::int64_t>> a,Loc){rusk_write('x');ACTION}}",
}
let rows:=[inner(),[2]];watch(rows);wt "before";wt rows;
"#;
    for action in [
        "watched->clear();++watched->version;return make_obj<Unit>(Unit{});",
        "return Obj<Unit>(new Unit{},[](auto* p){delete p;watched->clear();++watched->version;});",
        "(*watched)[0]=make_obj<RuskVec<std::int64_t>>(RuskVec<std::int64_t>{});return make_obj<Unit>(Unit{});",
    ] {
        let case = Case::new(&definitions.replace("ACTION", action));
        for options in [vec![], vec!["--no-opt"]] {
            let out = case.run_with_options("", &options);
            assert!(!out.status.success());
            assert_eq!(out.stdout, b"before\nx");
            let error = String::from_utf8_lossy(&out.stderr);
            assert!(
                error.contains("main.rusk:") && error.contains("container changed during output"),
                "{error}"
            );
        }
    }
}

#[test]
fn declared_vector_types_support_opaque_copy_hooks_and_unit_storage() {
    let opaque = r#"
extern type Score{cpp_name:"std::int64_t",}
extern symbol copy_counter{code:"std::int64_t score_copies=0;",}
extern fn score(x:Int)->Score {cpp_name:"make_score",code:"Obj<std::int64_t> make_score(Obj<std::int64_t> x,Loc){return make_obj<std::int64_t>(*x);}",}
extern fn value(x:Score)->Int {cpp_name:"score_value",code:"Obj<std::int64_t> score_value(Obj<std::int64_t> x,Loc){return x;}",}
extern fn copies()->Int {cpp_name:"score_count",code:"Obj<std::int64_t> score_count(Loc){return make_obj<std::int64_t>(score_copies);}",}
extern fn copy(x:Score)->Score {cpp_name:"user::copy",code:"namespace user{Obj<std::int64_t> copy(Obj<std::int64_t> x,Loc){++score_copies;return make_obj<std::int64_t>(*x);}}",}
extern fn __write__(x:Score){cpp_name:"write_score",code:"void write_score(Obj<std::int64_t> x,Loc){rusk_write(*x);}",}
let seed:=score(2);let rows:=[seed;2];wt copies();
let independent=rows;wt copies();value(rows[0])=7;wt value(seed),rows,independent;
rows=independent;wt copies(),rows;let held:=rows.pop_back();wt value(held),rows.len();
"#;
    let unit = r#"
extern fn same(a:Unit,b:Unit)->Bool{cpp_name:"same_unit",code:"Obj<bool> same_unit(Obj<Unit> a,Obj<Unit> b,Loc){return make_obj<bool>(a==b);}",}
let original:={};let values:=__vec_new__::<Unit>();values.push_back(original);
let held:=values[0];wt same(held,original);
let detached=values;wt same(detached[0],original);
let repeated:=[original;2];wt same(repeated[0],repeated[1]);
values[0]:={};wt same(held,original),same(values[0],original);
let stored:=values[0];let popped:=values.pop_back();wt same(popped,stored),values.len();
let count:=0;for item in repeated{if same(item,repeated[0]){count+=1;}}wt count;
"#;
    for (source, expected) in [
        (opaque, "2\n4\n2 7 2 2 2\n6 2 2\n2 1\n"),
        (unit, "Yes\nNo\nNo\nYes No\nYes 0\n1\n"),
    ] {
        let case = Case::new(source);
        for options in [vec![], vec!["--no-opt"]] {
            let out = case.run_with_options("", &options);
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            assert_eq!(out.stdout, expected.as_bytes());
        }
    }
}

#[test]
fn vector_copy_and_assignment_guard_user_operations_and_displaced_cleanup() {
    let copying = r#"
extern type Score{cpp_name:"std::int64_t",}
struct Row{value:Score;other:Int;}
extern symbol watched{code:"struct RuskRecord0;Obj<RuskVec<RuskRecord0>> watched;",}
extern fn score()->Score{cpp_name:"score",code:"Obj<std::int64_t> score(Loc){return make_obj<std::int64_t>(1);}",}
extern fn watch(rows:Vec<Row>){cpp_name:"watch",code:"void watch(Obj<RuskVec<RuskRecord0>> rows,Loc){watched=rows;}",}
extern fn copy(x:Score)->Score{cpp_name:"copy_score",code:"Obj<std::int64_t> copy_score(Obj<std::int64_t> x,Loc){watched->clear();++watched->version;return make_obj<std::int64_t>(*x);}",}
let rows:=[Row{value:score(),other:2}];watch(rows);wt "before";let copied=rows;
"#;
    let assigning = r#"
extern symbol watched{code:"Obj<RuskVec<std::int64_t>> watched;",}
extern fn watch(rows:Vec<Int>){cpp_name:"watch",code:"void watch(Obj<RuskVec<std::int64_t>> rows,Loc){watched=rows;}",}
extern fn risky()->Int{cpp_name:"risky",code:"Obj<std::int64_t> risky(Loc){return Obj<std::int64_t>(new std::int64_t(1),[](auto* p){delete p;if(watched){watched->clear();++watched->version;}});}",}
let rows:=[risky(),2];watch(rows);wt "before";rows=[3,4];
"#;
    for (source, message) in [
        (copying, "container changed during copy"),
        (assigning, "container changed during assignment"),
    ] {
        let case = Case::new(source);
        for options in [vec![], vec!["--no-opt"]] {
            let out = case.run_with_options("", &options);
            assert!(!out.status.success());
            assert_eq!(out.stdout, b"before\n");
            let error = String::from_utf8_lossy(&out.stderr);
            assert!(
                error.contains("main.rusk:") && error.contains(message),
                "{error}"
            );
        }
    }
}

#[test]
fn option_operations_and_constructors_are_independent_generic_declarations() {
    let source = include_str!("../lib/collections.rusk");
    let mut declarations = source[source.find("extern fn Some").unwrap()..]
        .lines()
        .filter(|line| {
            !["effects:", "lower:"]
                .iter()
                .any(|p| line.trim_start().starts_with(p))
        })
        .collect::<Vec<_>>()
        .join("\n");
    for name in [
        "Some", "None", "Option", "is_some", "is_none", "__bool__", "__not__", "value", "try_read",
    ] {
        declarations =
            declarations.replace(&format!("fn {name}<"), &format!("fn migrated_{name}<"));
    }
    let case = Case::new(&format!(
        r#"{declarations}
let x:=3;let present:=migrated_Some(x);migrated_value(present)=8;
wt x,migrated_is_some(present),migrated_is_none(present),migrated___bool__(present),migrated___not__(present);
let absent:Option<Int>:=migrated_None();let empty:=migrated_Option::<Int>();wt migrated_is_none(absent),migrated_is_none(empty);
let array:Option<Vec<Int>>:=migrated_Some([]);wt migrated_value(array).len();
let pair:=migrated_try_read::<(Int,Str)>();wt migrated_value(pair);
struct Row{{value:Int;}}
fn __read__()->Row{{in n:Int;return Row{{value:n}};}}
let row:=migrated_try_read::<Row>();wt migrated_value(row).value;
fn __read__()->Unit{{in n:Int;}}
let unit:=migrated_try_read::<Unit>();wt migrated_is_some(unit),migrated_try_read::<Unit>().is_none();
"#
    ));
    for options in [vec![], vec!["--no-opt"]] {
        let out = case.run_with_options("4 hi 7 8", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            out.stdout,
            b"8 Yes No Yes No\nYes Yes\n0\n4 hi\n7\nYes Yes\n"
        );
    }
}

#[test]
fn option_unit_values_keep_identity_and_tokens_and_checked_access_locations() {
    let source = r#"
extern fn token(n:Int)->Unit{return_abi:"object",cpp_name:"token",code:"Obj<Unit> token(Obj<std::int64_t> n,Loc){return Obj<Unit>(new Unit{},[n](auto* p){++*n;delete p;});}",}
extern fn same(a:Unit,b:Unit)->Bool{cpp_name:"same",code:"Obj<bool> same(Obj<Unit> a,Obj<Unit> b,Loc){return make_obj<bool>(a==b);}",}
let count:=0;
{let original:=token(count);let option:=Some(original);let held:=option.value();wt same(original,held),count;option=None::<Unit>();wt same(original,held),count;}
wt count;
fn inspect(readonly noescape option:Option<Int>)->Int{return copy(option.value());}
wt inspect(Some(7));
"#;
    let case = Case::new(source);
    for options in [vec![], vec!["--no-opt"]] {
        let out = case.run_with_options("", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"Yes 0\nYes 0\n1\n7\n");
    }
    let escaped=Case::new(r#"
extern fn retain(x:Unit){cpp_name:"retain_unit",code:"Obj<Unit> saved_unit;void retain_unit(Obj<Unit> x,Loc){saved_unit=x;}",}
fn blocked(noescape option:Option<Unit>){retain(option.value());}
blocked(Some({}));
"#).compile();
    assert!(!escaped.status.success());
    assert!(String::from_utf8_lossy(&escaped.stderr).contains("noescape"));
    for source in [
        "wt \"before\";\nlet empty:=None::<Unit>();empty.value();",
        "wt \"before\";\nlet empty:=Option::<Int>();empty?!;",
    ] {
        let case = Case::new(source);
        for options in [vec![], vec!["--no-opt"]] {
            let out = case.run_with_options("", &options);
            assert!(!out.status.success());
            assert_eq!(out.stdout, b"before\n");
            let error = String::from_utf8_lossy(&out.stderr);
            assert!(
                error.contains("main.rusk:2:") && error.contains("absent Option value"),
                "{error}"
            );
        }
    }
}

#[test]
fn try_read_checks_eof_before_custom_and_structured_readers() {
    let custom = Case::new(
        r#"
fn __read__()->Unit{wt "reader";in n:Int;}
let first:=try_read::<Unit>();wt first.is_some();let second:=try_read::<Unit>();wt second.is_none();
"#,
    );
    let structured =
        Case::new("wt \"before\";\nlet pair:=try_read::<(Int,Str)>();wt pair.is_some();");
    for options in [vec![], vec!["--no-opt"]] {
        let out = custom.run_with_options("3", &options);
        assert!(out.status.success());
        assert_eq!(out.stdout, b"reader\nYes\nYes\n");
        let out = custom.execute("");
        assert!(out.status.success());
        assert_eq!(out.stdout, b"No\nYes\n");
        let out = structured.run_with_options("3 hi", &options);
        assert!(out.status.success());
        assert_eq!(out.stdout, b"before\nYes\n");
        let out = structured.execute("");
        assert!(out.status.success());
        assert_eq!(out.stdout, b"before\nNo\n");
        let out = structured.execute("3");
        assert!(!out.status.success());
        assert_eq!(out.stdout, b"before\n");
        let error = String::from_utf8_lossy(&out.stderr);
        assert!(
            error.contains("main.rusk:2:") && error.contains("unexpected end of input"),
            "{error}"
        );
    }
}

#[test]
fn derived_enum_copies_prove_independence_only_for_covered_payload_operations() {
    let case = Case::new(
        r#"
enum Choice{Pair(Int,Vec<Int>),Empty}
fn detached(readonly noescape value:Choice)->Choice{return copy(value);}
let values:=[1,2];let first:=Choice::Pair(3,values);let second:=detached(first);values[0]=9;
wt match second{Pair(n,v)=>(n,v[0]),Empty=>(0,0),};
fn optional(readonly noescape value:Option<Unit>)->Option<Unit>{return copy(value);}
extern fn same(a:Unit,b:Unit)->Bool{cpp_name:"same",code:"Obj<bool> same(Obj<Unit> a,Obj<Unit> b,Loc){return make_obj<bool>(a==b);}",}
let source:=Some({});let copied:=optional(source);wt same(source.value(),copied.value());
"#,
    );
    for options in [vec![], vec!["--no-opt"]] {
        let out = case.run_with_options("", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"3 1\nNo\n");
    }
    for copier in [
        "fn copy(x:Leaf)->Leaf{return x;}",
        r#"extern fn copy(x:Leaf)->Leaf{cpp_name:"opaque_copy",code:"Obj<std::int64_t> opaque_copy(Obj<std::int64_t> x,Loc){return make_obj<std::int64_t>(*x);}",}"#,
    ] {
        let source = format!(
            r#"
extern type Leaf{{cpp_name:"std::int64_t",}}
{copier}
enum Choice{{Value(Leaf),Empty}}
fn blocked(noescape input:Choice)->Choice{{return copy(input);}}
"#
        );
        let out = Case::new(&source).compile();
        assert!(!out.status.success());
        assert!(String::from_utf8_lossy(&out.stderr).contains("noescape"));
    }
}

#[test]
fn derived_enum_copy_pins_input_during_callbacks_and_keeps_the_payload_snapshot() {
    let source = r#"
extern type Leaf{cpp_name:"std::int64_t",}
enum Choice{Pair(Leaf,Int),Empty}
extern symbol observation{code:"std::function<void()> change_source;",}
extern fn leaf()->Leaf{cpp_name:"leaf",code:"Obj<std::int64_t> leaf(Loc){return make_obj<std::int64_t>(4);}",}
extern fn value(x:Leaf)->Int{cpp_name:"value_leaf",code:"Obj<std::int64_t> value_leaf(Obj<std::int64_t> x,Loc){return x;}",}
extern fn watch<F>(f:F){code:"void @{fn}(Obj<@{F}> f,Loc loc){change_source=[f,loc](){@{invoke F()->Unit}(f,loc);};}",}
extern fn unwatch(){cpp_name:"unwatch",code:"void unwatch(Loc){change_source={};}",}
extern fn copy(x:Leaf)->Leaf{cpp_name:"copy_leaf",code:"Obj<std::int64_t> copy_leaf(Obj<std::int64_t> x,Loc){if(change_source)change_source();return make_obj<std::int64_t>(*x);}",}
let source:=Choice::Pair(leaf(),7);watch(()=>{source=Choice::Empty;});let copied=source;unwatch();
wt match source{Pair(x,n)=>false,Empty=>true,};
wt match copied{Pair(x,n)=>(value(x),n),Empty=>(0,0),};
"#;
    let case = Case::new(source);
    for options in [vec![], vec!["--no-opt"]] {
        let out = case.run_with_options("", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"Yes\n4 7\n");
    }
}

#[test]
fn owned_enum_copy_holds_the_input_object_until_payload_callbacks_finish() {
    let source = r#"
extern type Leaf{cpp_name:"std::int64_t",}
enum Choice{Pair(Leaf,Int),Empty}
extern symbol observation{code:"std::function<void()> clear_source;std::int64_t dropped=0,during=-1;",}
extern fn leaf()->Leaf{cpp_name:"leaf",code:"Obj<std::int64_t> leaf(Loc){return make_obj<std::int64_t>(4);}",}
extern fn wrap<T>(x:T)->T{code:"Obj<@{T}> @{fn}(Obj<@{T}> x,Loc){return Obj<@{T}>(new @{T}(*x),[](auto* p){++dropped;delete p;});}",}
extern fn watch<T>(a:Vec<T>){code:"void @{fn}(Obj<@{Vec<T>}> a,Loc){clear_source=[a](){a->clear();++a->version;};}",}
extern fn clear_watch(){cpp_name:"clear_watch",code:"void clear_watch(Loc){clear_source={};}",}
extern fn copy(x:Leaf)->Leaf{cpp_name:"copy_leaf",code:"Obj<std::int64_t> copy_leaf(Obj<std::int64_t> x,Loc){if(clear_source)clear_source();during=dropped;return make_obj<std::int64_t>(*x);}",}
extern fn first_copy<T>(a:Vec<T>)->T{code:"Obj<@{T}> @{fn}(Obj<@{Vec<T>}> a,Loc loc){return @{owned_call copy(T)->T}((*a)[0],loc);}",}
extern fn counts()->(Int,Int){cpp_name:"counts",code:"Obj<RuskRecord0> counts(Loc){return make_obj<RuskRecord0>(RuskRecord0{make_obj<std::int64_t>(during),make_obj<std::int64_t>(dropped)});}",}
let rows:=[wrap(Choice::Pair(leaf(),7))];watch(rows);let copied:=first_copy(rows);clear_watch();
wt counts(),rows.len(),match copied{Pair(x,n)=>n,Empty=>0,};
"#;
    for source in [
        source.to_string(),
        source.replace(
            "return @{owned_call copy(T)->T}((*a)[0],loc);",
            "auto held=(*a)[0];return @{rooted_call copy(T)->T}(held,loc);",
        ),
    ] {
        let case = Case::new(&source);
        for options in [vec![], vec!["--no-opt"]] {
            let out = case.run_with_options("", &options);
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            assert_eq!(out.stdout, b"0 1 0 7\n");
        }
    }
}

#[test]
fn aggregate_assignments_use_independent_declarations_and_preserve_identity() {
    let library = include_str!("../lib/collections.rusk");
    let start = library.find("extern fn aggregate_assign_body<").unwrap();
    let end = library[start..]
        .find("// Standard Option operations")
        .unwrap()
        + start;
    let declarations = library[start..end]
        .lines()
        .filter(|line| !line.trim_start().starts_with("derive:"))
        .collect::<Vec<_>>()
        .join("\n")
        .replace("fn aggregate_assign_body<", "fn set_record<")
        .replace("fn sum_assign_body<", "fn set_enum<");
    let case = Case::new(&format!(
        r#"{declarations}
extern fn same<T>(a:T,b:T)->Bool {{code:"Obj<@{{Bool}}> @{{fn}}(Obj<@{{T}}> a,Obj<@{{T}}> b,Loc){{return make_obj<@{{Bool}}>(a==b);}}",}}
struct Row{{text:Char;unit:Unit;}}
let text:="ab";let unit:={{}};let source:=Row{{text:text[0],unit:unit}};let target:=Row{{text:'z',unit:{{}}}};let alias:=target;
set_record(target,source);wt same(target,alias),same(target.unit,unit),target.text;
source:=Row{{text:'x',unit:{{}}}};text+="!";wt text,target.text;
target=target;wt same(target,alias),target.text;
enum State{{Empty,Present(Char,Unit),}}
let value:=State::Present(text[1],unit);let destination:=State::Empty;let held:=destination;
set_enum(destination,value);value:=State::Empty;text+="?";
match destination{{Present(c,u)=>{{wt same(destination,held),same(u,unit),c,text;}},_=>{{}},}};
destination=destination;match destination{{Present(c,_)=>{{wt same(destination,held),c;}},_=>{{}},}};
enum Reversed<T>{{Missing,Present(T),}}
let n:=Reversed::Present(7);let m:=Reversed::Present(9);set_enum(n,m);m:=Reversed::Missing;
match n{{Present(v)=>{{wt v;}},_=>{{}},}};
"#
    ));
    for options in [vec![], vec!["--no-opt"]] {
        let out = case.run_with_options("", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            out.stdout,
            b"Yes No a\nab! a\nYes a\nYes No b ab!?\nYes b\n9\n"
        );
    }
}

#[test]
fn aggregate_assignment_cleanup_keeps_the_established_field_update_order() {
    let case = Case::new(
        r#"
extern symbol assignment_observer {code:"std::function<void()> assignment_observer;std::string assignment_log;",}
extern fn token()->Unit {
    return_abi:"object",cpp_name:"assignment_token",
    code:"Obj<Unit> assignment_token(Loc){return Obj<Unit>(new Unit{},[](Unit* p){if(assignment_observer)assignment_observer();delete p;});}",
}
extern fn watch<T>(value:T) {
    code:"void @{fn}(Obj<@{T}> value,Loc){assignment_observer=[weak=std::weak_ptr<@{T}>(value)](){if(auto owner=weak.lock())assignment_log=std::to_string(*owner->field_0)+\",\"+std::to_string(*owner->field_2);};}",
}
extern fn observed()->Str {
    cpp_name:"assignment_observed",code:"Obj<std::string> assignment_observed(Loc){return make_obj<std::string>(assignment_log);}",
}
struct Row{first:Int;token:Unit;last:Int;}
let target:=Row{first:1,token:token(),last:3};let source:=Row{first:7,token:{},last:9};
watch(target);target=source;wt observed(),target.first,target.last;
"#,
    );
    for options in [vec![], vec!["--no-opt"]] {
        let out = case.run_with_options("", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"7,3 7 9\n");
    }
}

#[test]
fn derived_assignment_keeps_unknown_effects_and_propagates_copy_failures() {
    for source in [
        "struct Row{n:Int;}fn blocked(noescape x:Row,y:Row){x=y;}blocked(Row{n:1},Row{n:2});",
        "enum E{Value(Int),Empty}fn blocked(noescape x:E,y:E){x=y;}blocked(E::Value(1),E::Empty);",
        "struct Row{n:Int;}fn blocked(readonly x:Row,y:Row){x=y;}blocked(Row{n:1},Row{n:2});",
    ] {
        let case = Case::new(source);
        let out = case.compile();
        assert!(!out.status.success());
        assert!(!case.0.join("main.cpp").exists());
    }
    let definitions = r#"
extern type Leaf {cpp_name:"std::int64_t",}
extern fn leaf()->Leaf {cpp_name:"assignment_leaf",code:"Obj<std::int64_t> assignment_leaf(Loc){return make_obj<std::int64_t>(1);}",}
extern fn copy(x:Leaf)->Leaf {cpp_name:"assignment_copy",code:"Obj<std::int64_t> assignment_copy(Obj<std::int64_t>,Loc loc){fail(loc,\"copy rejected\");}",}
"#;
    for operation in [
        "struct Row{n:Leaf;}let a:=Row{n:leaf()};let b:=Row{n:leaf()};wt \"before\";a=b;",
        "enum E{Value(Leaf),Empty}let a:=E::Empty;let b:=E::Value(leaf());wt \"before\";a=b;",
    ] {
        for options in [vec![], vec!["--no-opt"]] {
            let out =
                Case::new(&format!("{definitions}{operation}")).run_with_options("", &options);
            assert!(!out.status.success());
            assert_eq!(out.stdout, b"before\n");
            let error = String::from_utf8_lossy(&out.stderr);
            assert!(
                error.contains("main.rusk:") && error.contains("copy rejected"),
                "{error}"
            );
        }
    }
}

#[test]
fn unused_aggregate_copies_do_not_require_payload_copy_operations() {
    let definitions = r#"
extern type Handle{cpp_name:"std::int64_t",}
extern fn handle()->Handle{cpp_name:"handle",code:"Obj<std::int64_t> handle(Loc){return make_obj<std::int64_t>(7);}",}
extern fn number(x:Handle)->Int{cpp_name:"number",code:"Obj<std::int64_t> number(Obj<std::int64_t> x,Loc){return x;}",}
struct Inner{value:Handle;}
struct Outer{inner:Inner;}
enum Choice{Value(Outer),Empty}
"#;
    let case = Case::new(&format!(
        "{definitions}{}",
        r#"
let original:=Outer{inner:Inner{value:handle()}};let shared:=original;
wt number(shared.inner.value);
let pair:=(original,handle());wt number(pair[1]);
let value:=Choice::Value(original);match value{Value(x)=>{wt number(x.inner.value);},Empty=>{},};
"#
    ));
    for options in [vec![], vec!["--no-opt"]] {
        let out = case.run_with_options("", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"7\n7\n7\n");
    }
    for operation in [
        "let original:=Outer{inner:Inner{value:handle()}};let copied=original;",
        "let original:=Choice::Value(Outer{inner:Inner{value:handle()}});let copied=original;",
        "let a:=Outer{inner:Inner{value:handle()}};let b:=a;a=b;",
    ] {
        let case = Case::new(&format!("{definitions}{operation}"));
        let out = case.compile();
        assert!(!out.status.success());
        let error = String::from_utf8_lossy(&out.stderr);
        assert!(
            error.contains("copy") && error.contains("main.rusk:"),
            "{error}"
        );
        assert!(!case.0.join("main.cpp").exists());
    }
}

#[test]
fn product_readers_use_field_operations_under_independent_names() {
    let library = include_str!("../lib/collections.rusk");
    let start = library.find("extern fn product_read_body<").unwrap();
    let end = start
        + library[start..]
            .find("// Standard Option operations")
            .unwrap();
    let declaration = library[start..end]
        .lines()
        .filter(|line| !line.trim_start().starts_with("derive:"))
        .collect::<Vec<_>>()
        .join("\n")
        .replace("fn product_read_body<", "fn read_fields<");
    let case = Case::new(&format!(
        r#"{declaration}
extern type Handle{{cpp_name:"std::int64_t",}}
extern fn __read__()->Handle{{cpp_name:"read_handle",code:"Obj<std::int64_t> read_handle(Loc loc){{auto value=read_token(loc);return make_obj<std::int64_t>(std::stoll(value));}}",}}
extern fn number(x:Handle)->Int{{cpp_name:"handle_number",code:"Obj<std::int64_t> handle_number(Obj<std::int64_t> x,Loc){{return x;}}",}}
fn __read__()->Unit{{in ignored:Int;}}
struct Row{{id:Int;ignored:Unit;name:Str;}}
let row:=read_fields::<Row>();wt row.id,row.name;
in pair:(Handle,(Int,Str));wt number(pair[0]),pair[1];
in units:((Unit,Int),Str);wt units[0][1],units[1];
"#
    ));
    for options in [vec![], vec!["--no-opt"]] {
        let out = case.run_with_options("1 2 hello 3 4 world 5 6 end", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"1 hello\n3 4 world\n6 end\n");
    }
}

#[test]
fn tuple_readers_preserve_order_and_custom_reader_precedence() {
    let case = Case::new(
        r#"
extern symbol read_order {code:"std::int64_t input_order=0;",}
extern type Step{cpp_name:"std::int64_t",}
extern fn number(x:Step)->Int{cpp_name:"step_number",code:"Obj<std::int64_t> step_number(Obj<std::int64_t> x,Loc){return x;}",}
extern fn step(x:Int)->Step{cpp_name:"step",code:"Obj<std::int64_t> step(Obj<std::int64_t> x,Loc){return x;}",}
extern fn __read__()->Step {
    cpp_name:"ordered_integer",code:"Obj<std::int64_t> ordered_integer(Loc loc){auto value=read_token(loc);input_order=input_order*10+std::stoll(value);return make_obj<std::int64_t>(std::stoll(value));}",
}
extern fn order()->Int{cpp_name:"get_input_order",code:"Obj<std::int64_t> get_input_order(Loc){return make_obj<std::int64_t>(input_order);}",}
fn __read__()->(Step,Step){in value:Step;return (value,step(9));}
in nested:((Step,Step),Step,(Step,Step));wt number(nested[0][0]),number(nested[0][1]),number(nested[1]),number(nested[2][0]),number(nested[2][1]),order();
"#,
    );
    for options in [vec![], vec!["--no-opt"]] {
        let out = case.run_with_options("1 2 3", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"1 9 2 3 9 123\n");
    }
}

#[test]
fn tuple_reader_failures_keep_partial_output_and_user_locations() {
    for input in ["1", "1 not-an-integer"] {
        for options in [vec![], vec!["--no-opt"]] {
            let out = Case::new("wt \"before\";in value:(Int,(Int,Int));wt value;")
                .run_with_options(input, &options);
            assert!(!out.status.success());
            assert_eq!(out.stdout, b"before\n");
            assert!(String::from_utf8_lossy(&out.stderr).contains("main.rusk:"));
        }
    }
    let case = Case::new("extern type Handle{cpp_name:\"std::int64_t\",}in value:(Handle,Int);");
    let out = case.compile();
    assert!(!out.status.success());
    let error = String::from_utf8_lossy(&out.stderr);
    assert!(
        error.contains("no input reader") && error.contains("main.rusk:"),
        "{error}"
    );
    assert!(!case.0.join("main.cpp").exists());
}

#[test]
fn tuple_reader_effect_queries_validate_fields_without_consuming_input() {
    let query = r#"extern fn reader_writes<T>()->Bool{code:"Obj<@{Bool}> @{fn}(Loc){return make_obj<@{Bool}>(@{writes __read__()->T});}",}"#;
    let case = Case::new(&format!("{query}wt reader_writes::<(Int,Str)>();"));
    for options in [vec![], vec!["--no-opt"]] {
        let out = case.run_with_options("", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"Yes\n");
    }
    let case = Case::new(&format!(
        "{query}extern type Handle{{cpp_name:\"std::int64_t\",}}wt reader_writes::<(Handle,Int)>();"
    ));
    let out = case.compile();
    assert!(!out.status.success());
    let error = String::from_utf8_lossy(&out.stderr);
    assert!(
        error.contains("no input reader") && error.contains("main.rusk:"),
        "{error}"
    );
    assert!(!case.0.join("main.cpp").exists());
}

#[test]
fn multidimensional_input_preserves_length_snapshots_order_and_unit_objects() {
    let case = Case::new(include_str!("../examples/beta08/dimensions_input.rusk"));
    for options in [vec![], vec!["--no-opt"]] {
        let out = case.run_with_options("1 2 3 4 5 6 7 8 a b c d", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"0 1 2 3 1 4\n5 6 7 8\n0\n4\n");
    }
    let case = Case::new(
        "fn __read__()->Unit{in ignored:Int;}in units(2,2):Vec<Vec<Unit>>;wt units.len(),units[0].len();",
    );
    for options in [vec![], vec!["--no-opt"]] {
        let out = case.run_with_options("1 2 3 4", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"2 2\n");
    }
    let case = Case::new(
        r#"
fn __read__(n:Int,m:Int)->Vec<Vec<Int>> {wt "custom",n,m;return [[n+m]];}
in rows(2,3):Vec<Vec<Int>>,cube(2,4,5):Vec<Vec<Vec<Int>>>;
wt rows,cube;
"#,
    );
    for options in [vec![], vec!["--no-opt"]] {
        let out = case.run_with_options("", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"custom 2 3\ncustom 4 5\ncustom 4 5\n5 9 9\n");
    }
}

#[test]
fn multidimensional_reader_validation_precedes_input_and_keeps_caller_locations() {
    for source in [
        "in rows(2,3,4,5):Vec<Vec<Vec<Int>>>;",
        "in rows(2,3):Vec<Vec<Vec<Int>>>;",
    ] {
        let case = Case::new(source);
        let out = case.compile();
        assert!(!out.status.success());
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("main.rusk:"),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(!case.0.join("main.cpp").exists());
    }

    for (source, input, message) in [
        (
            "wt \"before\";in rows(0,-1):Vec<Vec<Int>>;",
            "bad",
            "negative array length",
        ),
        (
            "wt \"before\";in rows(0,0,-1):Vec<Vec<Vec<Int>>>;",
            "bad",
            "negative array length",
        ),
        (
            "wt \"before\";in rows(2,2):Vec<Vec<Int>>;",
            "1 2 3",
            "unexpected end of input",
        ),
        (
            "wt \"before\";in rows(2,2):Vec<Vec<Int>>;",
            "1 2 3 bad",
            "invalid integer input",
        ),
        (
            "wt \"before\";in rows(9223372036854775807,0):Vec<Vec<Int>>;",
            "",
            "array length out of range",
        ),
    ] {
        for options in [vec![], vec!["--no-opt"]] {
            let out = Case::new(source).run_with_options(input, &options);
            assert!(!out.status.success(), "{source}");
            assert_eq!(out.stdout, b"before\n");
            let error = String::from_utf8_lossy(&out.stderr);
            assert!(
                error.contains(message) && error.contains("main.rusk:"),
                "{source}: {error}"
            );
        }
    }
}

#[test]
fn dimensions_template_works_as_an_unrelated_fixed_arity_user_extern() {
    let source = include_str!("../lib/collections.rusk");
    let start = source.find("extern fn dimensions_read_body<").unwrap();
    let end = source[start..]
        .find("// Language-generated record/")
        .unwrap()
        + start;
    let declaration = source[start..end]
        .replace(
            "dimensions_read_body<T>()",
            "unrelated<T>(height:Int,width:Int)",
        )
        .replace("derive:\"vector_read\",", "");
    let case = Case::new(&format!("{declaration} wt unrelated::<Vec<Int>>(2,2);"));
    for options in [vec![], vec!["--no-opt"]] {
        let out = case.run_with_options("1 2 3 4", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"1 2 3 4\n");
    }
}

#[test]
fn tuple_output_resolves_logical_writers_and_preserves_unknown_cleanup_order() {
    let case = Case::new(include_str!(
        "../examples/beta08/tuple_output_lifetime.rusk"
    ));
    for options in [vec![], vec!["--no-opt"]] {
        let out = case.run_with_options("", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"a b\nab!\n1:1 8:2\n3:0 4:0\n2\n");
    }
}

#[test]
fn tuple_output_keeps_user_tuple_writers_and_checks_effects_and_missing_fields() {
    let source = r#"
struct Row{value:Int;}
fn __write__(pair:(Row,Row)){__write__(pair[0].value+pair[1].value);}
wt (Row{value:2},Row{value:3});
struct Other{value:Int;}
fn __write__<T>(pair:(T,Other)){__write__(pair[1].value);}
wt (Row{value:4},Other{value:5});
"#;
    for options in [vec![], vec!["--no-opt"]] {
        let out = Case::new(source).run_with_options("", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"5\n5\n");
    }
    for restriction in ["readonly", "noescape"] {
        let source = format!(
            r#"
struct Row{{value:Int;}}
extern fn __write__(row:Row){{cpp_name:"opaque_tuple_field",code:"void opaque_tuple_field(Obj<RuskRecord0>,Loc){{}}",}}
fn show({restriction} value:(Row,Row)){{wt value;}}
show((Row{{value:1}},Row{{value:2}}));
"#
        );
        let out = Case::new(&source).compile();
        assert!(!out.status.success());
        assert!(String::from_utf8_lossy(&out.stderr).contains(restriction));
    }
    let case = Case::new("struct NoWriter{value:Int;}wt (1,NoWriter{value:2});");
    let out = case.compile();
    assert!(!out.status.success());
    let error = String::from_utf8_lossy(&out.stderr);
    assert!(
        error.contains("main.rusk:") && error.contains("__write__"),
        "{error}"
    );
    assert!(!case.0.join("main.cpp").exists());
}

#[test]
fn tuple_output_supports_nested_logical_types_and_readonly_vectors() {
    let source = r#"
extern type Score{cpp_name:"std::int64_t",}
extern fn score(x:Int)->Score{cpp_name:"tuple_score",code:"Obj<std::int64_t> tuple_score(Obj<std::int64_t> x,Loc){return x;}",}
extern fn __write__(x:Score){cpp_name:"write_tuple_score",code:"void write_tuple_score(Obj<std::int64_t> x,Loc){rusk_write(*x+100);}",}
wt ((1,score(2)),3);
struct Row{ch:Char;}
fn __write__(row:Row){__write__(row.ch);}
fn show(readonly noescape values:Vec<(Row,Row)>){wt values;}
let text:="ab";{let rows:=[(Row{ch:text[0]},Row{ch:text[1]})];show(rows);}
text.push_back('!');wt text;
"#;
    let case = Case::new(source);
    for options in [vec![], vec!["--no-opt"]] {
        let out = case.run_with_options("", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"1 102 3\na b\nab!\n");
    }
}

#[test]
fn tuple_output_template_works_as_an_unrelated_user_extern() {
    let source = include_str!("../lib/collections.rusk");
    let start = source.find("extern fn product_write_body<").unwrap();
    let end = source[start..]
        .find("// Preserve the fixed argument ABI")
        .unwrap()
        + start;
    let declaration = source[start..end]
        .lines()
        .filter(|line| {
            !line.trim_start().starts_with("derive:")
                && !line.trim_start().starts_with("effects:")
                && !line.trim_start().starts_with("lower:")
        })
        .collect::<Vec<_>>()
        .join("\n")
        .replace("product_write_body", "unrelated_output");
    let case = Case::new(&format!(
        r#"{declaration}
struct Row{{value:Int;}}
fn __write__(value:Row){{__write__(value.value+1);}}
unrelated_output((Row{{value:2}},[3,4]));wt "";
"#
    ));
    for options in [vec![], vec!["--no-opt"]] {
        let out = case.run_with_options("", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"3 3 4\n");
    }
}

#[test]
fn tuple_comparisons_use_logical_fields_and_preserve_boolean_sequence_order() {
    let case = Case::new(include_str!("../examples/beta08/tuple_comparisons.rusk"));
    for options in [vec![], vec!["--no-opt"]] {
        let out = case.run_with_options("", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            out.stdout,
            b"No Yes Yes Yes No No\nYes Yes Yes\nYes\nYes\nNo No Yes Yes 2\n1 2\n9 No\n"
        );
    }
}

#[test]
fn tuple_comparisons_hold_fields_and_roots_across_unknown_operations() {
    let case = Case::new(include_str!(
        "../examples/beta08/tuple_comparison_lifetime.rusk"
    ));
    for options in [vec![], vec!["--no-opt"]] {
        let out = case.run_with_options("", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"1,1;1,1;30,40;Yes 4\n1,1;1,1;2,3;Yes 0\n4\n");
    }
}

#[test]
fn tuple_comparisons_preserve_user_overloads_and_reject_unknown_restrictions() {
    let source = r#"
struct Row{value:Int;}
fn __lt__(a:(Row,Row),b:(Row,Row))->Bool{return a[1].value>b[1].value;}
fn __eq__<T>(a:(T,Row),b:(T,Row))->Bool{return a[1].value==b[1].value;}
wt (Row{value:1},Row{value:3})<(Row{value:9},Row{value:2});
wt (1,Row{value:2})==(7,Row{value:2});
"#;
    for options in [vec![], vec!["--no-opt"]] {
        let out = Case::new(source).run_with_options("", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"Yes\nYes\n");
    }
    for restriction in ["readonly", "noescape"] {
        let source = format!(
            r#"
struct Row{{value:Int;}}
extern fn __lt__(a:Row,b:Row)->Bool{{cpp_name:"opaque_tuple_compare",code:"Obj<bool> opaque_tuple_compare(Obj<RuskRecord0>,Obj<RuskRecord0>,Loc){{return make_obj<bool>(false);}}",}}
fn check({restriction} a:(Row,Row),b:(Row,Row))->Bool{{return a<b;}}
check((Row{{value:1}},Row{{value:2}}),(Row{{value:3}},Row{{value:4}}));
"#
        );
        let out = Case::new(&source).compile();
        assert!(!out.status.success());
        assert!(
            String::from_utf8_lossy(&out.stderr).contains(restriction),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    for source in [
        "struct NoOrder{value:Int;}wt (1,NoOrder{value:2})<(1,NoOrder{value:3});",
        "struct WrongOrder{value:Int;}fn __lt__(a:WrongOrder,b:WrongOrder)->Int{return 1;}wt (1,WrongOrder{value:2})<(1,WrongOrder{value:3});",
        "wt true<false;",
    ] {
        let case = Case::new(source);
        let out = case.compile();
        assert!(!out.status.success());
        assert!(String::from_utf8_lossy(&out.stderr).contains("main.rusk:"));
        assert!(!case.0.join("main.cpp").exists());
    }
}

#[test]
fn tuple_comparison_templates_are_usable_under_unrelated_user_names() {
    let source = include_str!("../lib/collections.rusk");
    let equal = source.find("extern fn product_equal_body<").unwrap();
    let less = source.find("extern fn product_less_body<").unwrap();
    let end = source.find("extern fn product_not_equal_body<").unwrap();
    let declarations = source[equal..end]
        .lines()
        .filter(|line| {
            !line.trim_start().starts_with("derive:")
                && !line.trim_start().starts_with("effects:")
                && !line.trim_start().starts_with("lower:")
        })
        .collect::<Vec<_>>()
        .join("\n")
        .replace("product_equal_body", "user_equal")
        .replace("product_less_body", "user_less");
    assert!(less > equal);
    let case = Case::new(&format!(
        "{declarations} wt user_equal((1,2),(1,2)),user_less((false,[false,true]),(false,[true,false]));"
    ));
    for options in [vec![], vec!["--no-opt"]] {
        let out = case.run_with_options("", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"Yes Yes\n");
    }
}

#[test]
fn hash_collection_declarations_keep_context_defaults_copy_and_value_lifetimes() {
    for (source, expected) in [
        (
            include_str!("../examples/beta08/hash_collection_operations.rusk"),
            "2 No No Yes Yes 7\nempty some\n3 4 1\n5\nYes No\nYes No\nYes\nYes No 0 Yes\nZb\nZb!\n",
        ),
        (
            include_str!("../examples/beta08/dictionary_unit_values.rusk"),
            "Yes Yes Yes\nNo 2\n2 Yes\n",
        ),
    ] {
        let case = Case::new(source);
        for options in [vec![], vec!["--no-opt"]] {
            let out = case.run_with_options("", &options);
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            assert_eq!(out.stdout, expected.as_bytes());
        }
    }
}

#[test]
fn hash_collection_operations_work_with_unrelated_user_type_and_constructor_names() {
    let declarations = include_str!("../lib/hash_collections.rusk")
        .split("\n}\n")
        .filter(|part| !part.contains("extern fn hash_default_value<"))
        .collect::<Vec<_>>()
        .join("\n}\n")
        .lines()
        .filter(|line| {
            let line = line.trim_start();
            !line.starts_with("effects:")
                && !line.starts_with("representation:")
                && !line.starts_with("retains:")
                && !line.starts_with("aliases:")
                && !line.starts_with("lower_")
                && !line.starts_with("lower:")
        })
        .collect::<Vec<_>>()
        .join("\n")
        .replace("Dict", "Store")
        .replace("Set", "Members")
        .replace("RuskStore", "RuskDict")
        .replace("RuskMembers", "RuskSet")
        .replace("__hash_key__", "store_key_hash")
        .replace("__equal_key__", "store_key_equal");
    let case = Case::new(&format!(
        r#"
{declarations}
let store:Store<Str,Option<Int>>:=Store();store["empty"]:=None;store.set("some",Some(3));
wt store.get("empty")?!.is_none(),store["some"]?!,store.len();
let keys:=store.keys();keys.sort();wt keys;
let vectors:Store<Int,Vec<Int>>:=Store();vectors[1].push_back(8);wt vectors[1];
let members:Members<Str>:=Members();wt members.insert("hello"),members.get("hello")?!;
wt store.map((p:(Str,Option<Int>))=>p[1]??0).fold(0,(a:Int,b:Int)=>a+b);
wt members.map((s:Str)=>s.len()).fold(0,(a:Int,b:Int)=>a+b);
wt members.erase("hello"),!members;
let saved=store;store["some"]:=Some(9);wt saved["some"]?!,store["some"]?!;
let alias:=members;let replacement:=Members::<Str>();replacement.insert("new");
members=replacement;replacement.clear();wt alias.contains("new"),replacement.contains("new");
let tuples:=Store::<(Int,(Str,Bool)),Int>();tuples.set((1,("x",true)),8);wt tuples.get((1,("x",true)))?!;
"#
    ));
    for options in [vec![], vec!["--no-opt"]] {
        let out = case.run_with_options("", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            out.stdout,
            b"Yes 3 2\nempty some\n8\nYes hello\n3\n5\nYes Yes\n3 9\nYes No\n8\n"
        );
    }
}

#[test]
fn hash_collection_size_queries_are_readonly_noescape_and_default_order_is_preserved() {
    let source = r#"
fn size(readonly noescape d:Dict<Str,Vec<Int>>)->Int{return d.len();}
fn empty(readonly noescape s:Set<Int>)->Bool{return !s&&s.is_empty();}
let d:=Dict::<Str,Vec<Int>>();d.set("x",[7]);wt size(d),empty(Set::<Int>());
extern type Value{cpp_name:"std::int64_t",}
extern symbol default_state{code:"Obj<std::string> original_key;",}
extern fn watch(key:Str){cpp_name:"watch_default_key",code:"void watch_default_key(Obj<std::string> key,Loc){original_key=key;}",}
extern fn Value()->Value{cpp_name:"make_default_value",code:"Obj<std::int64_t> make_default_value(Loc){*original_key=\"changed\";return make_obj<std::int64_t>(9);}",}
extern fn copy(value:Value)->Value{cpp_name:"copy_default_value",code:"Obj<std::int64_t> copy_default_value(Obj<std::int64_t> value,Loc){return make_obj<std::int64_t>(*value);}",}
extern fn number(value:Value)->Int{cpp_name:"default_value_number",code:"Obj<std::int64_t> default_value_number(Obj<std::int64_t> value,Loc){return value;}",}
let values:=Dict::<Str,Value>();let key:="before";watch(key);wt number(values[key]),key;
wt values.contains("before"),values.contains("changed"),values.len();
"#;
    for options in [vec![], vec!["--no-opt"]] {
        let out = Case::new(source).run_with_options("", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"1 Yes\n9 changed\nYes No 1\n");
    }
}

#[test]
fn indexed_protocols_use_normal_argument_and_result_contexts() {
    let source = r#"
extern type Slot{cpp_name:"std::int64_t",}
extern fn slot()->Slot{cpp_name:"context_slot",code:"Obj<std::int64_t> context_slot(Loc){return make_obj<std::int64_t>(3);}",}
extern fn __getitem__<T>(slot:Slot,index:Int)->T{
    code:"Obj<@{T}> @{fn}(Obj<@{Slot}> slot,Obj<@{Int}> index,Loc){return make_obj<@{T}>(static_cast<@{T}>(*slot+*index));}",
}

let value:=slot();let number:Int:=value[4];let flag:Bool:=value[-3];wt number,flag;
"#;
    for options in [vec![], vec!["--no-opt"]] {
        let out = Case::new(source).run_with_options("", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"7 No\n");
    }
}

#[test]
fn hash_iterators_preserve_keys_values_positions_and_exhaustion() {
    let case = Case::new(include_str!(
        "../examples/beta08/hash_collection_iterators.rusk"
    ));
    for options in [vec![], vec!["--no-opt"]] {
        let out = case.run_with_options("", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            out.stdout,
            b"key 5 Yes No\nNo No\nNo\nitem Yes No\nNo\nNo\n2 9\n2 9\n1 8\n"
        );
    }
}

#[test]
fn hash_iterator_copy_checks_mutation_before_copying_saved_positions() {
    for (setup, element) in [
        ("let owner:=Dict::<Int,Int>();owner.set(1,2);", "(Int,Int)"),
        ("let owner:=Set::<Int>();owner.insert(1);", "Int"),
    ] {
        for operation in [
            "let cursor:=__iter__(owner);owner.clear();let copied=cursor;",
            "let cursor:=owner.map((x:E)=>x);owner.clear();let copied=cursor;",
            "let cursor:=owner.filter((x:E)=>true);owner.clear();let copied=cursor;",
            "let cursor:=__iter__(owner);owner.clear();wt __iter_has_next__(cursor);",
            "let cursor:=__iter__(owner);owner.clear();let value:=__iter_next__(cursor);",
        ] {
            for options in [vec![], vec!["--no-opt"]] {
                let operation = operation.replace(":E", &format!(":{element}"));
                let source = format!("{setup}wt \"before\";{operation}");
                let out = Case::new(&source).run_with_options("", &options);
                assert!(!out.status.success(), "{source}");
                assert_eq!(out.stdout, b"before\n");
                let error = String::from_utf8_lossy(&out.stderr);
                assert!(
                    error.contains("main.rusk:")
                        && error.contains("container changed during iteration"),
                    "{source}\n{error}"
                );
            }
        }
    }
}

#[test]
fn lazy_iterator_copy_uses_declared_vector_and_range_state_copies() {
    let case = Case::new(include_str!(
        "../examples/beta08/iterator_state_copies.rusk"
    ));
    for options in [vec![], vec!["--no-opt"]] {
        let out = case.run_with_options("", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"3\n7 7\n8\n0\n2 4 6 2 4 6\n");
    }
}

#[test]
fn hash_copies_and_assignments_preserve_deep_values_outer_aliases_and_unit() {
    let case = Case::new(include_str!(
        "../examples/beta08/hash_collection_copies.rusk"
    ));
    for options in [vec![], vec!["--no-opt"]] {
        let out = case.run_with_options("", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"a zb\nzb!\n8 7 3\n8\nYes Yes\nYes No\nNo\n1\n");
    }
}

#[test]
fn hash_types_only_require_value_copy_when_copy_is_requested() {
    let source = r#"
extern type Token{cpp_name:"std::int64_t",}
extern fn token()->Token{cpp_name:"uncopyable_token",code:"Obj<std::int64_t> uncopyable_token(Loc){return make_obj<std::int64_t>(7);}",}
extern fn number(x:Token)->Int{cpp_name:"token_number",code:"Obj<std::int64_t> token_number(Obj<std::int64_t> x,Loc){return x;}",}
let d:=Dict::<Int,Token>();d.set(1,token());wt d.len(),number(d.get(1)?!);
"#;
    for options in [vec![], vec!["--no-opt"]] {
        let out = Case::new(source).run_with_options("", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"1 7\n");
    }
    let out = Case::new(&format!("{source}let duplicate=d;")).compile();
    assert!(!out.status.success());
    let error = String::from_utf8_lossy(&out.stderr);
    assert!(
        error.contains("main.rusk:") && error.contains("copy"),
        "{error}"
    );
    let sharing = format!(
        r#"{source}
extern fn copy(x:Token)->Token{{cpp_name:"shared_token_copy",code:"Obj<std::int64_t> shared_token_copy(Obj<std::int64_t> x,Loc){{return x;}}",}}
let duplicate=d;let shared:=number(duplicate.get(1)?!);shared=9;wt number(d.get(1)?!);
"#
    );
    for options in [vec![], vec!["--no-opt"]] {
        let out = Case::new(&sharing).run_with_options("", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"1 7\n9\n");
    }
}

#[test]
fn hash_copy_readonly_contract_depends_on_actual_element_copies() {
    let source = r#"
fn clone(readonly noescape source:Dict<Int,Vec<Int>>)->Dict<Int,Vec<Int>>{return copy(source);}
let d:=Dict::<Int,Vec<Int>>();d.set(1,[4]);let independent:=clone(d);
independent.get(1)?![0]=9;wt d.get(1)?!,independent.get(1)?!;
"#;
    for options in [vec![], vec!["--no-opt"]] {
        let out = Case::new(source).run_with_options("", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"4 9\n");
    }
    let source = r#"
extern type Token{cpp_name:"std::int64_t",}
extern fn copy(x:Token)->Token{cpp_name:"opaque_token_copy",code:"Obj<std::int64_t> opaque_token_copy(Obj<std::int64_t> x,Loc){return x;}",}
fn clone(readonly noescape source:Dict<Int,Token>)->Dict<Int,Token>{return copy(source);}
"#;
    let out = Case::new(source).compile();
    assert!(!out.status.success());
    let error = String::from_utf8_lossy(&out.stderr);
    assert!(
        error.contains("readonly") || error.contains("noescape"),
        "{error}"
    );
}

#[test]
fn hash_copy_guards_user_calls_and_deferred_source_value_cleanup() {
    for operation in [
        "let duplicate=d;",
        "let target:=Dict::<Int,Leaf>();target=d;",
    ] {
        let source = include_str!("../examples/beta08/hash_copy_invalidation.rusk")
            .replace("let duplicate=d;", operation);
        for input in ["No", "Yes"] {
            for options in [vec![], vec!["--no-opt"]] {
                let out = Case::new(&source).run_with_options(input, &options);
                assert!(!out.status.success());
                assert_eq!(out.stdout, b"before\n");
                let error = String::from_utf8_lossy(&out.stderr);
                assert!(
                    error.contains("main.rusk:") && error.contains("container changed during copy"),
                    "{source}\n{error}"
                );
            }
        }
    }
}

#[test]
fn hash_assignment_publishes_snapshot_before_cleanup_and_checks_reentry() {
    let case = Case::new(include_str!(
        "../examples/beta08/hash_assignment_lifetime.rusk"
    ));
    for mutate in [false, true] {
        for options in [vec![], vec!["--no-opt"]] {
            let out = case.run_with_options(if mutate { "Yes" } else { "No" }, &options);
            if mutate {
                assert!(!out.status.success());
                assert_eq!(out.stdout, b"before\n");
                let error = String::from_utf8_lossy(&out.stderr);
                assert!(
                    error.contains("main.rusk:")
                        && error.contains("container changed during assignment"),
                    "{error}"
                );
            } else {
                assert!(
                    out.status.success(),
                    "{}",
                    String::from_utf8_lossy(&out.stderr)
                );
                assert_eq!(out.stdout, b"before\n2 2 2\n");
            }
        }
    }
}

#[test]
fn hash_policies_use_logical_keys_and_preserve_basic_and_nested_key_behavior() {
    let case = Case::new(include_str!("../examples/beta08/hash_key_policies.rusk"));
    for options in [vec![], vec!["--no-opt"]] {
        let out = case.run_with_options("", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"3 4 5 6 0\n2 Yes Yes\n7 8\nYes 9\n1 0\n");
    }
}

#[test]
fn hash_key_eligibility_uses_rusk_types_even_when_native_hashes_exist() {
    for key in ["Opaque", "Tuple<Int,Opaque>", "Row", "Vec<Int>", "Unit"] {
        let source = format!(
            r#"
extern type Opaque{{cpp_name:"std::int64_t",}}
extern fn __hash_key__(value:Opaque)->Int{{cpp_name:"opaque_hash",code:"Obj<std::int64_t> opaque_hash(Obj<std::int64_t> value,Loc){{return value;}}",}}
extern fn __equal_key__(a:Opaque,b:Opaque)->Bool{{cpp_name:"opaque_equal",code:"Obj<bool> opaque_equal(Obj<std::int64_t> a,Obj<std::int64_t> b,Loc){{return make_obj<bool>(*a==*b);}}",}}
struct Row{{value:Int;}}
let unused:Dict<{key},Int>;
"#
        );
        let out = Case::new(&source).compile();
        assert!(!out.status.success(), "{key}");
        let error = String::from_utf8_lossy(&out.stderr);
        assert!(
            error.contains("native tuple fields require a Tuple") && error.contains("main.rusk:"),
            "{error}"
        );
    }
}

#[test]
fn hash_constructor_functions_allow_ordinary_user_overloads() {
    let source = "fn Dict(value:Int)->Int{return value+1;}fn Set(value:Int)->Int{return value+2;}let dictionary:=Dict::<Int,Int>();let members:=Set::<Int>();wt Dict(4),Set(4),dictionary.len(),members.len();";
    for options in [vec![], vec!["--no-opt"]] {
        let out = Case::new(source).run_with_options("", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"5 6 0 0\n");
    }
    for source in [
        "struct Dict{value:Int;}",
        "enum Set{Empty}",
        "extern type<K,V> Dict{cpp_name:\"std::int64_t\",}",
    ] {
        let out = Case::new(source).compile();
        assert!(!out.status.success());
        assert!(String::from_utf8_lossy(&out.stderr).contains("duplicate type"));
    }
}

#[test]
fn lazy_declarations_preserve_cached_values_captures_assignment_and_unit_inputs() {
    let case = Case::new(include_str!("../examples/beta08/lazy_iterators.rusk"));
    for options in [vec![], vec!["--no-opt"]] {
        let out = case.run_with_options("", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            out.stdout,
            b"0\nYes 2\nYes 2\n2 2 2\n4 4\n4 4\n2 2 4\n6 8 6 8\na ab!\nYes\n"
        );
    }
}

#[test]
fn lazy_algorithms_and_types_work_under_unrelated_user_names() {
    let declarations = include_str!("../lib/lazy_iterators.rusk")
        .split("\n}\n")
        .filter(|part| !part.contains("extern symbol"))
        .collect::<Vec<_>>()
        .join("\n}\n")
        .lines()
        .filter(|line| {
            let line = line.trim_start();
            !line.starts_with("representation:")
                && !line.starts_with("lower_")
                && !line.starts_with("retains:")
                && !line.starts_with("aliases:")
                && !line.starts_with("effects:")
        })
        .collect::<Vec<_>>()
        .join("\n")
        .replace("Mapped", "Transform")
        .replace("Filtered", "Select")
        .replace("extern fn map<", "extern fn convert_values<")
        .replace("extern fn filter<", "extern fn pick_values<");
    let source = format!(
        r#"{declarations}
let source:=[1,2,3,4];let visits:=0;
let filtered:=pick_values(source,(x:Int)=>{{visits+=1;x%2==0}});
let mapped:=convert_values(filtered,(x:Int)=>x*3);let snapshot=mapped;
wt visits,mapped.to_vec(),visits,snapshot.to_vec(),visits;
"#
    );
    for options in [vec![], vec!["--no-opt"]] {
        let out = Case::new(&source).run_with_options("", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"0 6 12 4 6 12 4\n");
    }
}

#[test]
fn lazy_creation_does_not_request_unused_value_copies_and_rejects_unit_results() {
    let source = r#"
extern type Token{cpp_name:"std::int64_t",}
extern fn token()->Token{cpp_name:"lazy_token",code:"Obj<std::int64_t> lazy_token(Loc){return make_obj<std::int64_t>(7);}",}
extern fn number(x:Token)->Int{cpp_name:"lazy_token_number",code:"Obj<std::int64_t> lazy_token_number(Obj<std::int64_t> x,Loc){return x;}",}
let values:=[token()];let mapped:=values.map((x:Token)=>x);
wt number(mapped.to_vec()[0]);
"#;
    for options in [vec![], vec!["--no-opt"]] {
        let out = Case::new(source).run_with_options("", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"7\n");
    }
    for source in [
        "let unused:=[1].map((x:Int)=>{});",
        "let values:Vec<Unit>:=[];let unused:=values.filter((value:Unit)=>true);",
    ] {
        let out = Case::new(source).compile();
        assert!(!out.status.success());
        let error = String::from_utf8_lossy(&out.stderr);
        assert!(
            error.contains("native nonunit reference") && error.contains("main.rusk:"),
            "{error}"
        );
    }
}

#[test]
fn iterator_copies_keep_owner_restrictions_without_retaining_them_outside_the_result() {
    let source = include_str!("../examples/beta08/iterator_shared_owners.rusk");
    for options in [vec![], vec!["--no-opt"]] {
        let out = Case::new(source).run_with_options("", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"2\n0\n0\n7 7\n2 9\n");
    }
    for source in [
        "fn leak(noescape owner:Vec<Int>)->Iter<Vec<Int>>{return copy(__iter__(owner));}leak([1]);",
        "fn leak(noescape owner:Dict<Int,Int>)->Iter<Dict<Int,Int>>{return copy(__iter__(owner));}leak(Dict::<Int,Int>());",
        "fn write(readonly owner:Vec<Int>){let it:=__iter__(owner);let saved=it;__iter_next__(saved)=9;}write([1]);",
    ] {
        let out = Case::new(source).compile();
        let error = String::from_utf8_lossy(&out.stderr);
        assert!(
            !out.status.success() && (error.contains("noescape") || error.contains("readonly")),
            "{error}"
        );
    }
}
