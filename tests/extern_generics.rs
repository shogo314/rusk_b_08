mod support;
use support::Case;

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
