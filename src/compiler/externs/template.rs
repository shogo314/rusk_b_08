//! Explicit native references, independent of C++ token spelling.
use super::super::tuples::type_parts;

#[derive(Clone, Copy)]
pub(super) enum CallKind {
    Owned,
    Boxed,
    Object,
    Value,
    Effect(crate::effects::Query),
}
#[derive(Clone, Copy)]
pub(super) enum InvokeKind {
    Normal,
    Boxed,
    Writes,
}

#[derive(Clone)]
pub(super) enum Part {
    Variants {
        ty: String,
        index: String,
        parts: Vec<Part>,
    },
    Fields {
        ty: String,
        variant: Option<String>,
        element: String,
        index: String,
        parts: Vec<Part>,
    },
    Index(String),
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
    if body && let Some(index) = text.strip_prefix("index ") {
        if !identifier(index) {
            return Err("invalid native field index reference".into());
        }
        return Ok(Part::Index(index.into()));
    }
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
            .or_else(|| text.strip_prefix("owned_call "))
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
                    } else if text.starts_with("owned_call ") {
                        CallKind::Object
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
    parse_parts(&mut &text[..], body, 0, &mut 4096)
}

fn parse_parts(
    rest: &mut &str,
    body: bool,
    depth: usize,
    budget: &mut usize,
) -> Result<Vec<Part>, String> {
    if depth > 64 {
        return Err("native field expansion nesting limit exceeded".into());
    }
    let mut parts = vec![];
    let mut literal = String::new();
    let marker = if body { "@" } else { "{" };
    while let Some(at) = rest.find(marker) {
        literal.push_str(&rest[..at]);
        *rest = &rest[at..];
        if body && rest.starts_with("@@") {
            literal.push('@');
            *rest = &rest[2..];
            continue;
        }
        let prefix = if body { "@{" } else { "{" };
        if !rest.starts_with(prefix) {
            literal.push('@');
            *rest = &rest[1..];
            continue;
        }
        let end = rest.find('}').ok_or("unterminated native type reference")?;
        if !literal.is_empty() {
            parts.push(Part::Text(std::mem::take(&mut literal)));
        }
        let header = &rest[prefix.len()..end];
        *rest = &rest[end + 1..];
        if *budget == 0 {
            return Err("native template reference limit exceeded".into());
        }
        *budget -= 1;
        if body && header == "end" {
            return if depth > 0 {
                Ok(parts)
            } else {
                Err("native field expansion end without fields".into())
            };
        }
        if body && let Some(header) = header.strip_prefix("variants ") {
            let (ty, index) = header
                .split_once(" as ")
                .ok_or("invalid native variants reference; expected variants Type as Index")?;
            let (ty, index) = (ty.trim(), index.trim());
            if !valid_type(ty) || !identifier(index) {
                return Err("invalid native variants binding or type".into());
            }
            parts.push(Part::Variants {
                ty: ty.into(),
                index: index.into(),
                parts: parse_parts(rest, body, depth + 1, budget)?,
            });
        } else if body && let Some(header) = header.strip_prefix("fields ") {
            let (ty, binders) = header
                .split_once(" as ")
                .ok_or("invalid native fields reference; expected fields Type as Element,Index")?;
            let (element, index) = binders
                .split_once(',')
                .ok_or("invalid native fields bindings")?;
            let (ty, element, index) = (ty.trim(), element.trim(), index.trim());
            let (ty, variant) = if let Some((ty, variant)) = ty.split_once(" variant ") {
                let variant = variant.trim();
                if !identifier(variant) && variant.parse::<usize>().is_err() {
                    return Err("invalid native variant index".into());
                }
                (ty.trim(), Some(variant.to_string()))
            } else {
                (ty, None)
            };
            if !valid_type(ty) || !identifier(element) || !identifier(index) || element == index {
                return Err("invalid native fields bindings or type".into());
            }
            parts.push(Part::Fields {
                ty: ty.into(),
                variant,
                element: element.into(),
                index: index.into(),
                parts: parse_parts(rest, body, depth + 1, budget)?,
            });
        } else {
            parts.push(reference(header, body)?);
        }
    }
    if depth > 0 {
        return Err("unterminated native field expansion".into());
    }
    literal.push_str(rest);
    *rest = "";
    if !literal.is_empty() {
        parts.push(Part::Text(literal));
    }
    Ok(parts)
}
