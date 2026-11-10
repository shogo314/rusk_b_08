mod support;
use support::Case;

#[test]
fn native_type_patterns_bind_nested_arguments_and_preserve_disjoint_concrete_types() {
    let case = Case::new(
        r#"
extern type<T> Slot<Vec<T>> {cpp_name:"std::shared_ptr<{T}>",}
extern type Slot<Int> {cpp_name:"std::int64_t",}
extern type<A,B> PairCell<Tuple<A,B>> {cpp_name:"std::pair<Obj<{A}>,Obj<{B}>>",}
extern fn cell<T>(x:T)->Slot<Vec<T>> {
    code:"Obj<@{Slot<Vec<T>>}> @{fn}(Obj<@{T}> x,Loc){return make_obj<@{Slot<Vec<T>>}>(x);}",
}
extern fn value<T>(x:Slot<Vec<T>>)->T {
    code:"Obj<@{T}> @{fn}(Obj<@{Slot<Vec<T>>}> x,Loc){return *x;}",
}
extern fn pair<A,B>(a:A,b:B)->PairCell<Tuple<A,B>> {
    code:"Obj<@{PairCell<Tuple<A,B>>}> @{fn}(Obj<@{A}> a,Obj<@{B}> b,Loc){return make_obj<@{PairCell<Tuple<A,B>>}>(@{PairCell<Tuple<A,B>>}{a,b});}",
}
extern fn first<A,B>(x:PairCell<Tuple<A,B>>)->A {
    code:"Obj<@{A}> @{fn}(Obj<@{PairCell<Tuple<A,B>>}> x,Loc){return x->first;}",
}
extern fn number(x:Int)->Slot<Int> {cpp_name:"make_number",code:"Obj<std::int64_t> make_number(Obj<std::int64_t> x,Loc){return x;}",}
extern fn number_value(x:Slot<Int>)->Int {cpp_name:"number_value",code:"Obj<std::int64_t> number_value(Obj<std::int64_t> x,Loc){return x;}",}
let original:=3;let held:=cell(original);value(held)=9;
wt original,value(cell("ok")),first(pair(7,"pair")),number_value(number(11));
extern type Iter<Int>{cpp_name:"std::int64_t",}
for x in [1,2]{wt x;}
"#,
    );
    for options in [vec![], vec!["--no-opt"]] {
        let out = case.run_with_options("", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"9 ok 7 11\n1\n2\n");
    }
}

#[test]
fn native_type_patterns_reject_finite_overlap_in_both_orders() {
    for (a, b) in [
        (
            r#"extern type<T> Family<Vec<T>>{cpp_name:"{T}",}"#,
            r#"extern type<U> Family<Vec<U>>{cpp_name:"{U}",}"#,
        ),
        (
            r#"extern type<T> Family<Vec<T>>{cpp_name:"{T}",}"#,
            r#"extern type Family<Vec<Int>>{cpp_name:"std::int64_t",}"#,
        ),
        (
            r#"extern type<T> Family<T,Int>{cpp_name:"{T}",}"#,
            r#"extern type<T> Family<Str,T>{cpp_name:"{T}",}"#,
        ),
        (
            r#"extern type<T> Family<T,T,T>{cpp_name:"{T}",}"#,
            r#"extern type<A,B> Family<A,B,A>{cpp_name:"{A}",}"#,
        ),
    ] {
        for source in [format!("{a}\n{b}"), format!("{b}\n{a}")] {
            let out = Case::new(&source).compile();
            assert!(!out.status.success());
            let error = String::from_utf8_lossy(&out.stderr);
            assert!(error.contains("duplicate type family 'Family'"), "{error}");
        }
    }
    let out = Case::new(
        r#"
extern type<T> Finite<T,Vec<T>>{cpp_name:"{T}",}
extern type<U> Finite<Vec<U>,U>{cpp_name:"{U}",}
extern type<T> Repeated<T,T>{cpp_name:"{T}",}
extern type Repeated<Int,Str>{cpp_name:"std::int64_t",}
wt 1;
"#,
    )
    .run("");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.stdout, b"1\n");
}

#[test]
fn invalid_and_expensive_native_type_patterns_fail_before_cpp_output() {
    for (source, expected) in [
        (
            r#"extern type<A,B> Bad<Vec<A>>{cpp_name:"{A}",}"#,
            "not bound",
        ),
        (
            r#"extern type<T> Bad<Missing<T>>{cpp_name:"{T}",}"#,
            "unknown type constructor",
        ),
        (
            r#"extern type T<Int>{cpp_name:"std::int64_t",} extern type<T> Bad<T<Int>>{cpp_name:"{T}",}"#,
            "not bound",
        ),
        (
            r#"extern type<A,B> Bad<Tuple<A,B>>{cpp_name:"{A}",} fn unused(x:Bad<Int,Int>){}"#,
            "expects 1 type arguments",
        ),
        (
            r#"extern type<T> Bad<Vec<T>>{cpp_name:"{T}",} fn unused(x:Bad<Vec<UnknownElement>>){}"#,
            "unknown type",
        ),
    ] {
        let out = Case::new(source).compile();
        assert!(!out.status.success());
        let error = String::from_utf8_lossy(&out.stderr);
        assert!(error.contains(expected), "{error}");
    }
    let a = vec!["T"; 3000].join(",");
    let b = vec!["U"; 3000].join(",");
    let out=Case::new(&format!("extern type<T> Wide<{a}>{{cpp_name:\"{{T}}\",}} extern type<U> Wide<{b}>{{cpp_name:\"{{U}}\",}} ")).compile();
    assert!(!out.status.success());
    let error = String::from_utf8_lossy(&out.stderr);
    assert!(
        error.contains("native type pattern comparison limit exceeded"),
        "{error}"
    );
}

#[test]
fn object_unit_returns_preserve_cleanup_and_opaque_holder_retention() {
    let definitions = r#"
extern type<T> Cell {cpp_name:"std::shared_ptr<{T}>",}
extern fn token(x:Int)->Unit {
    return_abi:"object",cpp_name:"make_unit_token",
    code:"Obj<Unit> make_unit_token(Obj<std::int64_t> x,Loc){return Obj<Unit>(new Unit{},[x](Unit* p){++*x;delete p;});}",
}
extern fn discard<T>(x:T) {
    code:"void @{fn}(Obj<@{T}> x,Loc loc){@{call token(T)->Unit}(x,loc);}",
}
extern fn retain<T>(x:T)->Cell<Unit> {
    code:"Obj<@{Cell<Unit>}> @{fn}(Obj<@{T}> x,Loc loc){auto value=@{boxed_call token(T)->Unit}(x,loc);return make_obj<@{Cell<Unit>}>(value);}",
}
"#;
    let case = Case::new(&format!(
        "{definitions} let n:=0;discard(n);wt n;{{let holder:=retain(n);wt n;}}wt n;"
    ));
    for options in [vec![], vec!["--no-opt"]] {
        let out = case.run_with_options("", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"1\n1\n2\n");
    }
    let out = Case::new(&format!(
        "{definitions} fn blocked(noescape x:Int){{let holder:=retain(x);}} blocked(1);"
    ))
    .compile();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("noescape"));
}

