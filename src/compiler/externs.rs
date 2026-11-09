//! Initial generic extern prototype: opaque native types and static native bodies.
use super::*;
mod template;
pub(super) mod type_patterns;
use template::Part;

#[derive(Clone)]
pub(super) struct ExternalType {
    pattern: String,
    parameters: Vec<Name>,
    native: String,
    span: Span,
    representation: Option<crate::hir::Type>,
}

impl Compiler {
    pub(super) fn register_return_abi(
        &mut self,
        index: usize,
        name: &Name,
        properties: &BTreeMap<String, String>,
        result: &str,
    ) -> Result<(), String> {
        if let Some(abi) = properties.get("return_abi") {
            if abi != "object" {
                return Err(name
                    .span
                    .error("invalid native return ABI; expected object"));
            }
            if result == "Unit" {
                self.native_unit_objects.insert(index);
            }
        }
        Ok(())
    }
    pub(super) fn abi_reference_name(&self, index: usize, object: bool) -> String {
        if self.functions[index].result != "Unit"
            || object == self.native_unit_objects.contains(&index)
        {
            self.functions[index].cpp.clone()
        } else {
            (if object {
                crate::native_abi::UnitBridge::Box
            } else {
                crate::native_abi::UnitBridge::Discard
            })
            .name(index)
        }
    }
    pub(super) fn abi_reference(&mut self, index: usize, object: bool) -> String {
        if self.functions[index].result == "Unit"
            && object != self.native_unit_objects.contains(&index)
        {
            self.native_unit_bridges.insert((
                index,
                if object {
                    crate::native_abi::UnitBridge::Box
                } else {
                    crate::native_abi::UnitBridge::Discard
                },
            ));
            if self.functions[index].body.is_none()
                && template::identifier(&self.functions[index].cpp)
            {
                self.native_references.insert(index);
            }
        }
        self.abi_reference_name(index, object)
    }
    pub(super) fn register_extern_type(
        &mut self,
        name: &Name,
        parameters: &[Name],
        properties: &BTreeMap<String, String>,
        origin: Origin,
    ) -> Result<(), String> {
        generics::parameters_valid(parameters)?;
        if name.text.starts_with("__Rusk")
            || matches!(
                tuples::type_parts(&name.text).map_or(name.text.as_str(), |(base, _)| base),
                "Vec" | "Tuple" | "Option" | "Dict" | "Set"
            )
        {
            return Err(name
                .span
                .error("generic extern requires an unreserved type constructor name"));
        }
        if self.types.contains_key(&name.text)
            || self.generic_records.contains_key(&name.text)
            || self.generic_extern_types.contains_key(&name.text)
        {
            return Err(name.span.error("duplicate type"));
        }
        validate(name, properties, &["cpp_name", "representation"])?;
        let representation = type_metadata::parse_properties(name, properties, origin)?;
        let pattern = type_patterns::pattern(name, parameters);
        let mut bound = BTreeMap::new();
        generics::unify(&pattern, &pattern, parameters, &mut bound);
        for parameter in parameters {
            if !bound.contains_key(&parameter.text) {
                return Err(parameter
                    .span
                    .error("native type parameter is not bound by the type pattern"));
            }
        }
        self.generic_extern_types.insert(
            name.text.clone(),
            ExternalType {
                pattern,
                parameters: parameters.to_vec(),
                native: property(name, properties, "cpp_name")?,
                span: name.span.clone(),
                representation,
            },
        );
        Ok(())
    }

