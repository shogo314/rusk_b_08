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
            !["effects:", "lower:", "lower_owned:", "lower_borrowed:"]
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