#[test]
fn unit_references_support_normal_and_object_return_abis_and_recursive_definitions() {
    let case = Case::new(
        r#"
extern fn unit_clone<T>(x:T)->T {
    code:"@{return T} @{fn}(Obj<@{T}> x,Loc loc){return @{call copy(T)->T}(x,loc);}",
}
extern fn descend<T>(x:T,n:Int)->T {
    return_abi:"object",
    code:"Obj<@{T}> @{fn}(Obj<@{T}> x,Obj<@{Int}> n,Loc loc){if(*n==0)return @{boxed_call copy(T)->T}(x,loc);return @{boxed_call descend(T,Int)->T}(x,make_obj<@{Int}>(*n-1),loc);}",
}
extern fn run<F,T,R>(f:F,x:T)->R {
    return_abi:"object",
    code:"Obj<@{R}> @{fn}(Obj<@{F}> f,Obj<@{T}> x,Loc loc){return @{boxed_invoke F(T)->R}(f,x,loc);}",
}
extern fn call_normal<T>(x:T)->T {
    code:"@{return T} @{fn}(Obj<@{T}> x,Loc loc){return @{call descend(T,Int)->T}(x,make_obj<@{Int}>(2),loc);}",
}
fn touch(x:Unit) {}
extern fn call_touch<T>(x:T)->T {
    return_abi:"object",
    code:"Obj<@{T}> @{fn}(Obj<@{T}> x,Loc loc){return @{boxed_call touch(T)->T}(x,loc);}",
}
unit_clone({});call_normal({});descend({},2);call_touch({});
let total:=0;run((x:Int)=>{total+=x;},3);wt total,run((x:Int)=>x+4,2);
fold([1,2,3],{},(u:Unit,x:Int)=>{total+=x;});wt total;
"#,
    );
    for options in [vec![], vec!["--no-opt"]] {
        let out = case.run_with_options("", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"3 6\n9\n");
    }
}

#[test]
fn aggregate_unit_fields_keep_shared_aliases_and_detach_on_copy() {
    let case = Case::new(
        r#"
extern fn same_unit(a:Unit,b:Unit)->Bool {
    cpp_name:"same_unit",
    code:"Obj<bool> same_unit(Obj<Unit> a,Obj<Unit> b,Loc){return make_obj<bool>(a==b);}",
}
struct Holder {value:Unit;}
let original:=Holder{value:{}};let alias:=original;let copied=original;
wt same_unit(original.value,alias.value),same_unit(original.value,copied.value);
let held:=original.value;original=copied;wt same_unit(held,original.value);
let first:=Some({});let second=first;
match (first,second) {(Some(a),Some(b))=>{wt same_unit(a,b);},_=>{wt "wrong";},};
"#,
    );
    for options in [vec![], vec!["--no-opt"]] {
        let out = case.run_with_options("", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"Yes No\nNo\nNo\n");
    }
}

#[test]
fn invalid_native_return_abis_are_checked_before_use() {
    for source in [
        "extern fn unused() {cpp_name:\"unused\",code:\"unused\",return_abi:\"scalar\",}",
        "extern fn unused<T>(x:T)->T {code:\"unused\",return_abi:\"scalar\",}",
    ] {
        let out = Case::new(source).compile();
        assert!(!out.status.success());
        assert!(String::from_utf8_lossy(&out.stderr).contains("invalid native return ABI"));
    }
}

#[test]
fn native_value_references_preserve_calls_cleanup_and_scalar_identity() {
    let case = Case::new(
        r#"
struct Item{value:Int;visits:Int;}
fn inspect(x:Item)->Int{x.visits+=1;return x.value;}
extern fn measured<T,R>(x:T)->R {
    code:"Obj<@{R}> @{fn}(Obj<@{T}> x,Loc loc){return make_obj<@{R}>(@{value_call inspect(T)->R}(x,loc));}",
}
extern fn finish(x:Int)->Int {
    cpp_name:"finish_cleanup",
    code:"Obj<std::int64_t> finish_cleanup(Obj<std::int64_t> x,Loc){struct Guard{Obj<std::int64_t> x;~Guard(){*x=7;}} guard{x};return x;}",
}
extern fn finished<T>(x:T)->Int {
    code:"Obj<@{Int}> @{fn}(Obj<@{T}> x,Loc loc){return make_obj<@{Int}>(@{value_call finish(T)->Int}(x,loc));}",
}
extern fn detached<T>(x:T)->T {
    code:"Obj<@{T}> @{fn}(Obj<@{T}> x,Loc loc){return make_obj<@{T}>(@{value_call copy(T)->T}(x,loc));}",
}
extern fn inspect_writes<T,R>(x:T)->Bool {
    code:"Obj<@{Bool}> @{fn}(Obj<@{T}>,Loc){return make_obj<@{Bool}>(@{writes inspect(T)->R});}",
}
extern fn copy_writes<T>(x:T)->Bool {
    code:"Obj<@{Bool}> @{fn}(Obj<@{T}>,Loc){return make_obj<@{Bool}>(@{writes copy(T)->T});}",
}
extern fn finish_writes<T>(x:T)->Bool {
    code:"Obj<@{Bool}> @{fn}(Obj<@{T}>,Loc){return make_obj<@{Bool}>(@{writes finish(T)->Int});}",
}
fn unchanged(x:Int)->Int{return x;}
extern fn unchanged_writes<T,R>(x:T)->Bool {
    code:"Obj<@{Bool}> @{fn}(Obj<@{T}>,Loc){return make_obj<@{Bool}>(@{writes unchanged(T)->R});}",
}
extern fn callback_writes<F,T,R>(f:F,x:T)->Bool {
    code:"Obj<@{Bool}> @{fn}(Obj<@{F}>,Obj<@{T}>,Loc){return make_obj<@{Bool}>(@{writes_invoke F(T)->R});}",
}
extern fn copy_fails<T>(x:T)->Bool {
    code:"Obj<@{Bool}> @{fn}(Obj<@{T}>,Loc){return make_obj<@{Bool}>(@{fails copy(T)->T});}",
}
extern fn finish_fails<T>(x:T)->Bool {
    code:"Obj<@{Bool}> @{fn}(Obj<@{T}>,Loc){return make_obj<@{Bool}>(@{fails finish(T)->Int});}",
}
extern fn unchanged_fails<T,R>(x:T)->Bool {
    code:"Obj<@{Bool}> @{fn}(Obj<@{T}>,Loc){return make_obj<@{Bool}>(@{fails unchanged(T)->R});}",
}
extern fn unit_copy_safe<T>()->Bool {
    code:"Obj<@{Bool}> @{fn}(Loc){return make_obj<@{Bool}>(!@{writes copy(T)->T} && !@{fails copy(T)->T});}",
}
extern fn metadata_only()->Int {cpp_name:"not_a_callable::metadata_only",code:"not valid C++",}
extern fn unknown_metadata<T>()->Bool {
    code:"Obj<@{Bool}> @{fn}(Loc){return make_obj<@{Bool}>(@{writes metadata_only()->T} && @{fails metadata_only()->T});}",
}
let row:=Item{value:4,visits:0};wt measured(row),row.visits;
let x:=2;let y:=finished(x);y=9;wt x,y;
let c:='a';let d:=detached(c);d='Z';wt c,d,detached(true);
wt inspect_writes(row),copy_writes(x),finish_writes(x),unchanged_writes(x),row.visits,x;
wt callback_writes((n:Int)=>n+1,1),callback_writes((n:Int)=>{row.visits+=1;n},1),row.visits;
wt copy_fails(x),finish_fails(x),unchanged_fails(x);
wt unit_copy_safe::<Unit>(),unknown_metadata::<Int>();
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
            b"4 1\n7 9\na Z Yes\nYes No Yes No 1 7\nNo Yes 1\nNo Yes No\nYes Yes\n"
        );
    }
}

