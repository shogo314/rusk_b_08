mod support;
use support::Case;

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
    ] {
        declarations =
            declarations.replace(&format!("fn {name}<"), &format!("fn migrated_{name}<"));
    }
    let case = Case::new(&format!(
        r#"{declarations}
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
            b"0 No Yes No\n4\n8 3 1\n3 1 9 1\n6 1\n2 4\n6 1\n1\n6\n7 7\n5 2 2\n"
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
