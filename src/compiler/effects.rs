use super::*;

impl Compiler {
    pub(super) fn validate_lowering_source(
        &self,
        name: &Name,
        properties: &BTreeMap<String, String>,
        origin: Origin,
    ) -> Result<(), String> {
        if [
            "lower",
            "lower_owned",
            "lower_borrowed",
            "lower_rooted",
            "operation",
        ]
        .iter()
        .any(|p| properties.contains_key(*p))
            && origin != Origin::StandardLibrary
        {
            return Err(name.span.error(
                "native lowering definitions are reserved for the embedded standard library",
            ));
        }
        Ok(())
    }

    pub(super) fn register_native_lowering(
        &mut self,
        index: usize,
        name: &Name,
        params: &[String],
        result: &str,
        properties: &BTreeMap<String, String>,
        origin: Origin,
    ) -> Result<(), String> {
        self.validate_lowering_source(name, properties, origin)?;
        let parse = |key| {
            properties
                .get(key)
                .map(|s| {
                    crate::native_lowering::Template::parse(s, params.len())
                        .map_err(|e| name.span.error(e))
                })
                .transpose()
        };
        let scalar = parse("lower")?;
        let owned = parse("lower_owned")?;
        let borrowed = parse("lower_borrowed")?;
        let rooted = parse("lower_rooted")?;
        if scalar.is_some()
            && !self
                .ir_types()
                .get(result)
                .is_some_and(crate::hir::Type::leaf)
        {
            return Err(name
                .span
                .error("scalar lowering requires a declared scalar representation"));
        }
        if (scalar.is_some() && owned.is_some())
            || ((borrowed.is_some() || rooted.is_some()) && owned.is_none())
        {
            return Err(name
                .span
                .error("incompatible native lowering representations"));
        }
        let assignment = match properties.get("operation").map(String::as_str) {
            None => false,
            Some("assign")
                if scalar.is_some()
                    && result == "Unit"
                    && params.len() == 2
                    && params[0] == params[1]
                    && properties
                        .get("effects")
                        .is_some_and(|e| e == "write_first") =>
            {
                true
            }
            _ => return Err(name.span.error("invalid native operation descriptor")),
        };
        if scalar.is_some() || owned.is_some() {
            self.native_lowerings.insert(
                index,
                crate::native_lowering::Lowering {
                    scalar,
                    owned,
                    borrowed,
                    rooted,
                    assignment,
                },
            );
        }
        Ok(())
    }