    pub(super) fn register_extern_function(
        &mut self,
        name: &Name,
        parameters: &[Name],
        arguments: &[(DeclaredName, Name)],
        result: &Option<Name>,
        properties: &BTreeMap<String, String>,
        origin: Origin,
    ) -> Result<(), String> {
        validate(
            name,
            properties,
            &[
                "code",
                "effects",
                "lower",
                "lower_owned",
                "lower_borrowed",
                "operation",
                "return_abi",
            ],
        )?;
        if properties.get("return_abi").is_some_and(|v| v != "object") {
            return Err(name
                .span
                .error("invalid native return ABI; expected object"));
        }
        if properties.contains_key("effects") && origin != Origin::StandardLibrary {
            return Err(name
                .span
                .error("native effect contracts are reserved for the embedded standard library"));
        }
        self.validate_lowering_source(name, properties, origin)?;
        property(name, properties, "code")?;
        let mut parts = Vec::new();
        for key in ["code", "lower", "lower_owned", "lower_borrowed"] {
            if let Some(text) = properties.get(key) {
                parts.extend(template::parse(text, true).map_err(|e| name.span.error(e))?);
            }
        }
        let calls = parts
            .iter()
            .filter_map(|part| match part {
                Part::Call {
                    name,
                    arguments,
                    result,
                    ..
                } => Some(generics::CallRequirement {
                    name: name.clone(),
                    arguments: arguments.clone(),
                    result: result.clone(),
                }),
                _ => None,
            })
            .collect();
        let callbacks = parts
            .into_iter()
            .filter_map(|part| match part {
                Part::Invoke {
                    name,
                    arguments,
                    result,
                    ..
                } => Some(generics::CallbackRequirement {
                    ty: name,
                    parameters: arguments,
                    result,
                }),
                _ => None,
            })
            .collect();
        for (argument, _) in arguments {
            if argument.binding.is_some() || argument.access.is_some() || argument.escape.is_some()
            {
                return Err(argument
                    .span
                    .error("modifiers on extern parameters require a Rusk function body"));
            }
        }
        self.register_generic_function(generics::GenericFunction {
            name: name.clone(),
            parameters: parameters.to_vec(),
            arguments: arguments.to_vec(),
            result: result.clone(),
            body: None,
            foreign: Some(properties.clone()),
            origin,
            callbacks,
            calls,
        })
    }

    pub(super) fn specialize_extern_type(&mut self, name: &Name) -> Result<bool, String> {
        let Some((base, args)) = tuples::type_parts(&name.text) else {
            return Ok(false);
        };
        let Some(arity) = self.external_type_arity(base) else {
            return Ok(false);
        };
        if args.len() != arity {
            return Err(name
                .span
                .error(format!("type '{base}' expects {} type arguments", arity)));
        }
        let Some((template, bindings)) = self.generic_extern_types.values().find_map(|template| {
            let mut bindings = BTreeMap::new();
            generics::unify(
                &template.pattern,
                &name.text,
                &template.parameters,
                &mut bindings,
            )
            .then(|| (template.clone(), bindings))
        }) else {
            return Ok(false);
        };
        if self.preparing_extern_types.len() >= 64
            || !self.preparing_extern_types.insert(name.text.clone())
        {
            return Err(name.span.error("recursive generic extern type expansion"));
        }
        let result = (|| {
            for ty in args {
                self.check_type(&Name {
                    text: ty.into(),
                    span: name.span.clone(),
                })?;
            }
            let native = self.render_native(
                &template.native,
                &bindings,
                None,
                &template.span,
                false,
                None,
            )?;
            self.types.insert(name.text.clone(), native);
            if let Some(kind) = template.representation {
                let kind = type_metadata::substitute(&kind, &bindings);
                self.native_types.insert(
                    name.text.clone(),
                    type_metadata::Metadata {
                        kind: kind.clone(),
                        span: template.span.clone(),
                    },
                );
                self.validate_representation(
                    &Name {
                        text: name.text.clone(),
                        span: template.span.clone(),
                    },
                    &kind,
                )?;
            }
            Ok(true)
        })();
        self.preparing_extern_types.remove(&name.text);
        result.map_err(|error: String| {
            format!(
                "{error}\nwhile specializing '{}' at {}:{}:{}",
                name.text, name.span.file, name.span.line, name.span.column
            )
        })
    }

    fn external_type_arity(&self, base: &str) -> Option<usize> {
        self.generic_extern_types.values().find_map(|definition| {
            tuples::type_parts(&definition.pattern)
                .filter(|(name, _)| *name == base)
                .map(|(_, args)| args.len())
        })
    }

