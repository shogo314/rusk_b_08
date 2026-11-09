//! Explicit native references, independent of C++ token spelling.
use super::super::tuples::type_parts;

#[derive(Clone, Copy)]
pub(super) enum CallKind {
    Owned,
    Boxed,
    Value,
    Effect(crate::effects::Query),
}
#[derive(Clone, Copy)]
pub(super) enum InvokeKind {
    Normal,
    Boxed,
    Writes,
}

pub(super) enum Part {
    Text(String),
    Type(String),
    ReturnType(String),
    Finish(String),
    SelfName,
    Call {
        name: String,
        arguments: Vec<String>,
        result: String,
        kind: CallKind,
    },
    Invoke {
        name: String,
        arguments: Vec<String>,
        result: String,
        kind: InvokeKind,
    },
}

pub(super) fn identifier(text: &str) -> bool {
    let mut bytes = text.bytes();
    bytes
        .next()
        .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

pub(super) fn valid_type(text: &str) -> bool {
    fn check(text: &str, depth: usize) -> bool {
        if depth > 64 {
            return false;
        }
        match type_parts(text) {
            Some((base, args)) => identifier(base) && args.iter().all(|a| check(a, depth + 1)),
            None => identifier(text),
        }
    }
    check(text, 0)
}

fn reference(text: &str, body: bool) -> Result<Part, String> {
    if body && text == "fn" {
        return Ok(Part::SelfName);
    }
    if body && let Some(ty) = text.strip_prefix("return ") {
        if !valid_type(ty) {
            return Err("invalid native return type reference".into());
        }
        return Ok(Part::ReturnType(ty.into()));
    }
    if body && let Some(ty) = text.strip_prefix("finish ") {
        if !valid_type(ty) {
            return Err("invalid native return value reference".into());
        }
        return Ok(Part::Finish(ty.into()));
    }
    if body
        && let Some(call) = text
            .strip_prefix("call ")
            .or_else(|| text.strip_prefix("boxed_call "))
            .or_else(|| text.strip_prefix("value_call "))
            .or_else(|| text.strip_prefix("writes "))
            .or_else(|| text.strip_prefix("fails "))
            .or_else(|| text.strip_prefix("invoke "))
            .or_else(|| text.strip_prefix("boxed_invoke "))
            .or_else(|| text.strip_prefix("writes_invoke "))
    {
        let call = call
            .chars()
            .filter(|c| !c.is_ascii_whitespace())
            .collect::<String>();
        let (signature, result) = call
            .rsplit_once(")->")
            .ok_or("invalid native callable reference; expected name(Types)->Type")?;
        let (name, params) = signature
            .split_once('(')
            .ok_or("invalid native callable reference")?;
        let name = name.replace("::<", "<");
        if !valid_type(&name) || !valid_type(result) {
            return Err("invalid native callable reference".into());
        }
        let arguments = if params.is_empty() {
            vec![]
        } else {
            let pattern = format!("Args<{params}>");
            if !valid_type(&pattern) {
                return Err("invalid native callable parameter types".into());
            }
            type_parts(&pattern)
                .unwrap()
                .1
                .into_iter()
                .map(String::from)
                .collect()
        };
        return Ok(
            if text.starts_with("invoke ")
                || text.starts_with("writes_invoke ")
                || text.starts_with("boxed_invoke ")
            {
                Part::Invoke {
                    name,
                    arguments,
                    result: result.into(),
                    kind: if text.starts_with("writes_invoke ") {
                        InvokeKind::Writes
                    } else if text.starts_with("boxed_invoke ") {
                        InvokeKind::Boxed
                    } else {
                        InvokeKind::Normal
                    },
                }
            } else {
                Part::Call {
                    name,
                    arguments,
                    result: result.into(),
                    kind: if text.starts_with("value_call ") {
                        CallKind::Value
                    } else if text.starts_with("boxed_call ") {
                        CallKind::Boxed
                    } else if text.starts_with("writes ") {
                        CallKind::Effect(crate::effects::Query::Writes)
                    } else if text.starts_with("fails ") {
                        CallKind::Effect(crate::effects::Query::Fails)
                    } else {
                        CallKind::Owned
                    },
                }
            },
        );
    }
    if !valid_type(text) {
        return Err("invalid native type reference".into());
    }
    Ok(Part::Type(text.into()))
}

pub(super) fn parse(text: &str, body: bool) -> Result<Vec<Part>, String> {
    let mut parts = vec![];
    let mut literal = String::new();
    let mut rest = text;
    let marker = if body { "@" } else { "{" };
    while let Some(at) = rest.find(marker) {
        literal.push_str(&rest[..at]);
        rest = &rest[at..];
        if body && rest.starts_with("@@") {
            literal.push('@');
            rest = &rest[2..];
            continue;
        }
        let prefix = if body { "@{" } else { "{" };
        if !rest.starts_with(prefix) {
            literal.push('@');
            rest = &rest[1..];
            continue;
        }
        let end = rest.find('}').ok_or("unterminated native type reference")?;
        if !literal.is_empty() {
            parts.push(Part::Text(std::mem::take(&mut literal)));
        }
        parts.push(reference(&rest[prefix.len()..end], body)?);
        rest = &rest[end + 1..];
    }
    literal.push_str(rest);
    if !literal.is_empty() {
        parts.push(Part::Text(literal));
    }
    Ok(parts)
}
