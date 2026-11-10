use super::*;
const TYPES: &str = r#"
extern type Int {cpp_name:"std::int64_t",representation:"i64",}
extern type Unit {cpp_name:"Unit",representation:"unit",}
"#;
fn compile(definitions: &str, user: &str) -> Result<String, String> {
    Compiler::default().compile(
        &[
            Source::standard(
                crate::frontend::parse(&format!("{TYPES}{definitions}"), "shared-definitions.rusk")
                    .unwrap(),
            ),
            Source::user(crate::frontend::parse(user, "shared-use.rusk").unwrap()),
        ],
        crate::backend::Emit::Lir,
    )
}
const WRAPPER: &str = r#"
extern type<T> Capsule {cpp_name:"std::shared_ptr<{T}>",retains:"value:T",}
extern fn capsule<T>(x:T)->Capsule<T>{code:"unused",effects:"read_fresh",aliases:"return.value=0",}
extern fn unwrap<T>(x:Capsule<T>)->T{code:"unused",effects:"read_fresh",aliases:"return=0.value",}
extern fn same<T>(x:T)->T{code:"unused",effects:"read_fresh",aliases:"return=0",}
extern fn replace(x:Int,y:Int){cpp_name:"replace",code:"unused",effects:"write_first",}
"#;
#[test]
fn native_shared_members_follow_declarations_and_flow_through_rusk_functions() {
    let hir = compile(WRAPPER, "let x:=capsule(3);let y:=unwrap(x);").unwrap();
    assert!(hir.contains("Shared("), "{hir}");
    assert!(hir.contains("Retained("), "{hir}");
    compile(
        WRAPPER,
        "fn keep(readonly noescape x:Int){let boxed:=capsule(x);let y:=unwrap(boxed);}keep(3);",
    )
    .unwrap();
    for user in [
        "fn bad(readonly x:Int){let held:=capsule(x);replace(unwrap(held),3);}bad(3);",
        "fn relay<T>(x:Capsule<T>)->Capsule<T>{return same(x);}fn bad(readonly x:Int){replace(unwrap(relay(capsule(x))),3);}bad(3);",
        "fn leak(noescape x:Int)->Capsule<Int>{return capsule(x);}leak(3);",
        "fn relay<T>(x:Capsule<T>)->T{return unwrap(x);}fn leak(noescape x:Int)->Int{return relay(capsule(x));}leak(3);",
    ] {
        let error = compile(WRAPPER, user).unwrap_err();
        assert!(
            error.contains("readonly") || error.contains("noescape"),
            "{error}"
        );
    }
}
#[test]
fn retained_and_alias_annotations_reject_invalid_shapes_and_untrusted_origins() {
    for definitions in [
        "extern type<T> Bad{cpp_name:\"int\",retains:\"value:Missing\",}",
        "extern type<T> Bad{cpp_name:\"int\",retains:\"value:T;value:T\",}",
        "extern type<T> Bad{cpp_name:\"int\",retains:\"value:\",}",
        "extern type Bad{cpp_name:\"int\",representation:\"i64\",retains:\"value:Int\",}",
        "extern fn bad<T>(x:T)->T{code:\"unused\",aliases:\"return=0\",}",
        "extern fn bad<T>(x:T)->T{code:\"unused\",effects:\"read_fresh\",aliases:\"return=1\",}",
    ] {
        let error = compile(definitions, "").unwrap_err();
        assert!(error.contains("shared-definitions.rusk:"), "{error}");
    }
    for user in [
        "extern type<T> Spoof{cpp_name:\"int\",retains:\"value:T\",}",
        "extern type Spoof{cpp_name:\"int\",retains:\"value:Int\",}",
        "extern fn spoof<T>(x:T)->T{code:\"unused\",aliases:\"return=0\",effects:\"read_fresh\",}",
        "extern fn spoof(x:Int)->Int{cpp_name:\"spoof\",code:\"unused\",aliases:\"return=0\",}",
    ] {
        let error = compile("", user).unwrap_err();
        assert!(error.contains("reserved"), "{error}");
    }
    for aliases in ["return.missing=0", "return.value=0.value"] {
        let definitions = format!(
            r#"extern type<T> Capsule{{cpp_name:"std::shared_ptr<{{T}}>",retains:"value:T",}}extern fn make<T>(x:T)->Capsule<T>{{code:"unused",effects:"read_fresh",aliases:"{aliases}",}}"#
        );
        let error = compile(&definitions, "make(3);").unwrap_err();
        assert!(error.contains("native alias member"), "{error}");
    }
}