#[test]
fn native_value_references_reject_aggregate_results() {
    for (ty, body) in [("Str", "return \"text\";"), ("Vec<Int>", "return [1];")] {
        let source = format!(
            r#"
fn produce()->{ty}{{{body}}}
extern fn native<T>()->T {{code:"@{{value_call produce()->T}}",}}
native::<{ty}>();
"#
        );
        let out = Case::new(&source).compile();
        assert!(!out.status.success());
        let error = String::from_utf8_lossy(&out.stderr);
        assert!(
            error.contains("native value references require Unit or a declared scalar result"),
            "{error}"
        );
    }
}

const CELL: &str = r#"
extern type<T> Cell { cpp_name: "std::shared_ptr<{T}>", }
extern fn cell<T>(x:T)->Cell<T> {
    code: "Obj<@{Cell<T>}> @{fn}(Obj<@{T}> x,Loc){return make_obj<@{Cell<T>}>(std::make_shared<@{T}>(*x));}",
}
extern fn get<T>(x:Cell<T>)->T {
    code: "Obj<@{T}> @{fn}(Obj<@{Cell<T>}> x,Loc){return make_obj<@{T}>(**x);}",
}
extern fn put<T>(x:Cell<T>,v:T) {
    code: "void @{fn}(Obj<@{Cell<T>}> x,Obj<@{T}> v,Loc){**x=*v;}",
}
"#;

#[test]
fn generic_native_types_and_functions_preserve_abi_and_sharing() {
    let case = Case::new(&format!(
        r#"{CELL}
let n:=3; let c:=cell(n); let alias:=c; put(alias,9);
let s:=cell("hello"); put(s,"world");
let b:=cell(true); put(b,false);
wt n,get(c),get(s),get(b),get(cell::<Int>(7)),get(get(cell(cell(5))));
"#
    ));
    for options in [vec![], vec!["--no-opt"]] {
        let out = case.run_with_options("", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"3 9 world No 7 5\n");
    }
    for emit in ["hir", "lir"] {
        let out = case.compile_with_options(&["--emit", emit]);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let ir = std::fs::read_to_string(case.0.join("main.cpp")).unwrap();
        assert!(ir.contains("Cell<Int>") && ir.contains("Cell<Str>"));
        assert!(!ir.contains("std::make_shared"));
    }
}

#[test]
fn externs_share_generic_resolution_and_leave_cpp_text_unchanged() {
    let case = Case::new(
        r#"
extern fn identity<T>(x:T)->T {
    code: "Obj<@{T}> @{fn}(Obj<@{T}> x,Loc){ using Hidden=@{Vec<T>}; /* T {Int} */ const char* s=R\"tag(T {Int} @@{T})tag\"; (void)s; return x;}",
}
fn identity(x:Int)->Int { return x+100; }
extern fn length<T>(x:T)->Int {
    code: "Obj<@{Int}> @{fn}(Obj<@{T}> x,Loc){return make_obj<@{Int}>(x->size());}",
}
let x:=3; let alias:=identity::<Int>(x); alias=5;
wt x,identity(2),identity("ok"),length([1,2,3]);
"#,
    );
    let out = case.run("");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.stdout, b"5 102 ok 3\n");
    let cpp = std::fs::read_to_string(case.0.join("main.cpp")).unwrap();
    assert!(cpp.contains("/* T {Int} */"));
    assert!(cpp.contains("R\"tag(T {Int} @{T})tag\""));
}

#[test]
fn distinct_logical_types_are_not_unified_by_native_representation() {
    let source = r#"
extern type<T> First { cpp_name: "std::shared_ptr<{T}>", }
extern type<T> Second { cpp_name: "std::shared_ptr<{T}>", }
extern fn make<T>(x:T)->First<T> {
    code: "Obj<@{First<T>}> @{fn}(Obj<@{T}> x,Loc){return make_obj<@{First<T>}>(std::make_shared<@{T}>(*x));}",
}
extern fn accept<T>(x:Second<T>)->T { code: "unused", }
accept(make(1));
"#;
    let out = Case::new(source).compile();
    assert!(!out.status.success());
    let error = String::from_utf8_lossy(&out.stderr);
    assert!(
        error.contains("main.rusk:") && !error.contains("syntax error"),
        "{error}"
    );
}

#[test]
fn invalid_native_generics_report_rusk_source_locations() {
    for source in [
        "extern type<T,T> Cell {cpp_name: \"{T}\",}",
        "extern type<T> Cell {cpp_name: \"{Missing}\",} let c:Cell<Int>;",
        "extern type<T> Cell {cpp_name: \"{T\",} let c:Cell<Int>;",
        "extern type<T> Cell {cpp_name: \"{Cell<T>}\",} let c:Cell<Int>;",
        "extern type<T> Cell {cpp_name: \"{T}\",} let c:Cell<Int,Str>;",
        "extern type<T> Cell {cpp_name: \"{T}\",} let c:Cell<Missing>;",
        "extern type<T> Cell {cpp_name: \"{T}\",} struct Cell<T>{x:T;}",
        "struct Cell<T>{x:T;} extern type<T> Cell {cpp_name: \"{T}\",}",
        "extern type<T> Cell {cpp_name: \"{T}\",} enum Cell<T>{Some(T)}",
        "extern fn f<T>(x:T){code:\"@{Missing}\",} f(1);",
        "extern fn f<T>(x:T){code:\"@{T\",} f(1);",
        "extern fn f<T>(x:T){code:\"@{}\",} f(1);",
        "extern fn f<T>(x:T,x:T){code:\"unused\",}",
        "extern fn f<T>(x:T){code:\"unused\",} fn f<U>(x:U){}",
        "extern fn f<T>(readonly x:T){code:\"unused\",}",
        "extern fn f<T>(x:T){effects:\"read_fresh\",code:\"unused\",}",
        "extern fn f<T>(x:T){code:\"unused\",} f::<Missing>(1);",
    ] {
        let out = Case::new(source).compile();
        let error = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success(), "{source}");
        assert!(
            error.contains("main.rusk:") && !error.contains("syntax error"),
            "{source}: {error}"
        );
    }
}