    // Check symbolic patterns without choosing arbitrary types for their parameters.
    fn validate_symbolic_type(
        &self,
        ty: &str,
        parameters: &[Name],
        span: &Span,
    ) -> Result<(), String> {
        if !template::valid_type(ty) {
            return Err(span.error("invalid native type reference"));
        }
        if parameters.iter().any(|p| p.text == ty) || self.types.contains_key(ty) {
            return Ok(());
        }
        let Some((base, args)) = tuples::type_parts(ty) else {
            return Err(span.error(format!("unknown type '{ty}'")));
        };
        let arity = if let Some(arity) = self.external_type_arity(base) {
            Some(arity)
        } else if let Some(d) = self.generic_records.get(base) {
            Some(d.parameters.len())
        } else if let Some(d) = self.enum_definitions.get(base) {
            Some(d.parameters.len())
        } else {
            match base {
                "Vec" | "Iter" | "Set" => Some(1),
                "Dict" => Some(2),
                "Tuple" if args.len() >= 2 => Some(args.len()),
                _ => None,
            }
        }
        .or_else(|| {
            self.types.keys().find_map(|t| {
                tuples::type_parts(t)
                    .filter(|(b, _)| *b == base)
                    .map(|(_, a)| a.len())
            })
        });
        let Some(arity) = arity else {
            return Err(span.error(format!("unknown type constructor '{base}'")));
        };
        if parameters.iter().any(|p| p.text == base) || args.len() != arity {
            return Err(span.error(format!("type '{base}' expects {arity} type arguments")));
        }
        for arg in args {
            self.validate_symbolic_type(arg, parameters, span)?;
        }
        Ok(())
    }

    pub(super) fn validate_extern_declarations(&self) -> Result<(), String> {
        let validate_parts =
            |parts: Vec<Part>, parameters: &[Name], span: &Span| -> Result<(), String> {
                for part in parts {
                    match part {
                        Part::Type(ty) | Part::ReturnType(ty) | Part::Finish(ty) => {
                            self.validate_symbolic_type(&ty, parameters, span)?
                        }
                        Part::Invoke {
                            name,
                            arguments,
                            result,
                            ..
                        } => {
                            self.validate_symbolic_type(&name, parameters, span)?;
                            for ty in arguments.iter().chain(std::iter::once(&result)) {
                                self.validate_symbolic_type(ty, parameters, span)?;
                            }
                        }
                        Part::Call {
                            name,
                            arguments,
                            result,
                            ..
                        } => {
                            if let Some((_, args)) = tuples::type_parts(&name) {
                                for arg in args {
                                    self.validate_symbolic_type(arg, parameters, span)?;
                                }
                            }
                            for ty in arguments.iter().chain(std::iter::once(&result)) {
                                self.validate_symbolic_type(ty, parameters, span)?;
                            }
                        }
                        _ => {}
                    }
                }
                Ok(())
            };
        for definition in self.generic_extern_types.values() {
            self.validate_symbolic_type(
                &definition.pattern,
                &definition.parameters,
                &definition.span,
            )?;
            if let Some(kind) = &definition.representation {
                for child in type_metadata::references(kind) {
                    self.validate_symbolic_type(child, &definition.parameters, &definition.span)?;
                }
            }
            validate_parts(
                template::parse(&definition.native, false).map_err(|e| definition.span.error(e))?,
                &definition.parameters,
                &definition.span,
            )?;
        }
        for definition in &self.generic_functions {
            let Some(properties) = &definition.foreign else {
                continue;
            };
            for ty in definition
                .arguments
                .iter()
                .map(|(_, ty)| ty)
                .chain(definition.result.iter())
            {
                self.validate_symbolic_type(&ty.text, &definition.parameters, &ty.span)?;
            }
            for key in ["code", "lower", "lower_owned", "lower_borrowed"] {
                if let Some(text) = properties.get(key) {
                    validate_parts(
                        template::parse(text, true).map_err(|e| definition.name.span.error(e))?,
                        &definition.parameters,
                        &definition.name.span,
                    )?;
                }
            }
        }
        Ok(())
    }

