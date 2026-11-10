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

const STORE_RESULTS: &str = r#"
extern fn save<T>(target:Cell<T>,value:T){
 effects:"read_fresh",
 code:"void @{fn}(Obj<@{Cell<T>}> target,Obj<@{T}> value,Loc loc){target->field_0=@{boxed_call child(T)->T via (1) into 0.value}(value,loc);}",
}
extern fn save_invoke<T,F>(target:Cell<T>,value:T,callback:F){
 effects:"read_fresh",
 code:"void @{fn}(Obj<@{Cell<T>}> target,Obj<@{T}> value,Obj<@{F}> callback,Loc loc){target->field_0=@{boxed_invoke F(T)->T via (2,1) into 0.value}(callback,value,loc);}",
}
"#;

#[test]
fn mapped_stores_propagate_retention_through_results_and_captured_callbacks() {
    let declarations = "struct Cell<T>{value:T;}fn child(x:Int)->Int{return x;}";
    let hir = compile(
        STORE_RESULTS,
        &format!("{declarations}save(Cell::<Int>{{value:0}},1);"),
    )
    .unwrap();
    assert!(
        section(&hir, "save").contains("source: NativeContract"),
        "{hir}"
    );
    assert!(section(&hir, "save").contains("retained: May"), "{hir}");
    for user in [
        "fn bad(noescape value:Int){let cell:=Cell::<Int>{value:0};save(cell,value);}bad(1);",
        "fn bad(readonly cell:Cell<Int>){save(cell,1);}bad(Cell::<Int>{value:0});",
        "fn bad(noescape held:Int){let cell:=Cell::<Int>{value:0};save_invoke(cell,1,(x:Int)=>held);}bad(1);",
    ] {
        let error = compile(STORE_RESULTS, &format!("{declarations}{user}")).unwrap_err();
        assert!(
            error.contains("mapped-user.rusk:")
                && (error.contains("noescape") || error.contains("readonly")),
            "{error}"
        );
    }
    let fresh = "struct Cell<T>{value:T;}fn child(x:Int)->Int{return 7;}fn local(noescape value:Int){let cell:=Cell::<Int>{value:0};save(cell,value);}local(1);";
    let hir = compile(STORE_RESULTS, fresh).unwrap();
    assert!(section(&hir, "save").contains("retained: No"), "{hir}");
    let opaque = "struct Cell<T>{value:T;}extern fn child(x:Int)->Int{cpp_name:\"opaque_child\",code:\"unused\",}save(Cell::<Int>{value:0},1);";
    let hir = compile(STORE_RESULTS, opaque).unwrap();
    assert!(section(&hir, "save").contains("source: Opaque"), "{hir}");
}

#[test]
fn mapped_storage_validates_unselected_destinations_logical_types_and_trust() {
    for body in [
        "@{boxed_call child(T)->T via (0) into 1}",
        "@{if_type T matches Int}@{boxed_call child(T)->T via (0) into 1}@{end}",
    ] {
        let standard = format!("extern fn unused<T>(x:T){{code:\"{body}\",}}");
        let error = compile(&standard, "").unwrap_err();
        assert!(error.contains("destination out of range"), "{error}");
    }
    let user = "extern fn forged<T>(x:T,y:T){code:\"@{boxed_call child(T)->T via (1) into 0}\",}";
    assert!(compile("", user).unwrap_err().contains("reserved"));
    for target in ["0", "0.missing"] {
        let standard = format!(
            r#"
extern type Other{{cpp_name:"std::int64_t",representation:"i64",}}
extern fn child(x:Int)->Other{{cpp_name:"child",code:"unused",effects:"read_fresh",}}
extern fn outer<T>(x:T){{effects:"read_fresh",code:"@{{boxed_call child(Int)->Other via (0) into {target}}}",}}
"#
        );
        let error = compile(&standard, "outer(1);").unwrap_err();
        assert!(
            error.contains("mapped-user.rusk")
                && (error.contains("result type mismatch")
                    || error.contains("unknown native alias member")),
            "{error}"
        );
    }
}

#[test]
fn mapped_storage_executes_callback_sharing_and_character_cleanup_in_both_modes() {
    let user = r#"
struct Cell<T>{value:T;}
let text:="abc";let target:=Cell::<Char>{value:'?'};
save_invoke(target,text[0],(x:Char)=>x);target.value='Z';
let detached=target;let held:=target.value;target.value:='Q';held:='R';
text.push_back('!');wt text,target.value,detached.value,held;
let source:=[3];let destination:=Cell::<Vec<Int>>{value:[]};
save_invoke(destination,[0],(value:Vec<Int>)=>source);
destination.value[0]=7;source:=[];wt destination.value,source.len();
"#;
    let dir = std::env::temp_dir().join(format!(
        "rusk-mapped-store-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&dir).unwrap();
    for optimize in [true, false] {
        let cpp = Compiler::with_optimization(optimize)
            .compile(
                &[
                    crate::stdlib::load().unwrap(),
                    Source::standard(
                        crate::parse(STORE_RESULTS, "mapped-store-standard.rusk").unwrap(),
                    ),
                    Source::user(crate::parse(user, "mapped-store-use.rusk").unwrap()),
                ],
                crate::backend::Emit::Cpp,
            )
            .unwrap();
        std::fs::write(dir.join("main.cpp"), cpp).unwrap();
        let build = std::process::Command::new("g++")
            .args(["-std=c++20", "-O2"])
            .arg(dir.join("main.cpp"))
            .arg("-o")
            .arg(dir.join("main"))
            .output()
            .unwrap();
        assert!(
            build.status.success(),
            "{}",
            String::from_utf8_lossy(&build.stderr)
        );
        let out = std::process::Command::new(dir.join("main"))
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"Zbc! Q Z R\n7 0\n");
    }
    std::fs::remove_dir_all(dir).unwrap();
}
