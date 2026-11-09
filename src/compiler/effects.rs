use super::*;

impl Compiler {
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
}
