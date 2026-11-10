use super::*;

const TYPES: &str = r#"
extern type Int{cpp_name:"std::int64_t",representation:"i64",}
extern type Unit{cpp_name:"Unit",representation:"unit",}
"#;
fn compile(standard: &str, user: &str) -> Result<String, String> {
    Compiler::default().compile(
        &[
            Source::standard(
                crate::parse(&format!("{TYPES}{standard}"), "mapped-standard.rusk").unwrap(),
            ),
            Source::user(crate::parse(user, "mapped-user.rusk").unwrap()),
        ],
        crate::backend::Emit::Hir,
    )
}
fn section<'a>(hir: &'a str, name: &str) -> &'a str {
    hir.split(&format!("name: \"{name}\""))
        .nth(1)
        .unwrap()
        .split("name:")
        .next()
        .unwrap()
}

#[test]
fn mapped_dependencies_compose_aliases_across_layers_and_leave_cycles_and_unknowns_opaque() {
    let standard = r#"
extern fn outer<T>(x:T)->T{effects:"read_fresh",code:"@{boxed_call inner(T)->T via (0) into return}",}
extern fn inner<T>(x:T)->T{effects:"read_fresh",code:"@{boxed_call child(T)->T via (0) into return}",}
extern fn legacy<T>(x:T)->T{effects:"read_fresh",code:"@{boxed_call child(T)->T}",}
extern fn touch(x:Int){cpp_name:"touch",code:"unused",effects:"write_first",}
"#;
    for (child, expected) in [
        ("fn child(x:Int)->Int{return x;}", "NativeContract"),
        (
            "extern fn child(x:Int)->Int{cpp_name:\"unknown\",code:\"unused\",}",
            "Opaque",
        ),
        ("fn child(x:Int)->Int{return outer(x);}", "Opaque"),
        ("fn child(x:Int)->Int{touch(x);return x;}", "NativeContract"),
    ] {
        let hir = compile(standard, &format!("{child}outer(1);legacy(2);")).unwrap();
        for name in ["outer", "inner"] {
            assert!(
                section(&hir, name).contains(&format!("source: {expected}")),
                "{hir}"
            );
        }
        assert!(section(&hir, "legacy").contains("source: Opaque"), "{hir}");
        if child.contains("touch(x)") {
            assert!(section(&hir, "outer").contains("write: May"), "{hir}");
            assert!(section(&hir, "outer").contains("unknown: true"), "{hir}");
        } else if expected == "NativeContract" {
            assert!(section(&hir, "outer").contains("fresh: false"), "{hir}");
            assert!(section(&hir, "outer").contains("ReturnAlias"), "{hir}");
        }
    }
}

#[test]
fn mapped_callbacks_include_the_capture_environment_and_keep_return_escape_local() {
    let standard = r#"
extern fn bridge<F,T,R>(f:F,x:T)->R{effects:"read_fresh",code:"@{boxed_invoke F(T)->R via (0,1) into return}",}
extern fn discard<F,T,R>(f:F,x:T){effects:"read_fresh",code:"@{boxed_invoke F(T)->R via (0,1) into discard}",}
extern fn touch(x:Int){cpp_name:"touch",code:"unused",effects:"write_first",}
"#;
    let hir=compile(standard,"let held:=3;bridge((x:Int)=>held,1);discard((x:Int)=>x,1);fn local(readonly noescape x:Int){let value:=bridge((x:Int)=>x,x);}local(1);").unwrap();
    assert!(
        section(&hir, "bridge").contains("source: NativeContract"),
        "{hir}"
    );
    assert!(
        hir.split("name: \"bridge\"").skip(1).any(|s| s
            .split("name:")
            .next()
            .unwrap()
            .contains("Field(")),
        "missing captured-result alias"
    );
    assert!(section(&hir, "discard").contains("escape: No"), "{hir}");
    for user in [
        "fn bad(noescape x:Int)->Int{return bridge((x:Int)=>x,x);}bad(1);",
        "fn bad(readonly x:Int){bridge((x:Int)=>{touch(x);x},x);}bad(1);",
    ] {
        let error = compile(standard, user).unwrap_err();
        assert!(
            error.contains("readonly") || error.contains("noescape"),
            "{error}"
        );
    }
}

#[test]
fn mapped_references_validate_trust_types_and_bounds_even_before_use() {
    for user in [
        "extern fn forged<T>(x:T)->T{code:\"@{call copy(T)->T via (0) into return}\",}",
        "extern fn forged<T>(x:T)->T{code:\"@{if_type T matches Int}@{call copy(T)->T via (0) into return}@{end}\",}",
    ] {
        let error = compile("", user).unwrap_err();
        assert!(error.contains("reserved"), "{error}");
    }
    for reference in [
        "call child(T)->T via (1) into return",
        "writes child(T)->T via (0) into return",
        "value_call child(T)->T via (0) into value",
    ] {
        let declaration = format!(r#"extern fn outer<T>(x:T)->T{{code:"@{{{reference}}}",}}"#);
        assert!(compile(&declaration, "").is_err(), "{declaration}");
    }
    let declaration = r#"
extern type Other{cpp_name:"std::int64_t",representation:"i64",}
extern fn child(x:Other)->Other{cpp_name:"child",code:"unused",effects:"read_fresh",}
extern fn outer<T>(x:T)->Other{effects:"read_fresh",code:"@{call child(Other)->Other via (0) into return}",}
"#;
    let error = compile(declaration, "outer(1);").unwrap_err();
    assert!(error.contains("mapping argument type mismatch"), "{error}");
    assert!(error.contains("mapped-user.rusk"), "{error}");
}

#[test]
fn mapped_writes_follow_aliases_replaced_by_an_earlier_child() {
    let standard = r#"
extern fn touch(x:Int){cpp_name:"touch",code:"unused",effects:"write_first",}
extern fn combined<T>(a:T,b:Box){effects:"read_fresh",code:"@{boxed_call install(T,Box)->Unit via (0,1) into discard};@{boxed_call write(Box)->Unit via (0.member) into discard}",}
"#;
    let user = r#"
struct Box{value:Int;}struct Outer{member:Box;}
fn install(a:Outer,b:Box){a.member:=b;}
fn write(b:Box){touch(b.value);}
fn bad(readonly b:Box){let a:=Outer{member:Box{value:0}};combined(a,b);}
bad(Box{value:1});
"#;
    let result = compile(standard, user);
    assert!(
        result.is_err(),
        "a stored readonly argument was mutated through another argument"
    );
    assert!(result.unwrap_err().contains("readonly"));
}