#[test]
fn documented_generic_extern_example_runs() {
    let document = include_str!("../docs/development/beta08-extern-prototype.md");
    let examples = document
        .split("```rusk\n")
        .skip(1)
        .map(|s| s.split_once("\n```").unwrap().0)
        .collect::<Vec<_>>();
    assert_eq!(examples.len(), 3);
    for (source, expected) in examples
        .into_iter()
        .zip(["3 hello\n", "1 2 9 2\n", "3 3 ok\n"])
    {
        let out = Case::new(source).run("");
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, expected.as_bytes());
    }
}

#[test]
fn expected_types_infer_return_only_parameters_and_nested_arguments() {
    let case = Case::new(&format!(
        r#"{CELL}
extern fn empty<T>()->Cell<T> {{
    code: "Obj<@{{Cell<T>}}> @{{fn}}(Loc){{return make_obj<@{{Cell<T>}}>(std::make_shared<@{{T}}>());}}",
}}
extern fn same<T>(x:T)->T {{ code: "Obj<@{{T}}> @{{fn}}(Obj<@{{T}}> x,Loc){{return x;}}", }}
extern fn empty_from<A,T>(x:A)->Cell<T> {{
    code: "Obj<@{{Cell<T>}}> @{{fn}}(Obj<@{{A}}>,Loc){{return make_obj<@{{Cell<T>}}>(std::make_shared<@{{T}}>());}}",
}}
fn rusk_empty<T>()->Cell<T> {{ return empty(); }}
let a:Cell<Int>:=empty(); let b:Cell<Str>:=empty();
put(a,7); put(b,"ok");
let values:Vec<Int>:=same([]); values.push_back(get(a));
let nested:Cell<Cell<Int>>:=cell(empty());
let method:Cell<Int>:=3.empty_from();
let method_values:Vec<Int>:=3.choose([]);
let rusk:Cell<Int>:=rusk_empty();
wt get(a),get(b),values,get(rusk),get(get(nested)),get(method),method_values.len();
extern fn choose<A,T>(x:A,y:T)->T {{ code: "Obj<@{{T}}> @{{fn}}(Obj<@{{A}}>,Obj<@{{T}}> y,Loc){{return y;}}", }}
"#
    ));
    let out = case.run("");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.stdout, b"7 ok 7 0 0 0 0\n");
}

#[test]
fn expected_types_do_not_override_argument_or_explicit_type_arguments() {
    let definitions = format!(
        r#"{CELL}
extern fn empty<T>()->Cell<T> {{code:"Obj<@{{Cell<T>}}> @{{fn}}(Loc){{return make_obj<@{{Cell<T>}}>(std::make_shared<@{{T}}>());}}",}}
extern fn same<T>(x:T)->T {{code:"Obj<@{{T}}> @{{fn}}(Obj<@{{T}}> x,Loc){{return x;}}",}}
"#
    );
    for call in [
        "let c:Cell<Str>:=empty::<Int>();",
        "let c:Cell<Str>:=cell(1);",
        "let c:Cell<Str>:=same::<Cell<Int>>(cell(1));",
        "empty();",
    ] {
        let out = Case::new(&format!("{definitions}\n{call}")).compile();
        let error = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success(), "{call}");
        assert!(
            (error.contains("no matching function") || error.contains("type mismatch"))
                && !error.contains("syntax error"),
            "{error}"
        );
    }
}

#[test]
fn native_type_families_reject_overrides_in_both_declaration_orders() {
    for (a, b) in [
        (
            "extern type<T> Family {cpp_name:\"{T}\",}",
            "extern type Family<Int> {cpp_name:\"bool\",}",
        ),
        (
            "struct Family<T>{x:T;}",
            "extern type Family<Int> {cpp_name:\"bool\",}",
        ),
        (
            "enum Family<T>{Value(T)}",
            "extern type Family<Int> {cpp_name:\"bool\",}",
        ),
        (
            "extern type Family {cpp_name:\"bool\",}",
            "extern type Family<Int> {cpp_name:\"bool\",}",
        ),
    ] {
        for source in [format!("{a}\n{b}"), format!("{b}\n{a}")] {
            let out = Case::new(&source).compile();
            let error = String::from_utf8_lossy(&out.stderr);
            assert!(!out.status.success(), "{source}");
            assert!(
                error.contains("duplicate type family 'Family'")
                    && error.contains("previous definition at"),
                "{error}"
            );
        }
    }
    // Distinct concrete declarations retain their legacy meaning.
    let out = Case::new(
        r#"
extern type Legacy<Int> {cpp_name:"std::int64_t",}
extern type Legacy<Str> {cpp_name:"std::string",}
wt 1;
"#,
    )
    .run("");
    assert!(out.status.success());
    assert_eq!(out.stdout, b"1\n");
}

#[test]
fn native_type_references_expand_after_all_declarations_are_registered() {
    let declarations = [
        r#"extern type<T> Outer {cpp_name:"std::shared_ptr<{Inner<T>}>",}"#,
        r#"extern type<T> Inner {cpp_name:"{T}",}"#,
        r#"struct Item {value:Int;}"#,
        r#"extern fn wrap<T>(x:T)->Outer<T> {code:"Obj<@{Outer<T>}> @{fn}(Obj<@{T}> x,Loc){return make_obj<@{Outer<T>}>(x);}",}"#,
        r#"extern fn unwrap<T>(x:Outer<T>)->T {code:"Obj<@{T}> @{fn}(Obj<@{Outer<T>}> x,Loc){return *x;}",}"#,
    ];
    for source in [
        declarations.join("\n"),
        declarations
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n"),
    ] {
        let case = Case::new(&format!(
            "{source}\nlet x:Item:=unwrap(wrap(Item{{value:4}})); wt x.value;"
        ));
        let out = case.run("");
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"4\n");
    }
}

#[test]
fn expected_results_do_not_select_between_applicable_generic_overloads() {
    let out = Case::new(r#"
fn choose<T>(x:T)->Int { return 1; }
extern fn choose<T>(x:Vec<T>)->Str {code:"Obj<std::string> @{fn}(Obj<@{Vec<T>}>,Loc){return make_obj<std::string>(\"x\");}",}
let result:Int:=choose([1]);
"#).compile();
    let error = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success());
    assert!(error.contains("ambiguous generic call"), "{error}");
}

