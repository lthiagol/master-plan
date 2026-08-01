#[cfg(test)]
use std::collections::BTreeMap;

use anyhow::{bail, Context, Result};
use serde_json::Value;

pub fn project_fields(value: &Value, paths: &[String]) -> Result<Value> {
    let mut result = Value::Object(serde_json::Map::new());
    for path in paths {
        let trimmed = path.trim();
        if trimmed.is_empty() {
            continue;
        }
        let projected = project_single_path(value, trimmed)?;
        merge_value(&mut result, &projected)?;
    }
    Ok(result)
}

fn project_single_path(value: &Value, path: &str) -> Result<Value> {
    let segments = parse_path(path)?;
    project_recursive(value, &segments, 0, path)
}

fn project_recursive(
    value: &Value,
    segments: &[Segment],
    idx: usize,
    full_path: &str,
) -> Result<Value> {
    if idx >= segments.len() {
        return Ok(value.clone());
    }

    let seg = &segments[idx];

    match seg {
        Segment::Field(name) => match value {
            Value::Object(map) => match map.get(name.as_str()) {
                Some(inner) => {
                    let projected = project_recursive(inner, segments, idx + 1, full_path)?;
                    let mut obj = serde_json::Map::new();
                    obj.insert(name.clone(), projected);
                    Ok(Value::Object(obj))
                }
                None => bail!("unknown path: '{full_path}' — field '{name}' not found"),
            },
            _ => bail!("unknown path: '{full_path}' — expected object for field '{name}'"),
        },
        Segment::Index(i) => match value {
            Value::Array(arr) => match arr.get(*i) {
                Some(inner) => {
                    let projected = project_recursive(inner, segments, idx + 1, full_path)?;
                    let mut obj = serde_json::Map::new();
                    obj.insert(i.to_string(), projected);
                    Ok(Value::Object(obj))
                }
                None => bail!(
                    "unknown path: '{full_path}' — index {i} out of bounds (len={})",
                    arr.len()
                ),
            },
            _ => bail!("unknown path: '{full_path}' — expected array for index [{i}]"),
        },
        Segment::AllElements => match value {
            Value::Array(arr) => {
                let remaining = &segments[idx + 1..];
                let mut projected = Vec::new();
                for (item_idx, item) in arr.iter().enumerate() {
                    match project_recursive(item, remaining, 0, full_path) {
                        Ok(v) => projected.push(v),
                        Err(e) => {
                            bail!("unknown path: '{full_path}' — at index {item_idx}: {e:?}")
                        }
                    }
                }
                Ok(Value::Array(projected))
            }
            _ => bail!("unknown path: '{full_path}' — expected array for []"),
        },
        Segment::IdSelector(id) => match value {
            Value::Array(arr) => {
                // Stable-id selector (M93 AC-03): pick the element whose `id` field
                // matches. Used for acceptance_criteria[AC-03], steps[S4], work_packages[WP1].
                let mut matched: Option<(String, &Value)> = None;
                for item in arr {
                    let candidate = match item.get("id").and_then(|v| v.as_str()) {
                        Some(s) => s.to_string(),
                        None => continue,
                    };
                    if candidate == *id {
                        matched = Some((candidate, item));
                        break;
                    }
                }
                let (id_key, inner) = matched.with_context(|| {
                    format!(
                        "unknown path: '{full_path}' — no element with id '{id}' (array len={})",
                        arr.len()
                    )
                })?;
                let projected = project_recursive(inner, segments, idx + 1, full_path)?;
                let mut obj = serde_json::Map::new();
                obj.insert(id_key, projected);
                Ok(Value::Object(obj))
            }
            _ => bail!("unknown path: '{full_path}' — expected array for [{id}]"),
        },
    }
}

fn merge_value(dest: &mut Value, src: &Value) -> Result<()> {
    if !dest.is_object() || !src.is_object() {
        *dest = src.clone();
        return Ok(());
    }

    let dest_map = dest.as_object_mut().unwrap();
    let src_map = src.as_object().unwrap();

    for (key, src_val) in src_map {
        match dest_map.get_mut(key) {
            Some(dest_val) => {
                if dest_val.is_array() && src_val.is_array() {
                    let dest_arr = dest_val.as_array_mut().unwrap();
                    let src_arr = src_val.as_array().unwrap();
                    if dest_arr.len() != src_arr.len() {
                        bail!(
                            "cannot merge arrays of different lengths for key '{}' ({} vs {})",
                            key,
                            dest_arr.len(),
                            src_arr.len()
                        );
                    }
                    for (i, sv) in src_arr.iter().enumerate() {
                        merge_value(&mut dest_arr[i], sv)?;
                    }
                } else {
                    merge_value(dest_val, src_val)?;
                }
            }
            None => {
                dest_map.insert(key.clone(), src_val.clone());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
pub fn sort_value(value: &mut Value) {
    match value {
        Value::Object(map) => {
            let sorted: BTreeMap<String, Value> = map
                .iter()
                .map(|(k, v)| {
                    let mut val = v.clone();
                    sort_value(&mut val);
                    (k.clone(), val)
                })
                .collect();
            *map = sorted.into_iter().collect();
        }
        Value::Array(arr) => {
            for item in arr {
                sort_value(item);
            }
        }
        _ => {}
    }
}

#[derive(Debug, Clone)]
enum Segment {
    Field(String),
    Index(usize),
    AllElements,
    /// Stable-id selector (M93 AC-03): e.g. `acceptance_criteria[AC-03]`,
    /// `steps[S4]`, `work_packages[WP1]`. Matches an array element whose
    /// `id` field equals the selector string.
    IdSelector(String),
}

fn parse_path(path: &str) -> Result<Vec<Segment>> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut chars = path.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '.' => {
                if !current.is_empty() {
                    segments.push(Segment::Field(std::mem::take(&mut current)));
                }
            }
            '[' => {
                if !current.is_empty() {
                    segments.push(Segment::Field(std::mem::take(&mut current)));
                }
                let mut bracket_content = String::new();
                loop {
                    match chars.next() {
                        Some(']') => break,
                        Some(c) => bracket_content.push(c),
                        None => bail!("unclosed bracket in path: {path}"),
                    }
                }
                if bracket_content.is_empty() {
                    segments.push(Segment::AllElements);
                } else if let Ok(idx) = bracket_content.parse::<usize>() {
                    // Pure-numeric brackets stay as Index for backward compatibility
                    // with the M79 numeric-index projection contract.
                    segments.push(Segment::Index(idx));
                } else {
                    // Non-numeric (e.g. AC-03, S4, WP1) becomes a stable-id selector.
                    segments.push(Segment::IdSelector(bracket_content));
                }
            }
            _ => {
                current.push(ch);
            }
        }
    }
    if !current.is_empty() {
        segments.push(Segment::Field(current));
    }
    Ok(segments)
}
