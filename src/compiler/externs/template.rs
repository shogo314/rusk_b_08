//! Explicit native references, independent of C++ token spelling.
use super::super::tuples::type_parts;

#[derive(Clone, Copy)]
pub(super) enum CallKind {
    Owned,
    Boxed,
    Object,
    Rooted,
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
    // Only used while parsing a conditional; removed before returning its AST.
    Else,
    IfType {
        ty: String,
        pattern: String,
        parameters: Vec<String>,
        matched: Vec<Part>,
        otherwise: Vec<Part>,
    },
    Arguments {
        offset: usize,
        element: String,
        index: String,
        parts: Vec<Part>,
    },
    Variants {
        ty: String,
        index: String,
        parts: Vec<Part>,
    },
    Fields {
        ty: String,
        variant: Option<String>,
        tuple_only: bool,
        element: String,
        index: String,
        parts: Vec<Part>,
    },
    Index(String),
    Text(String),
    Type(String),
    NonUnitType(String),
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

// Whitespace is removed from callable signatures before parsing their types.
pub(super) fn argument_offset(text: &str) -> Option<usize> {
    let offset = text.strip_prefix("...arguments")?;
    if offset.is_empty() || !offset.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    offset.parse().ok()
}

pub(super) fn expand_arguments(
    patterns: &[String],
    actual: &[String],
    budget: &mut usize,
) -> Result<Vec<String>, String> {
    let mut result = vec![];
    for pattern in patterns {
        let types = if let Some(offset) = argument_offset(pattern) {
            actual
                .get(offset..)
                .ok_or("native argument offset out of range")?
        } else {
            std::slice::from_ref(pattern)
        };
        if types.len() > *budget {
            return Err("native field expansion work limit exceeded".into());
        }
        *budget -= types.len();
        result.extend_from_slice(types);
    }
    Ok(result)
}

pub(super) fn validate_argument_bounds(parts: &[Part], count: usize) -> Result<(), String> {
    for part in parts {
        match part {
            Part::IfType {
                matched, otherwise, ..
            } => {
                validate_argument_bounds(matched, count)?;
                validate_argument_bounds(otherwise, count)?;
            }
            Part::Arguments { offset, parts, .. } => {
                if *offset > count {
                    return Err("native argument offset out of range".into());
                }
                validate_argument_bounds(parts, count)?;
            }
            Part::Fields { parts, .. } | Part::Variants { parts, .. } => {
                validate_argument_bounds(parts, count)?;
            }
            Part::Call { arguments, .. } | Part::Invoke { arguments, .. }
                if arguments
                    .iter()
                    .filter_map(|a| argument_offset(a))
                    .any(|n| n > count) =>
            {
                return Err("native argument offset out of range".into());
            }
            _ => {}
        }
    }
    Ok(())
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
    if body && let Some(ty) = text.strip_prefix("nonunit ") {
        if !valid_type(ty) {
            return Err("invalid native nonunit type reference".into());
        }
        return Ok(Part::NonUnitType(ty.into()));
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
            .or_else(|| text.strip_prefix("rooted_call "))
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
            let (_, types) =
                type_parts(&pattern).ok_or("invalid native callable parameter types")?;
            if types
                .iter()
                .any(|ty| !valid_type(ty) && argument_offset(ty).is_none())
            {
                return Err("invalid native callable parameter types".into());
            }
            types.into_iter().map(String::from).collect()
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
                    } else if text.starts_with("rooted_call ") {
                        CallKind::Rooted
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
    parse_parts(&mut &text[..], body, 0, &mut 4096, false)
}

// Type expressions can name a scalar ABI adapter without admitting function
// body constructs or implicit argument lists into a C++ type expression.
pub(super) fn parse_type(mut text: &str) -> Result<Vec<Part>, String> {
    let mut parts = vec![];
    while let Some(start) = text.find('{') {
        if parts.len() >= 4096 {
            return Err("native template reference limit exceeded".into());
        }
        let explicit = text[..start].ends_with('@');
        let literal = &text[..start - usize::from(explicit)];
        if !literal.is_empty() {
            parts.push(Part::Text(literal.into()));
        }
        let end = text[start..]
            .find('}')
            .ok_or("unterminated native type reference")?
            + start;
        let part = reference(&text[start + 1..end], explicit)?;
        match &part {
            Part::Type(_) => {}
            Part::Call {
                kind: CallKind::Value,
                arguments,
                ..
            } if arguments.iter().all(|a| argument_offset(a).is_none()) => {}
            _ => {
                return Err(
                    "native type expressions allow types and explicit value_call references".into(),
                );
            }
        }
        parts.push(part);
        text = &text[end + 1..];
    }
    if !text.is_empty() {
        parts.push(Part::Text(text.into()));
    }
    Ok(parts)
}

fn parse_parts(
    rest: &mut &str,
    body: bool,
    depth: usize,
    budget: &mut usize,
    allow_else: bool,
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
        if body && header == "else" {
            if !allow_else || parts.iter().any(|p| matches!(p, Part::Else)) {
                return Err("native else requires a type condition and can occur only once".into());
            }
            parts.push(Part::Else);
            continue;
        }
        if body && let Some(header) = header.strip_prefix("if_type ") {
            let (ty, pattern) = header.split_once(" matches ")
                .ok_or("invalid native type condition; expected if_type Type matches Pattern [with Parameters]")?;
            let (pattern, parameters) =
                if let Some((pattern, parameters)) = pattern.split_once(" with ") {
                    (
                        pattern.trim(),
                        parameters
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .collect::<Vec<_>>(),
                    )
                } else {
                    (pattern.trim(), vec![])
                };
            if !valid_type(ty.trim())
                || !valid_type(pattern)
                || parameters.iter().any(|p| !identifier(p))
                || parameters
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    != parameters.len()
            {
                return Err("invalid native type condition or bindings".into());
            }
            if parameters.len() > *budget {
                return Err("native template reference limit exceeded".into());
            }
            *budget -= parameters.len();
            let mut matched = parse_parts(rest, body, depth + 1, budget, true)?;
            let otherwise = if let Some(at) = matched.iter().position(|p| matches!(p, Part::Else)) {
                let otherwise = matched.split_off(at + 1);
                matched.pop();
                otherwise
            } else {
                vec![]
            };
            parts.push(Part::IfType {
                ty: ty.trim().into(),
                pattern: pattern.into(),
                parameters,
                matched,
                otherwise,
            });
        } else if body && let Some(header) = header.strip_prefix("arguments ") {
            let (prefix, binders) = header.split_once("as ").ok_or(
                "invalid native arguments reference; expected arguments [from N] as Type,Index",
            )?;
            let offset = if prefix.trim().is_empty() {
                0
            } else {
                let offset = prefix
                    .trim()
                    .strip_prefix("from ")
                    .ok_or("invalid native argument offset")?;
                if offset.is_empty() || !offset.bytes().all(|b| b.is_ascii_digit()) {
                    return Err("invalid native argument offset".into());
                }
                offset
                    .parse::<usize>()
                    .map_err(|_| "invalid native argument offset")?
            };
            let (element, index) = binders
                .split_once(',')
                .ok_or("invalid native arguments bindings")?;
            let (element, index) = (element.trim(), index.trim());
            if !identifier(element) || !identifier(index) || element == index {
                return Err("invalid native arguments bindings".into());
            }
            parts.push(Part::Arguments {
                offset,
                element: element.into(),
                index: index.into(),
                parts: parse_parts(rest, body, depth + 1, budget, false)?,
            });
        } else if body && let Some(header) = header.strip_prefix("variants ") {
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
                parts: parse_parts(rest, body, depth + 1, budget, false)?,
            });
        } else if body && let Some(header) = header.strip_prefix("fields ") {
            let (ty, binders) = header
                .split_once(" as ")
                .ok_or("invalid native fields reference; expected fields Type as Element,Index")?;
            let (element, index) = binders
                .split_once(',')
                .ok_or("invalid native fields bindings")?;
            let (ty, element, index) = (ty.trim(), element.trim(), index.trim());
            let tuple_only = ty.ends_with(" tuple");
            let ty = ty.strip_suffix(" tuple").unwrap_or(ty).trim();
            let (ty, variant) = if let Some((ty, variant)) = ty.split_once(" variant ") {
                if tuple_only {
                    return Err("native tuple fields cannot select a variant".into());
                }
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
                tuple_only,
                element: element.into(),
                index: index.into(),
                parts: parse_parts(rest, body, depth + 1, budget, false)?,
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