#[test]
fn native_callable_references_use_rusk_resolution_copy_and_explicit_arguments() {
    let case = Case::new(
        r#"
extern fn detached<T>(x:T)->T {
    code:"Obj<@{T}> @{fn}(Obj<@{T}> x,Loc loc){return @{call copy(T)->T}(x,loc);}",
}
extern fn via<T>(x:T)->T {
    code:"Obj<@{T}> @{fn}(Obj<@{T}> x,Loc loc){return @{call increment(T)->T}(x,loc);}",
}
extern fn explicit<T>(x:T)->T {
    code:"Obj<@{T}> @{fn}(Obj<@{T}> x,Loc loc){return @{call echo::<T>(T)->T}(x,loc);}",
}
extern fn concrete<T>(x:T)->T {
    code:"Obj<@{T}> @{fn}(Obj<@{T}> x,Loc loc){return @{call echo(T)->T}(x,loc);}",
}
extern fn via_unit<T>(x:T) {
    code:"void @{fn}(Obj<@{T}> x,Loc loc){@{call touch(T)->Unit}(x,loc);}",
}
extern fn observed<T>(x:T)->Int {
    code:"Obj<@{Int}> @{fn}(Obj<@{T}> x,Loc loc){return @{call read_len(T)->Int}(x,loc);}",
}
fn read_len(x:Vec<Int>)->Int{return x.len();}
fn touch(x:Int){x+=1;}
fn increment<T>(x:T)->T {x+=1;return x;}
fn echo<T>(x:T)->T {return x;}
fn echo(x:Int)->Int {return x+100;}
let x:=[1,2];let y:=detached(x);y[0]=9;
let v:=10;via_unit(v);
let n:=3; wt x,y,via(n),n,explicit(5),concrete(5),v,observed(x);
"#,
    );
    for options in [vec![], vec!["--no-opt"]] {
        let out = case.run_with_options("", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"1 2 9 2 4 4 5 105 11 2\n");
    }
}

#[test]
fn native_callable_dependencies_support_forward_and_mutually_recursive_bodies() {
    let case = Case::new(
        r#"
extern fn left<T>(x:T,n:Int)->T {
    code:"Obj<@{T}> @{fn}(Obj<@{T}> x,Obj<@{Int}> n,Loc loc){if(*n==0)return @{call copy(T)->T}(x,loc);return @{call right(T,Int)->T}(x,make_obj<@{Int}>(*n-1),loc);}",
}
extern fn right<T>(x:T,n:Int)->T {
    code:"Obj<@{T}> @{fn}(Obj<@{T}> x,Obj<@{Int}> n,Loc loc){if(*n==0)return x;return @{call left(T,Int)->T}(x,make_obj<@{Int}>(*n-1),loc);}",
}
let x:=7;let y:=left(x,4);y=8;
wt x,y,left("ok",3);
"#,
    );
    let out = case.run("");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.stdout, b"7 8 ok\n");
    for emit in ["hir", "lir"] {
        let out = case.compile_with_options(&["--emit", emit]);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let ir = std::fs::read_to_string(case.0.join("main.cpp")).unwrap();
        assert!(!ir.contains("make_obj"));
    }
}

#[test]
fn unused_native_declarations_check_symbolic_types_and_reference_syntax() {
    for (source, expected) in [
        (
            r#"extern type<T> Bad {cpp_name:"{Missing}",}"#,
            "unknown type",
        ),
        (
            r#"extern type<T> Bad {cpp_name:"{Vec<T,Int>}",}"#,
            "expects 1 type arguments",
        ),
        (
            r#"extern type<T> Bad {cpp_name:"{T",}"#,
            "unterminated native type reference",
        ),
        (
            r#"extern fn bad<T>(x:Missing){code:"unused",}"#,
            "unknown type",
        ),
        (
            r#"extern fn bad<T>(x:T)->Vec<T,Int>{code:"unused",}"#,
            "expects 1 type arguments",
        ),
        (
            r#"extern fn bad<T>(x:T){code:"@{Vec<>}",}"#,
            "invalid native type reference",
        ),
        (
            r#"extern fn bad<T>(x:T){code:"@{call copy(T)}",}"#,
            "invalid native callable reference",
        ),
        (
            r#"extern fn bad<T>(x:T){code:"@{call copy(Missing)->T}",}"#,
            "unknown type",
        ),
    ] {
        let case = Case::new(source);
        for stage in ["hir", "lir", "cpp"] {
            let out = case.compile_with_options(&["--emit", stage]);
            let error = String::from_utf8_lossy(&out.stderr);
            assert!(!out.status.success(), "{source}");
            assert!(
                error.contains(expected) && error.contains("main.rusk:"),
                "{error}"
            );
            assert!(!case.0.join("main.cpp").exists());
        }
    }
    // Symbolic validation must not instantiate an arbitrary T or try to resolve copy(T).
    let out = Case::new(
        r#"
extern type<T> Alias {cpp_name:"{Later<T>}",}
extern type<T> Later {cpp_name:"{T}",}
extern fn unused<T>(x:Alias<T>)->T {code:"@{call copy(T)->T}",}
wt 1;
"#,
    )
    .run("");
    assert!(out.status.success());
    assert_eq!(out.stdout, b"1\n");
}

#[test]
fn instantiated_callable_references_check_signatures_and_limit_expanding_recursion() {
    for (source, expected) in [
        (
            r#"extern fn f<T>(x:T)->T{code:"@{call missing(T)->T}",} f(1);"#,
            "no matching native callable",
        ),
        (
            r#"fn g(x:Int)->Str{return "x";} extern fn f<T>(x:T)->T{code:"@{call g(T)->T}",} f(1);"#,
            "type mismatch",
        ),
        (
            r#"extern fn f<T>(x:T)->T{code:"@{call copy(T,T)->T}",} f(1);"#,
            "no matching native callable",
        ),
        (
            r#"extern type<T> Nest{cpp_name:"{Int}",} extern fn f<T>(x:T)->T{code:"@{call f(Nest<T>)->Nest<T>}",} f(1);"#,
            "specialization limit",
        ),
    ] {
        let out = Case::new(source).compile();
        let error = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success(), "{source}");
        assert!(
            error.contains(expected) && error.contains("main.rusk:"),
            "{error}"
        );
    }
}

#[test]
fn native_callable_references_do_not_grant_trusted_effects() {
    let definitions = r#"
fn unchanged<T>(x:T)->T{return x;}
extern fn bridge<T>(x:T)->T {
    code:"Obj<@{T}> @{fn}(Obj<@{T}> x,Loc loc){return @{call unchanged(T)->T}(x,loc);}",
}

"#;
    for (source, expected) in [
        ("readonly let x:=1; bridge(x);", "cannot prove readonly"),
        (
            "fn f(noescape x:Int){bridge(x);} f(1);",
            "cannot prove noescape",
        ),
    ] {
        let out = Case::new(&format!("{definitions}\n{source}")).compile();
        let error = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success(), "{source}");
        assert!(
            error.contains("main.rusk:") && error.contains(expected),
            "{error}"
        );
    }
}

