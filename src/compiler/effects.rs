use super::*;

impl Compiler {
    pub(super) fn validate_lowering_source(
        &self,
        name: &Name,
        properties: &BTreeMap<String, String>,
        origin: Origin,
    ) -> Result<(), String> {
        if ["lower", "lower_owned", "lower_borrowed", "operation"]
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
        if (scalar.is_some() && owned.is_some()) || (borrowed.is_some() && owned.is_none()) {
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
        for (properties, result, expected) in [
            ("lower:\"$2\",", "Int", "argument out of range"),
            (
                "lower:\"$unknown\",",
                "Int",
                "invalid native lowering reference",
            ),
            ("lower_borrowed:\"$0\",", "Int", "incompatible"),
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
