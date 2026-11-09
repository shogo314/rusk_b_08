//! Trusted native expression templates. Arguments refer to evaluated IR slots.
#[derive(Clone)]
pub(crate) enum Piece {
    Text(String),
    Argument(usize),
    Location,
}

#[derive(Clone)]
pub(crate) struct Template(Vec<Piece>);
impl Template {
    pub(crate) fn parse(text: &str, arity: usize) -> Result<Self, String> {
        let mut pieces = vec![];
        let mut rest = text;
        let mut literal = String::new();
        while let Some(at) = rest.find('$') {
            literal.push_str(&rest[..at]);
            rest = &rest[at + 1..];
            if let Some(tail) = rest.strip_prefix('$') {
                literal.push('$');
                rest = tail;
                continue;
            }
            if !literal.is_empty() {
                pieces.push(Piece::Text(std::mem::take(&mut literal)));
            }
            if let Some(tail) = rest.strip_prefix("loc") {
                pieces.push(Piece::Location);
                rest = tail;
                continue;
            }
            let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
            if digits == 0 {
                return Err("invalid native lowering reference".into());
            }
            let index = rest[..digits]
                .parse::<usize>()
                .map_err(|_| "invalid native lowering argument")?;
            if index >= arity {
                return Err("native lowering argument out of range".into());
            }
            pieces.push(Piece::Argument(index));
            rest = &rest[digits..];
        }
        literal.push_str(rest);
        if !literal.is_empty() {
            pieces.push(Piece::Text(literal));
        }
        Ok(Self(pieces))
    }

    pub(crate) fn render(&self, args: &[String], location: &str) -> String {
        self.0
            .iter()
            .map(|p| match p {
                Piece::Text(text) => text.as_str(),
                Piece::Argument(index) => args[*index].as_str(),
                Piece::Location => location,
            })
            .collect()
    }
}

#[derive(Clone, Default)]
pub(crate) struct Lowering {
    pub scalar: Option<Template>,
    pub owned: Option<Template>,
    pub borrowed: Option<Template>,
    pub assignment: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn templates_use_explicit_evaluated_slots_and_preserve_literals() {
        let template = Template::parse("checked(*$0 + *$1, $loc) + $$cash", 2).unwrap();
        assert_eq!(
            template.render(&["left".into(), "right".into()], "call_site"),
            "checked(*left + *right, call_site) + $cash"
        );
        for text in ["$2", "$", "$missing", "$99999999999999999999999999999"] {
            assert!(Template::parse(text, 2).is_err());
        }
    }
}