#[test]
fn referenced_signatures_infer_return_types_and_context_for_empty_arguments() {
    let case = Case::new(
        r#"
extern fn chain<S,I,T,R>(value:S)->R {
    code:"@{return R} @{fn}(Obj<@{S}> value,Loc loc){auto extract=@{call extract(I)->T};auto length=@{call length_of(T)->R};auto pair=@{call package(S)->I}(value,loc);return length(extract(pair,loc),loc);}",
}
fn package(value:Int)->(Int,Str){return (value,"abc");}
fn extract(value:(Int,Str))->Str{return value[1];}
fn length_of(value:Str)->Int{return value.len();}
extern fn typed_empty<S,T>(source:S,values:Vec<T>)->Int {
    code:"Obj<@{Int}> @{fn}(Obj<@{S}>,Obj<@{Vec<T>}> values,Loc){auto requirement=@{call choose_element(S)->T};(void)requirement;return make_obj<@{Int}>(values->size());}",
}
fn choose_element(x:Int)->Str{return "unused";}
wt chain(4),typed_empty(1,[]);
"#,
    );
    let out = case.run("");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.stdout, b"3 0\n");
}

#[test]
fn cyclic_or_expensive_reference_inference_is_bounded_without_selecting_another_overload() {
    let cycle = r#"
extern fn a<T,R>(x:T)->R{code:"@{call b(T)->R}",}
extern fn b<T,R>(x:T)->R{code:"@{call a(T)->R}",}
a(1);
"#;
    let out = Case::new(cycle).compile();
    let error = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success());
    assert!(
        error.contains("native reference inference limit exceeded") && error.contains("main.rusk:"),
        "{error}"
    );

    // The fully typed fallback must not hide an undecidable matching candidate.
    let source = cycle.replace(
        "a(1);",
        "extern fn a<T>(x:Vec<T>)->Int{code:\"unused\",} a([1]);",
    );
    let out = Case::new(&source).compile();
    let error = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success());
    assert!(error.contains("inference limit exceeded"), "{error}");
}

#[test]
fn qualified_native_names_support_calls_value_references_and_unit_bridges() {
    let case = Case::new(
        r#"
extern fn number(x:Int)->Int {cpp_name:"native::number",code:"namespace native {Obj<std::int64_t> number(Obj<std::int64_t> x,Loc){return make_obj<std::int64_t>(*x+1);}}",}
extern fn touch(x:Int) {cpp_name:"native::touch",code:"namespace native {void touch(Obj<std::int64_t> x,Loc){++*x;}}",}
extern fn token(x:Int)->Unit {
    return_abi:"object",cpp_name:"native::token",
    code:"namespace native {Obj<Unit> token(Obj<std::int64_t> x,Loc){return Obj<Unit>(new Unit{},[x](Unit* p){++*x;delete p;});}}",
}
extern fn exercise<T>(x:T)->T {
    code:"Obj<@{T}> @{fn}(Obj<@{T}> x,Loc loc){auto boxed=@{boxed_call touch(T)->Unit}(x,loc);@{call token(T)->Unit}(x,loc);auto n=@{value_call number(T)->T}(x,loc);auto actual=@{call number(T)->T}(x,loc);return make_obj<@{T}>(n+*actual);}",
}
let n:=3;wt exercise(n),n;
"#,
    );
    for options in [vec![], vec!["--no-opt"]] {
        let out = case.run_with_options("", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"12 5\n");
    }
}

#[test]
fn unit_value_references_preserve_calls_cleanup_assignment_and_recursion() {
    let case = Case::new(
        r#"
fn touch(x:Int){x+=1;}
extern fn token(x:Int)->Unit {
    return_abi:"object",cpp_name:"native::token",
    code:"namespace native {Obj<Unit> token(Obj<std::int64_t> x,Loc){return Obj<Unit>(new Unit{},[x](Unit* p){++*x;delete p;});}}",
}
extern fn actions<T>(x:T) {
    code:"void @{fn}(Obj<@{T}> x,Loc loc){@{value_call touch(T)->Unit}(x,loc);@{value_call token(T)->Unit}(x,loc);@{value_call __assign__(T,T)->Unit}(x,make_obj<@{T}>(*x+5),loc);}",
}
extern fn descend<T>(x:T,n:Int) {
    code:"void @{fn}(Obj<@{T}> x,Obj<@{Int}> n,Loc loc){if(*n==0)return;@{value_call touch(T)->Unit}(x,loc);@{value_call descend(T,Int)->Unit}(x,make_obj<@{Int}>(*n-1),loc);}",
}
let count:=3;actions(count);wt count;descend(count,3);wt count;
"#,
    );
    for options in [vec![], vec!["--no-opt"]] {
        let out = case.run_with_options("", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"10\n13\n");
    }
}

#[test]
fn owned_references_keep_aliases_fresh_copies_and_both_unit_abis() {
    let case = Case::new(
        r#"
fn alias(x:Int)->Int{return x;}
fn action(x:Int){x+=1;}
extern fn own<T>(x:T)->T {code:"Obj<@{T}> @{fn}(Obj<@{T}> x,Loc loc){return @{owned_call copy(T)->T}(x,loc);}",return_abi:"object",}
extern fn share<T>(x:T)->T {code:"Obj<@{T}> @{fn}(Obj<@{T}> x,Loc loc){return @{owned_call alias(T)->T}(x,loc);}",}
extern fn action_object<T>(x:T)->Unit {return_abi:"object",code:"Obj<@{Unit}> @{fn}(Obj<@{T}> x,Loc loc){return @{owned_call action(T)->Unit}(x,loc);}",}
let x:=3;let independent:=own(x);let shared:=share(x);independent=8;shared=7;action_object(x);wt x,independent;
let text:="ab";let ch:=own(text[0]);text.push_back('!');ch='Z';wt text,ch;
own({});
let nested:=own([[1],[2]]);nested[0][0]=9;wt nested;
"#,
    );
    for options in [vec![], vec!["--no-opt"]] {
        let out = case.run_with_options("", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"8 8\nab! Z\n9 2\n");
    }
}

#[test]
fn object_unit_calls_keep_the_returned_token_until_its_stored_owner_is_released() {
    let case = Case::new(
        r#"
extern fn token(n:Int)->Unit{return_abi:"object",cpp_name:"token",code:"Obj<Unit> token(Obj<std::int64_t> n,Loc){return Obj<Unit>(new Unit{},[n](auto* p){++*n;delete p;});}",}
extern fn same(a:Unit,b:Unit)->Bool{cpp_name:"same",code:"Obj<bool> same(Obj<Unit> a,Obj<Unit> b,Loc){return make_obj<bool>(a==b);}",}
let count:=0;let values:=__vec_new__::<Unit>();
{let value:=token(count);values.push_back(value);}
wt count;
{let held:=values.pop_back();wt same(held,held),count;}
wt count;
"#,
    );
    for options in [vec![], vec!["--no-opt"]] {
        let out = case.run_with_options("", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"0\nYes 0\n1\n");
    }
}

