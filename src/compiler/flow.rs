use super::*;
impl Compiler {
    pub(super) fn check_unreachable(&mut self, statement: &Stmt) -> Result<(), String> {
        let saved_body = std::mem::take(&mut self.context.body);
        let temps = std::mem::take(&mut self.context.pending_temps);
        let loops = self.context.loops.clone();
        let inferred = self.context.inferred_return.clone();
        self.context.diverged = false;
        let result = self.statement_inner(statement);
        self.context.body = saved_body;
        self.context.pending_temps = temps;
        self.context.loops = loops;
        self.context.inferred_return = inferred;
        self.context.diverged = true;
        result
    }
    pub(super) fn declare_dead_binding(
        &mut self,
        binding: &Binding,
        ty: &str,
    ) -> Result<(), String> {
        match binding {
            Binding::Name(name) => {
                if name.text != "_" {
                    if self.context.scopes.last().unwrap().contains_key(&name.text) {
                        return Err(name.span.error("duplicate variable"));
                    }
                    self.context.scopes.last_mut().unwrap().insert(
                        name.text.clone(),
                        Variable {
                            id: ValueId::Probe,
                            ty: ty.into(),
                            initialized: true,
                            access: name.access.as_ref().map(|annotation| {
                                crate::ir::hir::AccessInfo {
                                    name: name.text.clone(),
                                    declaration: name.span.clone(),
                                    annotation: annotation.clone(),
                                }
                            }),
                            binding: name.binding.as_ref().map(|annotation| {
                                crate::ir::hir::BindingInfo {
                                    name: name.text.clone(),
                                    declaration: name.span.clone(),
                                    annotation: annotation.clone(),
                                }
                            }),
                        },
                    );
                }
            }
            Binding::Tuple(items, _) => {
                let fields = self.records.get(ty).map(|r| r.fields.clone());
                for (i, item) in items.iter().enumerate() {
                    self.declare_dead_binding(
                        item,
                        fields
                            .as_ref()
                            .and_then(|f| f.get(i))
                            .map_or(crate::ir::hir::DIVERGED, |(_, t)| t.text.as_str()),
                    )?;
                }
            }
        }
        Ok(())
    }

    pub(super) fn loop_target(&self, label: Option<&Name>, span: &Span) -> Result<usize, String> {
        if let Some(label) = label {
            self.context
                .loops
                .iter()
                .rposition(|l| l.label.as_ref().is_some_and(|n| n.text == label.text))
                .ok_or_else(|| {
                    label
                        .span
                        .error(format!("unknown loop label '{}'", label.text))
                })
        } else {
            self.context
                .loops
                .len()
                .checked_sub(1)
                .ok_or_else(|| span.error("break or continue outside a loop"))
        }
    }
    pub(super) fn block_expression(
        &mut self,
        statements: &[Stmt],
        tail: Option<&Expr>,
        expected: Option<&str>,
        span: &Span,
    ) -> Result<Value, String> {
        let outer = std::mem::take(&mut self.context.pending_temps);
        self.context.scopes.push(BTreeMap::new());
        let mut result = Value::never();
        let slot = self.temp();
        let body = self.capture(|c| {
            for statement in statements {
                c.statement(statement)?;
            }
            if c.context.diverged
                && let Some(tail) = tail
            {
                c.statement(&Stmt::Expr(tail.clone()))?;
            }
            if !c.context.diverged {
                result = if let Some(tail) = tail {
                    c.expr(tail, expected)?
                } else {
                    c.literal("Unit", "", span)?
                };
                if !c.context.diverged {
                    if let Some(expected) = expected {
                        c.same(expected, &result.ty, span)?;
                    }
                    c.context.body.push(Instruction::Rebind {
                        span: None,
                        target: Place::Local(slot.clone()),
                        value: result.clone(),
                    });
                    c.release_temps();
                }
            }
            Ok(())
        })?;
        self.context.scopes.pop();
        self.context.pending_temps = outer;
        if self.context.diverged {
            self.context.body.push(Instruction::Scope(body));
            return Ok(Value::never());
        }
        let result = Value {
            id: slot,
            ty: result.ty,
        };
        self.context.body.push(Instruction::Declare {
            result: result.clone(),
            initial: None,
        });
        self.context.body.push(Instruction::Scope(body));
        self.context.pending_temps.push(result.id.clone());
        Ok(result)
    }
}