    pub(super) fn render_native(
        &mut self,
        text: &str,
        bindings: &BTreeMap<String, String>,
        function: Option<&str>,
        span: &Span,
        body: bool,
        owner: Option<usize>,
    ) -> Result<String, String> {
        let mut output = String::new();
        for part in template::parse(text, body).map_err(|e| span.error(e))? {
            match part {
                Part::Text(text) => output.push_str(&text),
                Part::SelfName => output.push_str(
                    function
                        .ok_or_else(|| span.error("function reference outside native function"))?,
                ),
                Part::Type(ty) => {
                    let ty = generics::substitute(&ty, bindings);
                    self.check_type(&Name {
                        text: ty.clone(),
                        span: span.clone(),
                    })?;
                    output.push_str(&self.types[&ty]);
                }
                Part::ReturnType(ty) => {
                    let ty = generics::substitute(&ty, bindings);
                    self.check_type(&Name {
                        text: ty.clone(),
                        span: span.clone(),
                    })?;
                    output.push_str(&if ty == "Unit" {
                        "void".into()
                    } else {
                        format!("Obj<{}>", self.types[&ty])
                    });
                }
                Part::Finish(ty) => {
                    let ty = generics::substitute(&ty, bindings);
                    self.check_type(&Name {
                        text: ty.clone(),
                        span: span.clone(),
                    })?;
                    output.push_str(&format!(
                        "rusk_finish_return<{},{}>",
                        self.types[&ty],
                        ty == "Unit"
                    ));
                }
                Part::Invoke {
                    name,
                    arguments,
                    result,
                    kind,
                } => {
                    let ty = generics::substitute(&name, bindings);
                    let callback = self.callables.get(&ty).cloned().ok_or_else(|| {
                        span.error(format!("type '{ty}' is not statically callable"))
                    })?;
                    let arguments = arguments
                        .iter()
                        .map(|ty| generics::substitute(ty, bindings))
                        .collect::<Vec<_>>();
                    if callback.parameters != arguments {
                        return Err(span.error("native callback parameter type mismatch"));
                    }
                    self.same(
                        &generics::substitute(&result, bindings),
                        &callback.result,
                        span,
                    )?;
                    self.used.insert(callback.function);
                    if matches!(kind, template::InvokeKind::Writes) {
                        self.native_effect_references
                            .insert((callback.function, crate::effects::Query::Writes));
                        output.push_str(&format!("rusk_writes_{}", callback.function));
                    } else {
                        if let Some(owner) = owner {
                            self.native_contract_dependencies
                                .entry(owner)
                                .or_default()
                                .insert(callback.function);
                        }
                        output.push_str(&self.abi_reference(
                            callback.function,
                            matches!(kind, template::InvokeKind::Boxed),
                        ));
                    }
                }
                Part::Call {
                    name,
                    arguments,
                    result,
                    kind,
                } => {
                    let name = generics::substitute(&name, bindings);
                    let result = generics::substitute(&result, bindings);
                    self.check_type(&Name {
                        text: result.clone(),
                        span: span.clone(),
                    })?;
                    let mut values = vec![];
                    for ty in arguments {
                        let ty = generics::substitute(&ty, bindings);
                        self.check_type(&Name {
                            text: ty.clone(),
                            span: span.clone(),
                        })?;
                        values.push(Value {
                            ty,
                            id: ValueId::Probe,
                        });
                    }
                    let index = self
                        .resolve_callable(&name, &values, Some(&result), span)?
                        .ok_or_else(|| {
                            span.error(format!("no matching native callable '{name}'"))
                        })?;
                    self.same(&result, &self.functions[index].result, span)?;
                    if let template::CallKind::Effect(query) = kind {
                        // A semantic query does not call or take the address of
                        // a native definition, so it needs no target prototype.
                        self.native_effect_references.insert((index, query));
                        output.push_str(&format!("rusk_{}_{index}", query.name()));
                        continue;
                    }
                    if matches!(kind, template::CallKind::Value)
                        && result != "Unit"
                        && !self
                            .ir_types()
                            .get(&result)
                            .is_some_and(crate::hir::Type::scalar_value)
                    {
                        return Err(span.error(
                            "native value references require Unit or a declared scalar result",
                        ));
                    }
                    if self.functions[index].body.is_none()
                        && template::identifier(&self.functions[index].cpp)
                    {
                        self.native_references.insert(index);
                    }
                    self.used.insert(index);
                    if let Some(owner) = owner {
                        self.native_contract_dependencies
                            .entry(owner)
                            .or_default()
                            .insert(index);
                    }
                    match kind {
                        template::CallKind::Effect(query) => {
                            unreachable!("effect query handled above: {query:?}");
                        }
                        template::CallKind::Value => {
                            self.native_value_references.insert(index);
                            output.push_str(&format!("rusk_value_ref_{index}"));
                        }
                        template::CallKind::Owned => {
                            output.push_str(&self.abi_reference(index, false));
                        }
                        template::CallKind::Boxed => {
                            output.push_str(&self.abi_reference(index, true));
                        }
                    }
                }
            }
        }
        Ok(output)
    }
}