#[test]
fn native_reference_results_are_observed_after_argument_cleanup() {
    let case = Case::new(
        r#"
extern type Token{cpp_name:"std::int64_t",}
extern symbol observation{code:"Obj<std::int64_t> input;Obj<bool> result;std::int64_t seen=-1;",}
extern fn prepare(){cpp_name:"prepare",code:"void prepare(Loc){result=make_obj<bool>(true);seen=-1;input=Obj<std::int64_t>(new std::int64_t(1),[](auto* p){*result=false;delete p;});}",}
extern fn inspect(x:Token)->Bool{cpp_name:"inspect",code:"Obj<bool> inspect(Obj<std::int64_t> x,Loc){input.reset();return result;}",}
extern fn touch(x:Token)->Unit{return_abi:"object",cpp_name:"touch",code:"Obj<Unit> touch(Obj<std::int64_t> x,Loc){input.reset();return Obj<Unit>(new Unit{},[](auto* p){seen=*result?1:0;delete p;});}",}
extern fn observed()->Int{cpp_name:"observed",code:"Obj<std::int64_t> observed(Loc){return make_obj<std::int64_t>(seen);}",}
extern fn normal<T>()->Bool{code:"Obj<bool> @{fn}(Loc loc){auto value=@{call inspect(T)->Bool}(input,loc);return make_obj<bool>(*value);}",}
extern fn reference<T>()->Bool{code:"Obj<bool> @{fn}(Loc loc){return make_obj<bool>(@{value_call inspect(T)->Bool}(input,loc));}",}
extern fn normal_unit<T>(){code:"void @{fn}(Loc loc){auto value=@{boxed_call touch(T)->Unit}(input,loc);}",}
extern fn bridged_unit<T>(){code:"void @{fn}(Loc loc){@{call touch(T)->Unit}(input,loc);}",}
extern fn value_unit<T>(){code:"void @{fn}(Loc loc){@{value_call touch(T)->Unit}(input,loc);}",}
prepare();wt normal::<Token>();prepare();wt reference::<Token>();
prepare();normal_unit::<Token>();wt observed();
prepare();bridged_unit::<Token>();wt observed();
prepare();value_unit::<Token>();wt observed();
"#,
    );
    for options in [vec![], vec!["--no-opt"]] {
        let out = case.run_with_options("", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"No\nNo\n0\n0\n0\n");
    }
}

#[test]
fn native_fields_resolve_logical_operations_and_keep_stored_unit_objects() {
    let case = Case::new(
        r#"
extern type Score {cpp_name:"std::int64_t",}
extern fn score(x:Int)->Score {cpp_name:"score",code:"Obj<std::int64_t> score(Obj<std::int64_t> x,Loc){return x;}",}
extern fn number(x:Score)->Int {cpp_name:"number",code:"Obj<std::int64_t> number(Obj<std::int64_t> x,Loc){return x;}",}
extern fn copy(x:Score)->Score {cpp_name:"copy_score",code:"Obj<std::int64_t> copy_score(Obj<std::int64_t> x,Loc){return make_obj<std::int64_t>(*x+100);}",}
extern fn same_unit(a:Unit,b:Unit)->Bool {cpp_name:"same_unit",code:"Obj<bool> same_unit(Obj<Unit> a,Obj<Unit> b,Loc){return make_obj<bool>(a==b);}",}
extern fn clone_fields<T>(value:T)->T {
    code:"Obj<@{T}> @{fn}(Obj<@{T}> value,Loc loc){return make_obj<@{T}>(@{T}{@{fields T as F,I}@{owned_call copy(F)->F}(value->field_@{index I},loc),@{end}});}",
}
struct F {value:Int;}
struct I {value:Int;}
struct Pair {left:Int;right:Score;unit:Unit;}
let a:=Pair{left:1,right:score(2),unit:{}};
let b:=clone_fields(a);a.left=9;
wt b.left,number(b.right),same_unit(a.unit,b.unit);
let p:=clone_fields((3,"ok",{}));wt p[0],p[1];
let text:="abc";let original:=(text[0],{});let held:=clone_fields(original);
original:=(text[1],{});original:=('z',{});text+="!";wt text,held[0];
"#,
    );
    for options in [vec![], vec!["--no-opt"]] {
        let out = case.run_with_options("", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"1 102 No\n3 ok\nabc! a\n");
    }
}

#[test]
fn native_field_scopes_nest_and_do_not_leak_between_expansions() {
    let case = Case::new(
        r#"
extern fn total_fields<T>(value:T)->Int {
    code:"Obj<@{Int}> @{fn}(Obj<@{T}> value,Loc loc){std::int64_t total=0;@{fields T as F,I}{auto field=value->field_@{index I};@{fields F as G,J}total+=*@{owned_call copy(G)->G}(field->field_@{index J},loc);@{end}}@{end}@{fields T as F,I}total+=@{index I};@{end}return make_obj<@{Int}>(total);}",
}
wt total_fields(((1,2),(3,4,5)));
"#,
    );
    for options in [vec![], vec!["--no-opt"]] {
        let out = case.run_with_options("", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"16\n");
    }
}

#[test]
fn invalid_native_fields_are_diagnosed_before_cpp_even_when_unused() {
    for (code, expected) in [
        ("@{fields T as F,I}@{Missing}@{end}", "unknown type"),
        (
            "@{fields Missing<T> as F,I}@{end}",
            "unknown type constructor",
        ),
        (
            "@{fields T as F,I}@{index J}@{end}",
            "unbound native field index",
        ),
        ("@{fields T as F,I}@{end}@{F}", "unknown type"),
        (
            "@{fields T as F,I}@{end}@{index I}",
            "unbound native field index",
        ),
        ("@{fields T as T,I}@{end}", "shadows"),
        (
            "@{fields T as F,I}@{fields F as G,I}@{end}@{end}",
            "shadows",
        ),
        (
            "@{fields T as F,I}@{I}@{end}",
            "index cannot be used as a type",
        ),
        ("@{fields T as F,F}@{end}", "invalid native fields"),
        ("@{fields T as F,I}", "unterminated native field expansion"),
        ("@{end}", "end without fields"),
    ] {
        let case = Case::new(&format!("extern fn unused<T>(x:T){{code:\"{code}\",}}"));
        let out = case.compile();
        assert!(!out.status.success(), "{code}");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains(expected),
            "{code}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(!case.0.join("main.cpp").exists());
    }
    for (value, expected) in [
        ("1", "requires a struct or Tuple"),
        ("(x:Int)=>x", "callable environment fields are private"),
    ] {
        let case = Case::new(&format!(
            r#"extern fn fields<T>(x:T){{code:"void @{{fn}}(Obj<@{{T}}> x,Loc){{@{{fields T as F,I}}@{{end}}}}",}}fields({value});"#
        ));
        let out = case.compile();
        assert!(!out.status.success());
        assert!(
            String::from_utf8_lossy(&out.stderr).contains(expected),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(!case.0.join("main.cpp").exists());
    }
}

#[test]
fn native_field_expansions_have_depth_and_work_limits() {
    let mut nested = String::new();
    for i in 0..65 {
        nested += &format!("@{{fields T as F{i},I{i}}}");
    }
    nested += &"@{end}".repeat(65);
    let out = Case::new(&format!("extern fn unused<T>(x:T){{code:\"{nested}\",}} ")).compile();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("nesting limit exceeded"));
    let fields = vec!["1"; 70].join(",");
    let case = Case::new(&format!(
        r#"extern fn wide<T>(x:T){{code:"void @{{fn}}(Obj<@{{T}}> x,Loc){{@{{fields T as F,I}}@{{fields T as G,J}}@{{index J}};@{{end}}@{{end}}}}",}} wide(({fields}));"#
    ));
    let out = case.compile();
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("work limit exceeded"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!case.0.join("main.cpp").exists());
}