    pub(super) fn declared_native_effects(
        &self,
        name: &Name,
        parameters: &[String],
        result: &str,
        properties: &BTreeMap<String, String>,
        origin: Origin,
    ) -> Result<Option<crate::effects::Summary>, String> {
        let Some(specification) = properties.get("effects") else {
            return Ok(None);
        };
        if origin != Origin::StandardLibrary {
            return Err(name
                .span
                .error("native effect contracts are reserved for the embedded standard library"));
        }
        crate::native_effects::contract(specification, parameters, result, &self.ir_types())
            .map(Some)
            .map_err(|error| name.span.error(error))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DECLARATIONS: &str = r#"
extern type Int {cpp_name:"std::int64_t",}
extern type Unit {cpp_name:"Unit",}
extern fn stamp<T>(x:T)->Int {
    code:"Obj<@{Int}> @{fn}(Obj<@{T}>,Loc){return make_obj<@{Int}>(7);}",
    effects:"read_fresh",
}
stamp(1);
"#;

    const TYPES: &str = r#"
extern type Int {cpp_name:"std::int64_t",representation:"i64",}
extern type Unit {cpp_name:"Unit",representation:"unit",}
extern type Str {cpp_name:"std::string",}
"#;

    #[test]
    fn lowered_value_references_can_call_both_unit_abi_bridges() {
        let mut core = crate::parse(include_str!("../../lib/core.rusk"), "core.rusk").unwrap();
        let mut collections = crate::parse(
            include_str!("../../lib/collections.rusk"),
            "collections.rusk",
        )
        .unwrap();
        for statement in &mut core {
            if let Stmt::Symbol(_, properties) = statement {
                let code = properties.get_mut("code").unwrap();
                *code = code.replace("/* RUSK_IO_RUNTIME */", include_str!("../../lib/io.hpp"));
                code.push_str(include_str!("../../lib/scalar.hpp"));
            }
        }
        for statement in &mut collections {
            if let Stmt::Symbol(_, properties) = statement {
                properties
                    .get_mut("code")
                    .unwrap()
                    .push_str(include_str!("../../lib/containers.hpp"));
            }
        }
        let definitions=crate::parse(r#"
extern fn touch(x:Unit){cpp_name:"lower_touch",code:"void lower_touch(Obj<Unit>,Loc){}",}
extern fn flag<T>(x:T)->Bool{
    code:"Obj<@{Bool}> @{fn}(Obj<@{T}> x,Loc loc){@{call copy(T)->T}(x,loc);@{boxed_call touch(T)->T}(x,loc);return make_obj<@{Bool}>(true);}",
    lower:"(@{call copy(T)->T}($0,$loc),@{boxed_call touch(T)->T}($0,$loc),true)",
}
extern fn via<T>(x:T)->Bool{
    code:"Obj<@{Bool}> @{fn}(Obj<@{T}> x,Loc loc){return make_obj<@{Bool}>(@{value_call flag(T)->Bool}(x,loc));}",
}
"#,"renamed-lower.rusk").unwrap();
        let temp = std::env::temp_dir().join(format!(
            "rusk-lower-unit-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&temp).unwrap();
        for optimize in [true, false] {
            let sources = [
                Source::standard(core.clone()),
                Source::standard(collections.clone()),
                Source::standard(
                    crate::parse(
                        include_str!("../../lib/vec.rusk"),
                        "vector-declarations.rusk",
                    )
                    .unwrap(),
                ),
                Source::standard(definitions.clone()),
                Source::user(crate::parse("wt via({});", "unit-call.rusk").unwrap()),
            ];
            let cpp = Compiler::with_optimization(optimize)
                .compile(&sources, crate::backend::Emit::Cpp)
                .unwrap();
            std::fs::write(temp.join("main.cpp"), cpp).unwrap();
            let build = std::process::Command::new("g++")
                .args(["-std=c++20", "-O2"])
                .arg(temp.join("main.cpp"))
                .arg("-o")
                .arg(temp.join("main"))
                .output()
                .unwrap();
            assert!(
                build.status.success(),
                "{}",
                String::from_utf8_lossy(&build.stderr)
            );
            let run = std::process::Command::new(temp.join("main"))
                .output()
                .unwrap();
            assert!(
                run.status.success(),
                "{}",
                String::from_utf8_lossy(&run.stderr)
            );
            assert_eq!(run.stdout, b"Yes\n");
        }
        std::fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn native_contracts_wait_for_referenced_operations_without_assuming_call_cycles_are_pure() {
        let standard = Source::standard(crate::parse(&format!(r#"{TYPES}
extern fn outer<T>(x:T)->T {{effects:"read_fresh",code:"Obj<@{{T}}> @{{fn}}(Obj<@{{T}}> x,Loc loc){{return @{{call middle(T)->T}}(x,loc);}}",}}
extern fn middle<T>(x:T)->T {{effects:"read_fresh",code:"Obj<@{{T}}> @{{fn}}(Obj<@{{T}}> x,Loc loc){{return @{{call child(T)->T}}(x,loc);}}",}}
extern fn lower_only<T,R>(x:T)->R {{effects:"read_fresh",code:"unused",lower:"@{{value_call child(T)->R}}($0,$loc)",}}
extern fn touch(x:Int) {{cpp_name:"touch",code:"unused",effects:"write_first",}}
extern fn emit(x:Int) {{cpp_name:"emit",code:"unused",effects:"io_write",}}
extern fn checked(x:Int)->Int {{cpp_name:"checked",code:"unused",effects:"checked_read_fresh",}}
"#),"standard.rusk").unwrap());
        for (definition, expected) in [
            ("fn child(x:Int)->Int{return 7;}", "NativeContract"),
            (
                "extern fn child(x:Int)->Int{cpp_name:\"child\",code:\"unused\",}",
                "Opaque",
            ),
            ("fn child(x:Int)->Int{touch(x);return 7;}", "Opaque"),
            ("fn child(x:Int)->Int{return x;}", "Opaque"),
            ("fn child(x:Int)->Int{emit(x);return 7;}", "Opaque"),
            ("fn child(x:Int)->Int{return checked(x);}", "Opaque"),
            ("fn child(x:Int)->Int{return outer(x);}", "Opaque"),
        ] {
            let user = Source::user(
                crate::parse(
                    &format!("{definition} outer(1);lower_only(1);"),
                    "user.rusk",
                )
                .unwrap(),
            );
            let hir = Compiler::default()
                .compile(
                    &[Source::standard(standard.statements.clone()), user],
                    crate::backend::Emit::Hir,
                )
                .unwrap();
            for name in ["outer", "middle", "lower_only"] {
                let section = hir
                    .split(&format!("name: \"{name}\""))
                    .nth(1)
                    .unwrap()
                    .split("name:")
                    .next()
                    .unwrap();
                assert!(
                    section.contains(&format!("source: {expected}")),
                    "{definition}\n{hir}"
                );
            }
        }
    }

    #[test]
    fn native_contracts_also_check_captured_callback_dependencies() {
        let standard=Source::standard(crate::parse(&format!(r#"{TYPES}
extern fn bridge<F>(f:F)->Int{{effects:"read_fresh",code:"Obj<@{{Int}}> @{{fn}}(Obj<@{{F}}> f,Loc loc){{return @{{invoke F()->Int}}(f,loc);}}",}}
extern fn touch(x:Int){{cpp_name:"touch",code:"unused",effects:"write_first",}}
extern fn copy(x:Int)->Int{{cpp_name:"copy_int",code:"unused",effects:"read_fresh",}}
"#),"standard.rusk").unwrap());
        let derivations = Source::standard(
            crate::parse(include_str!("../../lib/collections.rusk"), "standard-derivations.rusk")
                .unwrap()
                .into_iter()
                .filter(|statement| matches!(statement, Stmt::GenericExternFunction(_,_,_,_,properties) if properties.contains_key("derive")))
                .collect(),
        );
        for (callback, expected) in [
            ("()=>7", "NativeContract"),
            ("()=>{touch(value);7}", "Opaque"),
        ] {
            let user = Source::user(
                crate::parse(&format!("let value:=1;bridge({callback});"), "user.rusk").unwrap(),
            );
            let hir = Compiler::default()
                .compile(
                    &[
                        Source::standard(standard.statements.clone()),
                        Source::standard(derivations.statements.clone()),
                        user,
                    ],
                    crate::backend::Emit::Hir,
                )
                .unwrap();
            let section = hir
                .split("name: \"bridge\"")
                .nth(1)
                .unwrap()
                .split("name:")
                .next()
                .unwrap();
            assert!(section.contains(&format!("source: {expected}")), "{hir}");
        }
    }

    #[test]
    fn lowering_and_assignment_roles_do_not_depend_on_function_names() {
        let definitions = r#"
extern fn unrelated(a:Int,b:Int)->Int {
    cpp_name:"wire_sum", code:"unused",lower:"(*$0 + *$1)",effects:"read_fresh",
}
extern fn store(a:Int,b:Int) {
    cpp_name:"wire_store",code:"unused",lower:"*$0 = *$1",operation:"assign",effects:"write_first",
}
let x:=unrelated(1,2);store(x,4);
"#;
        let source = Source::standard(
            crate::parse(&format!("{TYPES}\n{definitions}"), "renamed.rusk").unwrap(),
        );
        let cpp = Compiler::default()
            .compile(&[source], crate::backend::Emit::Cpp)
            .unwrap();
        let main = cpp.split("int main()").nth(1).unwrap();
        assert!(
            !main.contains("wire_sum(") && !main.contains("wire_store("),
            "{main}"
        );
        assert!(main.contains(" + ") && main.contains(" = "), "{main}");
    }

    #[test]
    fn lowering_rejects_untrusted_sources_and_invalid_descriptors() {
        for key in ["lower", "lower_owned", "lower_borrowed", "lower_rooted"] {
            let text = format!(
                "{TYPES} extern fn unused<T>(x:T)->Int{{code:\"unused\",{key}:\"@{{Missing}}\",}}"
            );
            let source = Source::standard(crate::parse(&text, "descriptor.rusk").unwrap());
            let error = Compiler::default()
                .compile(&[source], crate::backend::Emit::Cpp)
                .unwrap_err();
            assert!(error.contains("unknown type 'Missing'"), "{error}");
        }
        for (properties, result, expected) in [
            ("lower:\"$2\",", "Int", "argument out of range"),
            (
                "lower:\"$unknown\",",
                "Int",
                "invalid native lowering reference",
            ),
            ("lower_borrowed:\"$0\",", "Int", "incompatible"),
            ("lower_rooted:\"$0\",", "Int", "incompatible"),
            ("lower:\"1\",lower_owned:\"$0\",", "Int", "incompatible"),
            ("lower:\"1\",", "Str", "scalar lowering requires"),
            (
                "lower:\"1\",operation:\"assign\",",
                "Int",
                "invalid native operation",
            ),
        ] {
            let text = format!(
                "{TYPES}\nextern fn f(x:Int)->{result}{{cpp_name:\"f\",code:\"unused\",{properties}}}"
            );
            let source = Source::standard(crate::parse(&text, "descriptor.rusk").unwrap());
            let error = Compiler::default()
                .compile(&[source], crate::backend::Emit::Cpp)
                .unwrap_err();
            assert!(error.contains(expected), "{error}");
        }
        let text = format!(
            "{TYPES}\nextern fn f(x:Int)->Int{{cpp_name:\"f\",code:\"unused\",lower:\"*$0\",}} f(1);"
        );
        let text = text
            .replace("representation:\"i64\",", "")
            .replace("representation:\"unit\",", "");
        let source = Source::user(crate::parse(&text, "<stdlib/core.rusk>").unwrap());
        let error = Compiler::default()
            .compile(&[source], crate::backend::Emit::Cpp)
            .unwrap_err();
        assert!(
            error.contains("lowering definitions are reserved"),
            "{error}"
        );
    }

    #[test]
    fn standard_generic_contracts_follow_origin_when_the_display_name_changes() {
        let source =
            Source::standard(crate::parse(DECLARATIONS, "renamed-standard-library.rusk").unwrap());
        let hir = Compiler::default()
            .compile(&[source], crate::backend::Emit::Hir)
            .unwrap();
        assert!(hir.contains("name: \"stamp\""));
        assert!(hir.contains("source: NativeContract"), "{hir}");
    }

    #[test]
    fn embedded_display_name_cannot_grant_a_user_contract() {
        let source = Source::user(crate::parse(DECLARATIONS, "<stdlib/core.rusk>").unwrap());
        let error = Compiler::default()
            .compile(&[source], crate::backend::Emit::Hir)
            .unwrap_err();
        assert!(
            error.contains("reserved for the embedded standard library"),
            "{error}"
        );
    }

    #[test]
    fn backend_trust_uses_origin_instead_of_spelling() {
        let mut compiler = Compiler::default();
        for (origin, file) in [
            (Origin::User, "<stdlib/core.rusk>"),
            (Origin::StandardLibrary, "renamed.rusk"),
        ] {
            compiler.functions.push(Function {
                name: Name {
                    text: "unrelated".into(),
                    span: Span {
                        file: file.into(),
                        line: 1,
                        column: 1,
                    },
                },
                params: vec![],
                result: "Unit".into(),
                cpp: "unrelated".into(),
                code: String::new(),
                body: None,
                checked: None,
                origin,
            });
        }
        let context = compiler.cpp_context();
        assert!(!context.functions[0].trusted);
        assert!(context.functions[1].trusted);
    }

    #[test]
    fn diagnostic_location_inheritance_also_follows_source_origin() {
        for (origin, display, inherited) in [
            (Origin::StandardLibrary, "renamed.rusk", true),
            (Origin::User, "<stdlib/core.rusk>", false),
        ] {
            let declarations = Source::standard(
                crate::parse(
                    r#"
extern type Int {cpp_name:"std::int64_t",}
extern type Unit {cpp_name:"Unit",}
extern fn checked(x:Int)->Int {cpp_name:"checked",code:"unused",effects:"checked_read_fresh",}
"#,
                    "runtime.rusk",
                )
                .unwrap(),
            );
            let wrapper = Source {
                origin,
                statements: crate::parse(
                    "fn wrapper(x:Int)->Int{return checked(x);}wrapper(1);",
                    display,
                )
                .unwrap(),
            };
            let hir = Compiler::default()
                .compile(&[declarations, wrapper], crate::backend::Emit::Hir)
                .unwrap();
            let body = hir
                .split("name: \"wrapper\",")
                .nth(1)
                .unwrap()
                .split("effects: Summary")
                .next()
                .unwrap();
            assert!(
                body.contains(&format!("inherit_location: {inherited}")),
                "{body}"
            );
        }
    }
}
