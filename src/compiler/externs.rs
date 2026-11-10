//! Initial generic extern prototype: opaque native types and static native bodies.
use super::*;
pub(super) mod native_types;
mod template;
pub(super) mod type_patterns;
use template::Part;

#[derive(Clone)]
pub(super) struct ExternalType {
    pattern: String,
    parameters: Vec<Name>,
    native: String,
    span: Span,
    representation: Option<crate::ir::hir::Type>,
}

impl Compiler {
    pub(super) fn register_implicit_call(
        &mut self,
        name: &Name,
        arity: usize,
        properties: &BTreeMap<String, String>,
    ) -> Result<(), String> {
        if let Some(value) = properties.get("implicit_call") {
            if value != "true" || arity != 0 || properties.contains_key("derive") {
                return Err(name
                    .span
                    .error("implicit_call requires true, no parameters, and an ordinary extern"));
            }
            self.implicit_calls.insert(name.text.clone());
        }
        Ok(())
    }

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
                self.native_info_mut(index).unit_object = true;
            }
        }
        Ok(())
    }
    pub(super) fn abi_reference_name(&self, index: usize, object: bool) -> String {
        if self.functions[index].result != "Unit" || object == self.native_unit_object(index) {
            self.functions[index].cpp.clone()
        } else {
            (if object {
                crate::native::abi::UnitBridge::Box
            } else {
                crate::native::abi::UnitBridge::Discard
            })
            .name(index)
        }
    }
    pub(super) fn abi_reference(&mut self, index: usize, object: bool) -> String {
        if self.functions[index].result == "Unit" && object != self.native_unit_object(index) {
            self.native_requests.unit_bridges.insert((
                index,
                if object {
                    crate::native::abi::UnitBridge::Box
                } else {
                    crate::native::abi::UnitBridge::Discard
                },
            ));
            if self.functions[index].body.is_none()
                && template::identifier(&self.functions[index].cpp)
            {
                self.native_requests.declarations.insert(index);
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
        if self.types.contains_key(&name.text)
            || self.generic_records.contains_key(&name.text)
            || self.generic_extern_types.contains_key(&name.text)
        {
            return Err(name.span.error("duplicate type"));
        }
        validate(name, properties, &["cpp_name", "representation", "retains"])?;
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
                "aliases",
                "lower",
                "lower_owned",
                "lower_borrowed",
                "lower_rooted",
                "operation",
                "return_abi",
                "derive",
                "implicit_call",
            ],
        )?;
        self.register_implicit_call(name, arguments.len(), properties)?;
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
        if let Some(aliases) = properties.get("aliases") {
            if origin != Origin::StandardLibrary {
                return Err(name.span.error(
                    "native alias contracts are reserved for the embedded standard library",
                ));
            }
            if !properties.contains_key("effects") {
                return Err(name.span.error("native alias contract requires effects"));
            }
            crate::native::effects::aliases::parse(aliases, arguments.len())
                .map_err(|e| name.span.error(e))?;
        }
        self.validate_lowering_source(name, properties, origin)?;
        if let Some(descriptor) = properties
            .get("operation")
            .filter(|s| s.starts_with("enum_"))
        {
            super::enum_operations::parse(descriptor).map_err(|e| name.span.error(e))?;
        }

        property(name, properties, "code")?;
        let mut parts = Vec::new();
        for key in [
            "code",
            "lower",
            "lower_owned",
            "lower_borrowed",
            "lower_rooted",
        ] {
            if let Some(text) = properties.get(key) {
                parts.extend(template::parse(text, true).map_err(|e| name.span.error(e))?);
            }
        }
        if origin != Origin::StandardLibrary && template::has_mappings(&parts) {
            return Err(name
                .span
                .error("native call mappings are reserved for the embedded standard library"));
        }
        let mut calls = vec![];
        let mut callbacks = vec![];
        if !properties.contains_key("derive") {
            let actual = arguments
                .iter()
                .map(|(_, ty)| ty.text.clone())
                .collect::<Vec<_>>();
            let mut budget = 4096;
            for part in &parts {
                match part {
                    Part::Call {
                        name: target,
                        arguments,
                        result,
                        ..
                    } => {
                        calls.push(generics::CallRequirement {
                            name: target.clone(),
                            arguments: template::expand_arguments(arguments, &actual, &mut budget)
                                .map_err(|e| name.span.error(e))?,
                            result: result.clone(),
                        });
                    }
                    Part::Invoke {
                        name: target,
                        arguments,
                        result,
                        ..
                    } => {
                        callbacks.push(generics::CallbackRequirement {
                            ty: target.clone(),
                            parameters: template::expand_arguments(arguments, &actual, &mut budget)
                                .map_err(|e| name.span.error(e))?,
                            result: result.clone(),
                        });
                    }
                    _ => {}
                }
            }
        }
        for (argument, _) in arguments {
            if argument.binding.is_some() || argument.access.is_some() || argument.escape.is_some()
            {
                return Err(argument
                    .span
                    .error("modifiers on extern parameters require a Rusk function body"));
            }
        }
        let definition = generics::GenericFunction {
            name: name.clone(),
            parameters: parameters.to_vec(),
            arguments: arguments.to_vec(),
            result: result.clone(),
            body: None,
            foreign: Some(properties.clone()),
            origin,
            callbacks,
            calls,
        };
        if let Some(kind) = properties.get("derive") {
            if origin != Origin::StandardLibrary {
                return Err(name.span.error(
                    "native derivation definitions are reserved for the embedded standard library",
                ));
            }
            generics::parameters_valid(parameters)?;
            let assignment = matches!(kind.as_str(), "record_assign" | "enum_assign");
            let output = kind == "tuple_write";
            let comparison = matches!(
                kind.as_str(),
                "tuple_eq" | "tuple_ne" | "tuple_lt" | "tuple_le" | "tuple_gt" | "tuple_ge"
            );
            let vector_input = kind == "vector_read";
            let input = kind == "tuple_read" || vector_input;
            let valid_kind = assignment
                || input
                || output
                || comparison
                || matches!(kind.as_str(), "record_copy" | "enum_copy");
            if !valid_kind
                || parameters.len() != 1
                || arguments.len()
                    != if assignment || comparison {
                        2
                    } else if input {
                        0
                    } else {
                        1
                    }
                || arguments
                    .iter()
                    .any(|(_, ty)| ty.text != parameters[0].text)
                || result.as_ref().is_none_or(|r| {
                    if vector_input {
                        let mut bindings = BTreeMap::new();
                        !generics::unify(&r.text, &r.text, parameters, &mut bindings)
                            || !bindings.contains_key(&parameters[0].text)
                    } else {
                        r.text
                            != if comparison {
                                "Bool"
                            } else if assignment || output {
                                "Unit"
                            } else {
                                &parameters[0].text
                            }
                    }
                })
                || ((assignment || output) && parameters[0].text == "Unit")
                || (comparison && parameters[0].text == "Bool")
                || properties.keys().any(|key| {
                    !matches!(key.as_str(), "code" | "derive")
                        && !((output || comparison)
                            && matches!(
                                key.as_str(),
                                "effects"
                                    | "lower"
                                    | "lower_owned"
                                    | "lower_borrowed"
                                    | "lower_rooted"
                            ))
                })
            {
                return Err(name.span.error(if assignment {
                    "invalid native assignment derivation; expected one type parameter and (T,T)->Unit"
                }else if comparison {
                    "invalid native comparison derivation; expected one type parameter and (T,T)->Bool"
                }else if output {
                    "invalid native output derivation; expected one type parameter and (T)->Unit"
                }else if vector_input {
                    "invalid native input derivation; expected one type parameter bound by the result pattern and no declared arguments"
                }else if input {
                    "invalid native input derivation; expected one type parameter and ()->T"
                }else{
                    "invalid native copy derivation; expected one type parameter and (T)->T"
                }));
            }
            if self
                .native_derivations
                .insert(kind.clone(), definition)
                .is_some()
            {
                return Err(name.span.error("duplicate native derivation definition"));
            }
            Ok(())
        } else {
            self.register_generic_function(definition)
        }
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
            let parts =
                template::parse_type(&template.native).map_err(|e| template.span.error(e))?;
            let native = if parts.iter().any(|part| matches!(part, Part::Call { .. })) {
                if self.native_type_bindings.len() >= 512 {
                    return Err(name
                        .span
                        .error("native type reference specialization limit exceeded"));
                }
                let alias = format!("RuskNativeType{}", self.native_type_bindings.len());
                self.native_type_bindings.insert(
                    name.text.clone(),
                    native_types::Binding {
                        alias: alias.clone(),
                        template: template.native.clone(),
                        bindings: bindings.clone(),
                        span: template.span.clone(),
                        request: name.span.clone(),
                        native: None,
                        references: BTreeSet::new(),
                    },
                );
                alias
            } else {
                self.rendering_native_types.push(name.text.clone());
                let native = self.render_native(
                    &template.native,
                    &bindings,
                    None,
                    &template.span,
                    false,
                    None,
                );
                self.rendering_native_types.pop();
                native?
            };
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
        } else if base == "Tuple" && args.len() >= 2 {
            Some(args.len())
        } else {
            None
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

    fn validate_scoped_native_type(
        &self,
        ty: &str,
        parameters: &[Name],
        indices: &BTreeSet<String>,
        span: &Span,
    ) -> Result<(), String> {
        if ty
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .any(|word| indices.contains(word))
        {
            return Err(span.error("native field index cannot be used as a type"));
        }
        self.validate_symbolic_type(ty, parameters, span)
    }

    fn validate_native_parts(
        &self,
        parts: Vec<Part>,
        parameters: &[Name],
        indices: &BTreeSet<String>,
        span: &Span,
    ) -> Result<(), String> {
        for part in parts {
            match part {
                Part::IfType {
                    ty,
                    pattern,
                    parameters: names,
                    matched,
                    otherwise,
                } => {
                    self.validate_scoped_native_type(&ty, parameters, indices, span)?;
                    let mut scoped = parameters.to_vec();
                    let mut bound = BTreeMap::new();
                    for name in &names {
                        if parameters.iter().any(|p| &p.text == name) || indices.contains(name) {
                            return Err(span.error(format!(
                                "native type binding '{name}' shadows an existing binding"
                            )));
                        }
                        scoped.push(Name {
                            text: name.clone(),
                            span: span.clone(),
                        });
                    }
                    self.validate_scoped_native_type(&pattern, &scoped, indices, span)?;
                    let locals = names
                        .iter()
                        .map(|name| Name {
                            text: name.clone(),
                            span: span.clone(),
                        })
                        .collect::<Vec<_>>();
                    generics::unify(&pattern, &pattern, &locals, &mut bound);
                    if names.iter().any(|name| !bound.contains_key(name)) {
                        return Err(span.error("native type binding is not bound by the pattern"));
                    }
                    self.validate_native_parts(matched, &scoped, indices, span)?;
                    self.validate_native_parts(otherwise, parameters, indices, span)?;
                }
                Part::Index(index) => {
                    if !indices.contains(&index) {
                        return Err(span.error(format!("unbound native field index '{index}'")));
                    }
                }
                Part::Arguments {
                    element,
                    index,
                    parts,
                    ..
                } => {
                    for name in [&element, &index] {
                        if parameters.iter().any(|p| &p.text == name) || indices.contains(name) {
                            return Err(span.error(format!(
                                "native arguments binding '{name}' shadows an existing binding"
                            )));
                        }
                    }
                    let mut parameters = parameters.to_vec();
                    parameters.push(Name {
                        text: element,
                        span: span.clone(),
                    });
                    let mut indices = indices.clone();
                    indices.insert(index);
                    self.validate_native_parts(parts, &parameters, &indices, span)?;
                }
                Part::Variants { ty, index, parts } => {
                    self.validate_scoped_native_type(&ty, parameters, indices, span)?;
                    if parameters.iter().any(|p| p.text == index) || indices.contains(&index) {
                        return Err(span.error(format!(
                            "native variants binding '{index}' shadows an existing binding"
                        )));
                    }
                    let mut indices = indices.clone();
                    indices.insert(index);
                    self.validate_native_parts(parts, parameters, &indices, span)?;
                }
                Part::Fields {
                    ty,
                    variant,
                    element,
                    index,
                    parts,
                    tuple_only: _,
                } => {
                    self.validate_scoped_native_type(&ty, parameters, indices, span)?;
                    if let Some(variant) = variant
                        && variant.parse::<usize>().is_err()
                        && !indices.contains(&variant)
                    {
                        return Err(span.error(format!("unbound native variant index '{variant}'")));
                    }
                    for name in [&element, &index] {
                        if parameters.iter().any(|p| &p.text == name) || indices.contains(name) {
                            return Err(span.error(format!(
                                "native fields binding '{name}' shadows an existing binding"
                            )));
                        }
                    }
                    let mut parameters = parameters.to_vec();
                    parameters.push(Name {
                        text: element,
                        span: span.clone(),
                    });
                    let mut indices = indices.clone();
                    indices.insert(index);
                    self.validate_native_parts(parts, &parameters, &indices, span)?;
                }
                Part::Type(ty)
                | Part::NonUnitType(ty)
                | Part::ReturnType(ty)
                | Part::Finish(ty) => {
                    self.validate_scoped_native_type(&ty, parameters, indices, span)?
                }
                Part::Invoke {
                    name,
                    arguments,
                    result,
                    ..
                } => {
                    self.validate_scoped_native_type(&name, parameters, indices, span)?;
                    for ty in &arguments {
                        if template::argument_offset(ty).is_none() {
                            self.validate_scoped_native_type(ty, parameters, indices, span)?;
                        }
                    }
                    self.validate_scoped_native_type(&result, parameters, indices, span)?;
                }
                Part::Call {
                    name,
                    arguments,
                    result,
                    ..
                } => {
                    if let Some((_, args)) = tuples::type_parts(&name) {
                        for arg in args {
                            self.validate_scoped_native_type(arg, parameters, indices, span)?;
                        }
                    }
                    for ty in &arguments {
                        if template::argument_offset(ty).is_none() {
                            self.validate_scoped_native_type(ty, parameters, indices, span)?;
                        }
                    }
                    self.validate_scoped_native_type(&result, parameters, indices, span)?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    pub(super) fn validate_extern_declarations(&self) -> Result<(), String> {
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
            self.validate_native_parts(
                template::parse_type(&definition.native).map_err(|e| definition.span.error(e))?,
                &definition.parameters,
                &BTreeSet::new(),
                &definition.span,
            )?;
        }
        for definition in self
            .generic_functions
            .iter()
            .chain(self.native_derivations.values())
        {
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
            for key in [
                "code",
                "lower",
                "lower_owned",
                "lower_borrowed",
                "lower_rooted",
            ] {
                if let Some(text) = properties.get(key) {
                    let parts =
                        template::parse(text, true).map_err(|e| definition.name.span.error(e))?;
                    if properties
                        .get("derive")
                        .is_none_or(|kind| kind != "vector_read")
                    {
                        template::validate_argument_bounds(&parts, definition.arguments.len())
                            .map_err(|e| definition.name.span.error(e))?;
                    }
                    self.validate_native_parts(
                        parts,
                        &definition.parameters,
                        &BTreeSet::new(),
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
        let parts = if body {
            template::parse(text, true)
        } else {
            template::parse_type(text)
        }
        .map_err(|e| span.error(e))?;
        self.render_native_parts(parts, bindings, function, span, owner, &mut 4096)
    }

    fn expand_native_arguments(
        &self,
        arguments: &[String],
        bindings: &BTreeMap<String, String>,
        owner: Option<usize>,
        budget: &mut usize,
        span: &Span,
    ) -> Result<Vec<String>, String> {
        // Substitute explicit types before splicing. Concrete argument types must
        // never be interpreted as local binder names a second time.
        let patterns = arguments
            .iter()
            .map(|ty| {
                if template::argument_offset(ty).is_some() {
                    ty.clone()
                } else {
                    generics::substitute(ty, bindings)
                }
            })
            .collect::<Vec<_>>();
        let actual = owner
            .map(|id| self.functions[id].params.as_slice())
            .unwrap_or_default();
        template::expand_arguments(&patterns, actual, budget).map_err(|e| span.error(e))
    }

    fn render_native_parts(
        &mut self,
        parts: Vec<Part>,
        bindings: &BTreeMap<String, String>,
        function: Option<&str>,
        span: &Span,
        owner: Option<usize>,
        budget: &mut usize,
    ) -> Result<String, String> {
        let mut output = String::new();
        for part in parts {
            if *budget == 0 {
                return Err(span.error("native field expansion work limit exceeded"));
            }
            *budget -= 1;
            match part {
                Part::Else => return Err(span.error("unexpected native type branch")),
                Part::IfType {
                    ty,
                    pattern,
                    parameters,
                    matched,
                    otherwise,
                } => {
                    if parameters.len() > *budget {
                        return Err(span.error("native field expansion work limit exceeded"));
                    }
                    *budget -= parameters.len();
                    let ty = generics::substitute(&ty, bindings);
                    // Alpha-rename the pattern's variables before inserting outer
                    // concrete types. A concrete type with the same spelling as
                    // a local binder must remain a concrete type.
                    let renames = parameters
                        .iter()
                        .enumerate()
                        .map(|(i, name)| (name.clone(), format!("${i}")))
                        .collect::<BTreeMap<_, _>>();
                    let pattern =
                        generics::substitute(&generics::substitute(&pattern, &renames), bindings);
                    let locals = parameters
                        .iter()
                        .enumerate()
                        .map(|(i, _)| Name {
                            text: format!("${i}"),
                            span: span.clone(),
                        })
                        .collect::<Vec<_>>();
                    let mut inferred = BTreeMap::new();
                    let matches = generics::unify(&pattern, &ty, &locals, &mut inferred);
                    let mut scoped = bindings.clone();
                    if matches {
                        for (i, name) in parameters.into_iter().enumerate() {
                            scoped.insert(name, inferred[&format!("${i}")].clone());
                        }
                    }
                    output.push_str(&self.render_native_parts(
                        if matches { matched } else { otherwise },
                        &scoped,
                        function,
                        span,
                        owner,
                        budget,
                    )?);
                }
                Part::Index(index) => {
                    let value = bindings
                        .get(&index)
                        .filter(|v| v.parse::<usize>().is_ok())
                        .ok_or_else(|| {
                            span.error(format!("unbound native field index '{index}'"))
                        })?;
                    output.push_str(value);
                }
                Part::Arguments {
                    offset,
                    element,
                    index,
                    parts,
                } => {
                    let owner = owner
                        .ok_or_else(|| span.error("argument reference outside native function"))?;
                    let arguments = self.functions[owner].params.clone();
                    if offset > arguments.len() {
                        return Err(span.error("native argument offset out of range"));
                    }
                    for (i, ty) in arguments.into_iter().enumerate().skip(offset) {
                        if *budget == 0 {
                            return Err(span.error("native field expansion work limit exceeded"));
                        }
                        *budget -= 1;
                        let mut scoped = bindings.clone();
                        scoped.insert(element.clone(), ty);
                        scoped.insert(index.clone(), i.to_string());
                        output.push_str(&self.render_native_parts(
                            parts.clone(),
                            &scoped,
                            function,
                            span,
                            Some(owner),
                            budget,
                        )?);
                    }
                }
                Part::Variants { ty, index, parts } => {
                    let ty = generics::substitute(&ty, bindings);
                    self.check_type(&Name {
                        text: ty.clone(),
                        span: span.clone(),
                    })?;
                    let count = self
                        .enums
                        .get(&ty)
                        .ok_or_else(|| {
                            span.error(format!(
                                "native variants reference requires an enum; got '{ty}'"
                            ))
                        })?
                        .variants
                        .len();
                    for i in 0..count {
                        if *budget == 0 {
                            return Err(span.error("native field expansion work limit exceeded"));
                        }
                        *budget -= 1;
                        let mut scoped = bindings.clone();
                        scoped.insert(index.clone(), i.to_string());
                        output.push_str(&self.render_native_parts(
                            parts.clone(),
                            &scoped,
                            function,
                            span,
                            owner,
                            budget,
                        )?);
                    }
                }
                Part::Fields {
                    ty,
                    variant,
                    element,
                    index,
                    parts,
                    tuple_only,
                } => {
                    let ty = generics::substitute(&ty, bindings);
                    self.check_type(&Name {
                        text: ty.clone(),
                        span: span.clone(),
                    })?;
                    if tuple_only && !ty.starts_with("Tuple<") {
                        return Err(
                            span.error(format!("native tuple fields require a Tuple; got '{ty}'"))
                        );
                    }
                    if self.callables.contains_key(&ty)
                        && !owner
                            .is_some_and(|id| self.functions[id].origin == Origin::StandardLibrary)
                    {
                        return Err(span.error("callable environment fields are private"));
                    }
                    let fields = if let Some(variant) = variant {
                        let variant = bindings
                            .get(&variant)
                            .unwrap_or(&variant)
                            .parse::<usize>()
                            .map_err(|_| span.error("unbound native variant index"))?;
                        self.enums
                            .get(&ty)
                            .ok_or_else(|| {
                                span.error(format!(
                                    "native payload reference requires an enum; got '{ty}'"
                                ))
                            })?
                            .variants
                            .get(variant)
                            .ok_or_else(|| span.error("native variant index out of range"))?
                            .1
                            .clone()
                    } else {
                        self.records
                            .get(&ty)
                            .ok_or_else(|| {
                                span.error(format!(
                                    "native fields reference requires a struct or Tuple; got '{ty}'"
                                ))
                            })?
                            .fields
                            .iter()
                            .map(|(_, field)| field.clone())
                            .collect()
                    };
                    // Each iteration has a lexical binding scope. Expansion
                    // reuses ordinary typed references, including dependencies.
                    for (i, field) in fields.into_iter().enumerate() {
                        if *budget == 0 {
                            return Err(span.error("native field expansion work limit exceeded"));
                        }
                        *budget -= 1;
                        let mut scoped = bindings.clone();
                        scoped.insert(element.clone(), field.text.clone());
                        scoped.insert(index.clone(), i.to_string());
                        let rendered = self
                            .render_native_parts(
                                parts.clone(),
                                &scoped,
                                function,
                                span,
                                owner,
                                budget,
                            )
                            .map_err(|error| {
                                format!(
                                    "{error}\nwhile expanding field {i} of '{ty}' at {}:{}:{}",
                                    field.span.file, field.span.line, field.span.column
                                )
                            })?;
                        output.push_str(&rendered);
                    }
                }
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
                    if owner.is_none()
                        && function.is_none()
                        && let Some(current) = self.rendering_native_types.last()
                    {
                        self.native_type_dependencies
                            .entry(current.clone())
                            .or_default()
                            .insert(ty.clone());
                    }
                    output.push_str(&self.types[&ty]);
                }
                Part::NonUnitType(ty) => {
                    let ty = generics::substitute(&ty, bindings);
                    self.check_type(&Name {
                        text: ty.clone(),
                        span: span.clone(),
                    })?;
                    if ty == "Unit" {
                        return Err(span.error(
                            "native nonunit reference requires a value type other than Unit",
                        ));
                    }
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
                    route,
                } => {
                    let ty = generics::substitute(&name, bindings);
                    let callback = self.callables.get(&ty).cloned().ok_or_else(|| {
                        span.error(format!("type '{ty}' is not statically callable"))
                    })?;
                    let arguments =
                        self.expand_native_arguments(&arguments, bindings, owner, budget, span)?;
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
                        self.native_requests
                            .effects
                            .insert((callback.function, crate::analysis::effects::Query::Writes));
                        output.push_str(&format!("rusk_writes_{}", callback.function));
                    } else {
                        if let Some(owner) = owner {
                            self.record_native_dependency(
                                owner,
                                callback.function,
                                route.as_ref(),
                                span,
                            )?;
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
                    route,
                } => {
                    let name = generics::substitute(&name, bindings);
                    let result = generics::substitute(&result, bindings);
                    self.check_type(&Name {
                        text: result.clone(),
                        span: span.clone(),
                    })?;
                    let arguments =
                        self.expand_native_arguments(&arguments, bindings, owner, budget, span)?;
                    let mut values = vec![];
                    for ty in arguments {
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
                            if values.is_empty()
                                && name == result
                                && self.types.contains_key(&result)
                            {
                                span.error(format!(
                                    "type '{result}' has no zero-argument constructor"
                                ))
                            } else {
                                span.error(format!("no matching native callable '{name}'"))
                            }
                        })?;
                    self.same(&result, &self.functions[index].result, span)?;
                    if owner.is_none()
                        && function.is_none()
                        && let Some(current) = self.rendering_native_types.last()
                        && let Some(binding) = self.native_type_bindings.get_mut(current)
                    {
                        binding.references.insert(index);
                    }
                    if let template::CallKind::Effect(query) = kind {
                        // A semantic query does not call or take the address of
                        // a native definition, so it needs no target prototype.
                        self.native_requests.effects.insert((index, query));
                        output.push_str(&format!("rusk_{}_{index}", query.name()));
                        continue;
                    }
                    if matches!(kind, template::CallKind::Value)
                        && result != "Unit"
                        && !self
                            .ir_types()
                            .get(&result)
                            .is_some_and(crate::ir::hir::Type::scalar_value)
                    {
                        return Err(span.error(
                            "native value references require Unit or a declared scalar result",
                        ));
                    }
                    if self.functions[index].body.is_none()
                        && template::identifier(&self.functions[index].cpp)
                    {
                        self.native_requests.declarations.insert(index);
                    }
                    self.used.insert(index);
                    if let Some(owner) = owner {
                        self.record_native_dependency(owner, index, route.as_ref(), span)?;
                    }
                    match kind {
                        template::CallKind::Effect(query) => {
                            unreachable!("effect query handled above: {query:?}");
                        }
                        template::CallKind::Value => {
                            self.native_requests.values.insert(index);
                            output.push_str(&format!("rusk_value_ref_{index}"));
                        }
                        template::CallKind::Rooted => {
                            self.native_requests.rooted.insert(index);
                            self.abi_reference(index, true);
                            output.push_str(&format!("rusk_rooted_ref_{index}"));
                        }
                        template::CallKind::Object => {
                            self.native_requests.owned.insert(index);
                            self.abi_reference(index, true);
                            output.push_str(&format!("rusk_owned_ref_{index}"));
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

#[cfg(test)]
mod tests;