#[test]
fn native_variants_copy_payloads_using_logical_operations_under_an_unrelated_name() {
    let source = include_str!("../lib/collections.rusk");
    let start = source.find("extern fn sum_copy_body<").unwrap();
    let end = source[start..]
        .find("// Standard Option operations")
        .unwrap()
        + start;
    let declaration = source[start..end]
        .lines()
        .filter(|line| !line.trim_start().starts_with("derive:"))
        .collect::<Vec<_>>()
        .join("\n")
        .replace("fn sum_copy_body<", "fn external_enum_clone<")
        .replace("const Obj<@{T}>& value", "Obj<@{T}> value");
    let case = Case::new(&format!(
        r#"{declaration}
extern type Score{{cpp_name:"std::int64_t",}}
extern fn score(x:Int)->Score{{cpp_name:"score",code:"Obj<std::int64_t> score(Obj<std::int64_t> x,Loc){{return x;}}",}}
extern fn copy(x:Score)->Score{{cpp_name:"copy_score",code:"Obj<std::int64_t> copy_score(Obj<std::int64_t> x,Loc){{return make_obj<std::int64_t>(*x+100);}}",}}
extern fn number(x:Score)->Int{{cpp_name:"number",code:"Obj<std::int64_t> number(Obj<std::int64_t> x,Loc){{return x;}}",}}
extern fn same_unit(a:Unit,b:Unit)->Bool{{cpp_name:"same_unit",code:"Obj<bool> same_unit(Obj<Unit> a,Obj<Unit> b,Loc){{return make_obj<bool>(a==b);}}",}}
struct V{{value:Int;}}struct F{{value:Int;}}struct I{{value:Int;}}
enum Event{{Empty,Row(Int,Score,Unit),Text(Char),}}
let unit:={{}};let original:=Event::Row(1,score(2),unit);let copied:=external_enum_clone(original);
original:=Event::Empty;match copied{{Row(n,s,u)=>{{wt n,number(s),same_unit(u,unit);}},_=>{{}},}};
let text:="abc";let held:=Event::Text(text[0]);let detached:=external_enum_clone(held);held:=Event::Empty;
text+="!";match detached{{Text(c)=>{{wt text,c;}},_=>{{}},}};
enum Reversed<T>{{Missing,Present(T),}}
let a:=external_enum_clone(Reversed::Present(9));match a{{Present(n)=>{{wt n;}},_=>{{}},}};
let b:Reversed<Int>:=external_enum_clone(Reversed::Missing);match b{{Missing=>{{wt "empty";}},_=>{{}},}};
let optional:=external_enum_clone(Some(unit));wt same_unit(optional.value(),unit);
"#
    ));
    for options in [vec![], vec!["--no-opt"]] {
        let out = case.run_with_options("", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"1 102 No\nabc! a\n9\nempty\nNo\n");
    }
}

#[test]
fn native_variant_scopes_support_nested_enums_and_constant_payload_indices() {
    let case = Case::new(
        r#"
extern fn shape<T>(x:T)->Int {
    code:"Obj<@{Int}> @{fn}(Obj<@{T}>,Loc){std::int64_t sum=0;@{variants T as V}sum+=@{index V};@{fields T variant V as F,I}@{variants F as W}sum+=10+@{index W};@{fields F variant W as G,J}sum+=100+@{index J};@{end}@{end}@{end}@{end}return make_obj<@{Int}>(sum);}",
}
extern fn second_width<T>(x:T)->Int {
    code:"Obj<@{Int}> @{fn}(Obj<@{T}>,Loc){std::int64_t n=0;@{fields T variant 1 as F,I}++n;@{end}return make_obj<@{Int}>(n);}",
}
enum Inner{One(Int),Two(Int,Int),}enum Outer{Empty,Values(Inner,Inner),}
wt shape(Outer::Empty),second_width(Outer::Empty);
"#,
    );
    for options in [vec![], vec!["--no-opt"]] {
        let out = case.run_with_options("", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"645 2\n");
    }
}

#[test]
fn invalid_native_variants_and_payload_scopes_fail_before_cpp() {
    for (code, expected) in [
        (
            "@{variants Missing<T> as V}@{end}",
            "unknown type constructor",
        ),
        ("@{variants T as T}@{end}", "shadows"),
        (
            "@{variants T as V}@{variants T as V}@{end}@{end}",
            "shadows",
        ),
        (
            "@{variants T as V}@{V}@{end}",
            "index cannot be used as a type",
        ),
        (
            "@{variants T as V}@{end}@{index V}",
            "unbound native field index",
        ),
        (
            "@{fields T variant V as F,I}@{end}",
            "unbound native variant index",
        ),
        (
            "@{fields T variant -1 as F,I}@{end}",
            "invalid native variant index",
        ),
        (
            "@{variants T as V}@{fields T variant V as F,V}@{end}@{end}",
            "shadows",
        ),
    ] {
        let case = Case::new(&format!("extern fn unused<T>(x:T){{code:\"{code}\",}}"));
        let out = case.compile();
        assert!(!out.status.success());
        assert!(
            String::from_utf8_lossy(&out.stderr).contains(expected),
            "{code}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(!case.0.join("main.cpp").exists());
    }
    for (code, source, expected) in [
        ("@{variants T as V}@{end}", "bad(1);", "requires an enum"),
        (
            "@{fields T variant 0 as F,I}@{end}",
            "bad((1,2));",
            "requires an enum",
        ),
        (
            "@{fields T variant 2 as F,I}@{end}",
            "enum E{A,B}bad(E::A);",
            "index out of range",
        ),
    ] {
        let case = Case::new(&format!(
            r#"extern fn bad<T>(x:T){{code:"void @{{fn}}(Obj<@{{T}}>,Loc){{{code}}}",}}{source}"#
        ));
        let out = case.compile();
        assert!(!out.status.success());
        assert!(
            String::from_utf8_lossy(&out.stderr).contains(expected),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(!case.0.join("main.cpp").exists());
    }
}

#[test]
fn native_variants_share_the_structural_expansion_work_limit() {
    let variants = (0..70)
        .map(|i| format!("V{i}"))
        .collect::<Vec<_>>()
        .join(",");
    let case = Case::new(&format!(
        r#"extern fn wide<T>(x:T){{code:"void @{{fn}}(Obj<@{{T}}>,Loc){{@{{variants T as V}}@{{variants T as W}}@{{index W}};@{{end}}@{{end}}}}",}}enum E{{{variants}}}wide(E::V0);"#
    ));
    let out = case.compile();
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("work limit exceeded"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!case.0.join("main.cpp").exists());
}
