use super::*;

impl Compiler {
    pub(super) fn native_function(
        &mut self,
        name: &str,
        params: Vec<String>,
        result: &str,
        code: impl FnOnce(&str) -> String,
        span: &Span,
    ) -> Result<usize, String> {
        self.check_type(&Name {
            text: result.into(),
            span: span.clone(),
        })?;
        let index = self.functions.len();
        let cpp = format!("rusk_algorithm_{index}");
        let code = code(&cpp);
        self.functions.push(Function {
            name: Name {
                text: name.into(),
                span: span.clone(),
            },
            params,
            result: result.into(),
            cpp,
            code,
            body: None,
            origin: Origin::Generated,
            checked: None,
        });
        Ok(index)
    }
}
