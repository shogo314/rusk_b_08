//! Declaration registration passes, kept in source order.
use super::*;

impl Compiler {
    pub(super) fn register_declarations(&mut self, sources: &[Source]) -> Result<(), String> {
        generics::validate_type_declarations(sources)?;
        self.register_types_and_generics(sources)?;
        self.register_enum_declarations(sources)?;
        self.validate_declared_types()?;
        self.register_concrete_declarations(sources)?;
        self.prepare_symbols()?;
        self.prepare_native_types()?;
        Ok(())
    }
    fn register_types_and_generics(&mut self, sources: &[Source]) -> Result<(), String> {
        for source in sources {
            for s in source {
                if let Stmt::UserFunction(name, parameters, arguments, result, body) = s
                    && !parameters.is_empty()
                {
                    self.register_generic_function(generics::GenericFunction {
                        name: name.clone(),
                        parameters: parameters.clone(),
                        arguments: arguments.clone(),
                        result: result.clone(),
                        body: Some(body.clone()),
                        foreign: None,
                        origin: source.origin,
                        callbacks: vec![],
                        calls: vec![],
                    })?;
                }
                if let Stmt::GenericExternType(name, parameters, properties) = s {
                    self.register_extern_type(name, parameters, properties, source.origin)?;
                }
                if let Stmt::GenericExternFunction(
                    name,
                    parameters,
                    arguments,
                    result,
                    properties,
                ) = s
                {
                    self.register_extern_function(
                        name,
                        parameters,
                        arguments,
                        result,
                        properties,
                        source.origin,
                    )?;
                }
                if let Stmt::Struct(name, parameters, fields) = s {
                    if !parameters.is_empty() {
                        self.register_generic_record(generics::GenericRecord {
                            name: name.clone(),
                            parameters: parameters.clone(),
                            fields: fields.clone(),
                        })?;
                        continue;
                    }
                    let native = format!("RuskRecord{}", self.records.len());
                    if self.generic_records.contains_key(&name.text)
                        || self.generic_extern_types.contains_key(&name.text)
                        || self.types.insert(name.text.clone(), native).is_some()
                    {
                        return Err(name.span.error("duplicate type"));
                    }
                    self.records.insert(
                        name.text.clone(),
                        records::Record {
                            name: name.clone(),
                            fields: fields.clone(),
                        },
                    );
                }
                if let Stmt::Type(n, p) = s {
                    if self.generic_records.contains_key(&n.text)
                        || self.generic_extern_types.contains_key(&n.text)
                    {
                        return Err(n.span.error("duplicate type"));
                    }
                    validate(n, p, &["cpp_name", "representation", "retains"])?;
                    if self
                        .types
                        .insert(n.text.clone(), property(n, p, "cpp_name")?)
                        .is_some()
                    {
                        return Err(n.span.error("duplicate type"));
                    }
                    if let Some(kind) = type_metadata::parse_properties(n, p, source.origin)? {
                        self.native_types.insert(
                            n.text.clone(),
                            type_metadata::Metadata {
                                kind,
                                span: n.span.clone(),
                            },
                        );
                    }
                }
            }
        }
        Ok(())
    }
    fn register_enum_declarations(&mut self, sources: &[Source]) -> Result<(), String> {
        for source in sources {
            for s in source {
                if let Stmt::Enum(name, parameters, variants) = s {
                    self.register_enum(name, parameters, variants)?;
                }
            }
        }
        Ok(())
    }
    fn validate_declared_types(&mut self) -> Result<(), String> {
        self.validate_extern_declarations()?;
        for (name, meta) in self.native_types.clone() {
            self.validate_representation(
                &Name {
                    text: name,
                    span: meta.span,
                },
                &meta.kind,
            )?;
        }
        let enum_names = self
            .enum_definitions
            .values()
            .filter(|d| d.parameters.is_empty())
            .map(|d| d.name.clone())
            .collect::<Vec<_>>();
        for name in enum_names {
            self.check_type(&name)?;
        }
        self.prepare_records()?;
        Ok(())
    }
    fn register_concrete_declarations(&mut self, sources: &[Source]) -> Result<(), String> {
        for source in sources {
            for s in source {
                if let Stmt::UserFunction(_, parameters, ..) = s
                    && !parameters.is_empty()
                {
                    continue;
                }
                match s {
                    Stmt::Function(n, params, result, _)
                    | Stmt::UserFunction(n, _, params, result, _) => {
                        let (cpp, code, body) = match s {
                            Stmt::Function(_, _, _, p) => {
                                validate(
                                    n,
                                    p,
                                    &[
                                        "cpp_name",
                                        "code",
                                        "effects",
                                        "aliases",
                                        "lower",
                                        "lower_owned",
                                        "lower_borrowed",
                                        "lower_rooted",
                                        "operation",
                                        "return_abi",
                                        "implicit_call",
                                    ],
                                )?;
                                self.register_implicit_call(n, params.len(), p)?;
                                (property(n, p, "cpp_name")?, property(n, p, "code")?, None)
                            }
                            Stmt::UserFunction(_, _, params, _, body) => (
                                format!("rusk_fn{}", self.functions.len()),
                                String::new(),
                                Some((
                                    params.iter().map(|(n, _)| n.clone()).collect(),
                                    body.clone(),
                                )),
                            ),
                            _ => unreachable!(),
                        };
                        let mut names = BTreeSet::new();
                        for (param, ty) in params {
                            if matches!(s, Stmt::Function(..))
                                && let Some(annotation) = &param.binding
                            {
                                return Err(annotation.span.error("binding modifiers on extern parameters require a Rusk function body"));
                            }
                            if matches!(s, Stmt::Function(..))
                                && let Some(annotation) = &param.access
                            {
                                return Err(annotation.span.error("access modifiers on extern parameters require a Rusk function body"));
                            }
                            if matches!(s, Stmt::Function(..))
                                && let Some(annotation) = &param.escape
                            {
                                return Err(annotation.span.error("escape modifiers on extern parameters require a Rusk function body"));
                            }
                            self.check_type(ty)?;
                            if !names.insert(&param.text) {
                                return Err(param.span.error("duplicate parameter"));
                            }
                        }
                        if let Some(r) = result {
                            self.check_type(r)?;
                        }
                        let result = result
                            .as_ref()
                            .map(|r| r.text.clone())
                            .unwrap_or("Unit".into());
                        let params: Vec<_> = params.iter().map(|(_, t)| t.text.clone()).collect();
                        if let Some(old) = self.functions.iter().find(|f| {
                            f.name.text == n.text
                                && f.params == params
                                && (!return_selected(&n.text) || f.result == result)
                        }) {
                            return Err(n.span.error(format!(
                                "duplicate function; previous definition at {}:{}:{}",
                                old.name.span.file, old.name.span.line, old.name.span.column
                            )));
                        }
                        if let Stmt::Function(_, _, _, properties) = s
                            && let Some(effects) = self.declared_native_effects(
                                n,
                                &params,
                                &result,
                                properties,
                                source.origin,
                            )?
                        {
                            self.native_info_mut(self.functions.len()).contract = Some(effects);
                        }
                        if let Stmt::Function(_, _, _, properties) = s {
                            self.register_return_abi(self.functions.len(), n, properties, &result)?;
                            self.register_native_lowering(
                                self.functions.len(),
                                n,
                                &params,
                                &result,
                                properties,
                                source.origin,
                            )?;
                        }
                        self.functions.push(Function {
                            name: n.clone(),
                            params,
                            result,
                            cpp,
                            code,
                            body,
                            origin: source.origin,
                            checked: None,
                        });
                    }
                    Stmt::Symbol(n, p) => self.register_symbol(n, p)?,
                    _ => {}
                }
            }
        }
        Ok(())
    }
}
