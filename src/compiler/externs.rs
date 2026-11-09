//! Initial generic extern prototype: opaque native types and static native bodies.
use super::*;
mod template;
use template::Part;

#[derive(Clone)]
pub(super) struct ExternalType {
    parameters: Vec<Name>,
    native: String,
    span: Span,
}

impl Compiler {
    pub(super) fn register_extern_type(
        &mut self,
        name: &Name,
        parameters: &[Name],
        properties: &BTreeMap<String, String>,
    ) -> Result<(), String> {
        generics::parameters_valid(parameters)?;
        if name.text.starts_with("__Rusk")
            || tuples::type_parts(&name.text).is_some()
            || matches!(
                name.text.as_str(),
                "Vec" | "Tuple" | "Iter" | "Option" | "Dict" | "Set"
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
        validate(name, properties, &["cpp_name"])?;
        self.generic_extern_types.insert(
            name.text.clone(),
            ExternalType {
                parameters: parameters.to_vec(),
                native: property(name, properties, "cpp_name")?,
                span: name.span.clone(),
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
    ) -> Result<(), String> {
        // The prototype deliberately has no trusted effects or native-name override.
        validate(name, properties, &["code"])?;
        let parts = template::parse(&property(name, properties, "code")?, true)
            .map_err(|e| name.span.error(e))?;
        let callbacks = parts
            .into_iter()
            .filter_map(|part| match part {
                Part::Invoke {
                    name,
                    arguments,
                    result,
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
            callbacks,
        })
    }

    pub(super) fn specialize_extern_type(&mut self, name: &Name) -> Result<bool, String> {
        let Some((base, args)) = tuples::type_parts(&name.text) else {
            return Ok(false);
        };
        let Some(template) = self.generic_extern_types.get(base).cloned() else {
            return Ok(false);
        };
        if args.len() != template.parameters.len() {
            return Err(name.span.error(format!(
                "type '{base}' expects {} type arguments",
                template.parameters.len()
            )));
        }
        if self.preparing_extern_types.len() >= 64
            || !self.preparing_extern_types.insert(name.text.clone())
        {
            return Err(name.span.error("recursive generic extern type expansion"));
        }
        let result = (|| {
            let bindings = template
                .parameters
                .iter()
                .zip(args)
                .map(|(p, arg)| (p.text.clone(), arg.to_string()))
                .collect::<BTreeMap<_, _>>();
            for ty in bindings.values() {
                self.check_type(&Name {
                    text: ty.clone(),
                    span: name.span.clone(),
                })?;
            }
            let native =
                self.render_native(&template.native, &bindings, None, &template.span, false)?;
            self.types.insert(name.text.clone(), native);
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
        let arity = if let Some(d) = self.generic_extern_types.get(base) {
            Some(d.parameters.len())
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
                        Part::Type(ty) | Part::ReturnType(ty) => {
                            self.validate_symbolic_type(&ty, parameters, span)?
                        }
                        Part::Invoke {
                            name,
                            arguments,
                            result,
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
            validate_parts(
                template::parse(&property(&definition.name, properties, "code")?, true)
                    .map_err(|e| definition.name.span.error(e))?,
                &definition.parameters,
                &definition.name.span,
            )?;
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
                Part::Invoke {
                    name,
                    arguments,
                    result,
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
                    output.push_str(&self.functions[callback.function].cpp);
                }
                Part::Call {
                    name,
                    arguments,
                    result,
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
                        .resolve(&name, &values, Some(&result), span)?
                        .ok_or_else(|| {
                            span.error(format!("no matching native callable '{name}'"))
                        })?;
                    self.same(&result, &self.functions[index].result, span)?;
                    if self.functions[index].body.is_none() {
                        if !template::identifier(&self.functions[index].cpp) {
                            return Err(span.error(
                                "native callable references require a simple C++ function name",
                            ));
                        }
                        self.native_references.insert(index);
                    }
                    self.used.insert(index);
                    output.push_str(&self.functions[index].cpp);
                }
            }
        }
        Ok(output)
    }
}
