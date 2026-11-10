use super::*;

impl Compiler {
    pub(super) fn read_function(
        &mut self,
        ty: &str,
        dimensions: usize,
        span: &Span,
    ) -> Result<usize, String> {
        let args = (0..dimensions)
            .map(|_| Value {
                ty: "Int".into(),
                id: ValueId::Probe,
            })
            .collect::<Vec<_>>();
        if let Some(index) = self.resolve("__read__", &args, Some(ty), span)? {
            return Ok(index);
        }
        self.check_type(&Name {
            text: ty.into(),
            span: span.clone(),
        })?;
        let kind = if vector_element(ty).is_some() {
            if dimensions == 0 {
                return Err(span.error("array input requires one length per array dimension"));
            }
            "vector_read"
        } else if ty.starts_with("Tuple<") && dimensions == 0 {
            "tuple_read"
        } else {
            return Err(span.error(format!(
                "no input reader for '{ty}' with {dimensions} dimensions"
            )));
        };
        let index = self.native_function(
            "__read__",
            vec!["Int".into(); dimensions],
            ty,
            |_| String::new(),
            span,
        )?;
        self.native_derivation_tasks
            .insert(index, (kind.into(), ty.into()));
        Ok(index)
    }
}
