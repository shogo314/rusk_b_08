use super::*;

pub(super) fn type_parts(ty: &str) -> Option<(&str, Vec<&str>)> {
    let (base, rest) = ty.split_once('<')?;
    let inner = rest.strip_suffix('>')?;
    let mut depth = 0usize;
    let mut start = 0;
    let mut parts = vec![];
    for (index, c) in inner.char_indices() {
        match c {
            '<' => depth += 1,
            '>' => depth = depth.checked_sub(1)?,
            ',' if depth == 0 => {
                parts.push(&inner[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    if depth != 0 {
        return None;
    }
    parts.push(&inner[start..]);
    Some((base, parts))
}

impl Compiler {
    pub(super) fn specialize_tuple(&mut self, name: &Name) -> Result<(), String> {
        let (_, types) =
            type_parts(&name.text).ok_or_else(|| name.span.error("invalid tuple type"))?;
        if types.len() < 2 {
            return Err(name.span.error("tuple types require at least two elements"));
        }
        let mut fields = vec![];
        for (index, ty) in types.iter().enumerate() {
            let ty = Name {
                text: (*ty).into(),
                span: name.span.clone(),
            };
            self.check_type(&ty)?;
            fields.push((
                Name {
                    text: index.to_string(),
                    span: name.span.clone(),
                },
                ty,
            ));
        }
        self.types.insert(
            name.text.clone(),
            format!("RuskRecord{}", self.records.len()),
        );
        self.records.insert(
            name.text.clone(),
            records::Record {
                name: name.clone(),
                fields,
            },
        );
        self.prepare_record(&name.text)
    }

    pub(super) fn tuple_value(
        &mut self,
        items: &[Expr],
        expected: Option<&str>,
        span: &Span,
    ) -> Result<Value, String> {
        let hints = expected
            .and_then(type_parts)
            .filter(|(base, _)| *base == "Tuple")
            .map(|(_, args)| args);
        let mut values = vec![];
        for (index, item) in items.iter().enumerate() {
            values.push(self.expr(item, hints.as_ref().and_then(|h| h.get(index).copied()))?);
        }
        self.pack_tuple(values, span)
    }

    pub(super) fn tuple_item(
        &mut self,
        tuple: &Value,
        index: usize,
        span: &Span,
    ) -> Result<Value, String> {
        if !tuple.ty.starts_with("Tuple<") {
            return Err(span.error(format!("expected a tuple, found '{}'", tuple.ty)));
        }
        let ty = self.records[&tuple.ty]
            .fields
            .get(index)
            .ok_or_else(|| span.error("tuple index out of range"))?
            .1
            .text
            .clone();
        Ok(self.emit(
            &ty,
            Rvalue::Field {
                owner: tuple.clone(),
                index,
            },
        ))
    }

    pub(super) fn declare_binding(
        &mut self,
        binding: &Binding,
        value: Value,
    ) -> Result<(), String> {
        match binding {
            Binding::Name(name) if name.text == "_" => Ok(()),
            Binding::Name(name) => self.declare_annotated(name, value.ty.clone(), Some(value)),
            Binding::Tuple(bindings, span) => {
                if !value.ty.starts_with("Tuple<") {
                    return Err(span.error(format!("expected a tuple, found '{}'", value.ty)));
                }
                if bindings.len() != self.records[&value.ty].fields.len() {
                    return Err(span.error("tuple binding arity mismatch"));
                }
                for (index, binding) in bindings.iter().enumerate() {
                    let item = self.tuple_item(&value, index, binding.span())?;
                    self.declare_binding(binding, item)?;
                }
                Ok(())
            }
        }
    }

    fn predeclared_tuple_value(&self, ty: &str) -> bool {
        fn eligible(compiler: &Compiler, ty: &str, depth: usize) -> bool {
            use crate::ir::hir::{Scalar, Type};
            if depth >= 64 {
                return false;
            }
            if ty.starts_with("Tuple<") {
                return compiler.records.get(ty).is_some_and(|record| {
                    record
                        .fields
                        .iter()
                        .all(|(_, field)| eligible(compiler, &field.text, depth + 1))
                });
            }
            match compiler.native_types.get(ty).map(|metadata| &metadata.kind) {
                Some(Type::Scalar(Scalar::I64 | Scalar::Bool | Scalar::Char)) => true,
                Some(Type::Buffer(element) | Type::Vector(element)) => {
                    eligible(compiler, element, depth + 1)
                }
                _ => false,
            }
        }
        eligible(self, ty, 0)
    }

    pub(super) fn prepare_tuple_functions(
        &mut self,
        record: &records::Record,
    ) -> Result<(), String> {
        let ty = &record.name.text;
        if !ty.starts_with("Tuple<") || !self.predeclared_tuple_value(ty) {
            return Ok(());
        }
        // Preserve the existing concrete-operation priority for these tuples.
        // Other tuple writers are derived after ordinary overload resolution.
        self.tuple_write_function(ty, &record.name.span)?;
        for name in ["__eq__", "__ne__", "__lt__", "__le__", "__gt__", "__ge__"] {
            self.tuple_comparison_function(name, ty, &record.name.span)?;
        }
        Ok(())
    }

    pub(super) fn tuple_write_function(&mut self, ty: &str, span: &Span) -> Result<usize, String> {
        let index = self.native_function(
            "__write__",
            vec![ty.into()],
            "Unit",
            |_| String::new(),
            span,
        )?;
        self.native_derivation_tasks
            .insert(index, ("tuple_write".into(), ty.into()));
        Ok(index)
    }

    pub(super) fn tuple_comparison_function(
        &mut self,
        name: &str,
        ty: &str,
        span: &Span,
    ) -> Result<Option<usize>, String> {
        let kind = match name {
            "__eq__" => "tuple_eq",
            "__ne__" => "tuple_ne",
            "__lt__" => "tuple_lt",
            "__le__" => "tuple_le",
            "__gt__" => "tuple_gt",
            "__ge__" => "tuple_ge",
            _ => return Ok(None),
        };
        let index =
            self.native_function(name, vec![ty.into(); 2], "Bool", |_| String::new(), span)?;
        self.native_derivation_tasks
            .insert(index, (kind.into(), ty.into()));
        Ok(Some(index))
    }
}

pub(super) fn literal_index(index: &Expr) -> Result<usize, String> {
    match &index.kind {
        ExprKind::Literal(ty, text) if ty == "Int" => text
            .parse::<usize>()
            .map_err(|_| index.span.error("tuple index out of range")),
        _ => Err(index.span.error("tuple index must be an integer literal")),
    }
}

impl Compiler {
    pub(super) fn assign_tuple_value(
        &mut self,
        op: &str,
        left: &Expr,
        value: Value,
    ) -> Result<Value, String> {
        if value.is_never() {
            return Ok(Value::never());
        }

        if let ExprKind::Tuple(targets) = &left.kind {
            if !value.ty.starts_with("Tuple<") {
                return Err(left.span.error("tuple assignment requires a tuple value"));
            }
            if targets.len() != self.records[&value.ty].fields.len() {
                return Err(left.span.error("tuple assignment arity mismatch"));
            }
            let mut values = vec![];
            for (index, target) in targets.iter().enumerate() {
                let item = self.tuple_item(&value, index, &target.span)?;
                values.push(self.assign_tuple_value(op, target, item)?);
            }
            return self.pack_tuple(values, &left.span);
        }
        if op == "=" {
            let target = self.expr(left, None)?;
            self.same(&target.ty, &value.ty, &left.span)?;
            self.call("__assign__", vec![target.clone(), value], None, &left.span)?;
            return Ok(target);
        }
        match &left.kind {
            ExprKind::Var(name) => {
                let target = self.find_var(name)?;
                self.same(&target.ty, &value.ty, &left.span)?;
                self.check_fixed_rebind(&target, &left.span)?;
                self.body.push(Instruction::Rebind {
                    span: Some(left.span.clone()),
                    target: Place::Local(target.id),
                    value: value.clone(),
                });
                for scope in self.scopes.iter_mut().rev() {
                    if let Some(variable) = scope.get_mut(&name.text) {
                        variable.initialized = true;
                        break;
                    }
                }
                Ok(value)
            }
            ExprKind::Field(owner, name) => {
                let owner = self.expr(owner, None)?;
                let (index, ty) = self.field(&owner.ty, name)?;
                self.same(&ty, &value.ty, &left.span)?;
                self.body.push(Instruction::Rebind {
                    span: Some(left.span.clone()),
                    target: Place::Field { owner, index },
                    value: value.clone(),
                });
                Ok(value)
            }
            ExprKind::Index(owner, index) => {
                let owner = self.expr(owner, None)?;
                if owner.ty.starts_with("Tuple<") {
                    let position = literal_index(index)?;
                    let item = self.tuple_item(&owner, position, &index.span)?;
                    self.same(&item.ty, &value.ty, &left.span)?;
                    self.body.push(Instruction::Rebind {
                        span: Some(left.span.clone()),
                        target: Place::Field {
                            owner,
                            index: position,
                        },
                        value: value.clone(),
                    });
                    Ok(value)
                } else {
                    let index = self.expr(index, Some("Int"))?;
                    self.call("__setitem__", vec![owner, index, value], None, &left.span)
                }
            }
            _ => Err(left
                .span
                .error("rebinding requires a variable, field or index access")),
        }
    }

    fn pack_tuple(&mut self, values: Vec<Value>, span: &Span) -> Result<Value, String> {
        let name = Name {
            text: format!(
                "Tuple<{}>",
                values
                    .iter()
                    .map(|v| v.ty.clone())
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            span: span.clone(),
        };
        self.check_type(&name)?;
        Ok(self.emit(&name.text, Rvalue::Aggregate(values)))
    }
}
