use std::collections::BTreeMap;

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum Value {
    String(String),
    Bool(bool),
    Array(Vec<Self>),
    Table(BTreeMap<String, Self>),
}

pub(crate) fn statements(input: &str) -> Result<Vec<String>, String> {
    if input.contains("\"\"\"") || input.contains("'''") {
        return Err("multiline strings are unsupported".to_owned());
    }

    let mut statements = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    let mut escaped = false;
    let mut comment = false;
    let (mut square_depth, mut brace_depth) = (0usize, 0usize);

    for character in input.chars() {
        if comment {
            if character == '\n' {
                comment = false;
                finish_statement(&mut statements, &mut current, square_depth, brace_depth);
            }
            continue;
        }

        if let Some(delimiter) = quote {
            if character == '\n' {
                return Err("line breaks inside strings are unsupported".to_owned());
            }
            current.push(character);
            if delimiter == '"' && escaped {
                escaped = false;
            } else if delimiter == '"' && character == '\\' {
                escaped = true;
            } else if character == delimiter {
                quote = None;
            }
            continue;
        }

        match character {
            '#' => comment = true,
            '"' | '\'' => {
                quote = Some(character);
                current.push(character);
            }
            '[' => {
                square_depth += 1;
                current.push(character);
            }
            ']' => {
                square_depth = square_depth
                    .checked_sub(1)
                    .ok_or_else(|| "unmatched closing bracket".to_owned())?;
                current.push(character);
            }
            '{' => {
                brace_depth += 1;
                current.push(character);
            }
            '}' => {
                brace_depth = brace_depth
                    .checked_sub(1)
                    .ok_or_else(|| "unmatched closing brace".to_owned())?;
                current.push(character);
            }
            '\n' if brace_depth != 0 => {
                return Err("inline tables cannot span multiple lines".to_owned());
            }
            '\n' => finish_statement(&mut statements, &mut current, square_depth, brace_depth),
            _ => current.push(character),
        }
    }

    if quote.is_some() || square_depth != 0 || brace_depth != 0 {
        return Err("unterminated string, array, or inline table".to_owned());
    }
    finish_statement(&mut statements, &mut current, square_depth, brace_depth);
    Ok(statements)
}

pub(crate) fn parse_key_path(input: &str) -> Result<Vec<String>, String> {
    split_top_level(input, '.')?
        .into_iter()
        .map(parse_key)
        .collect()
}

pub(crate) fn split_assignment(input: &str) -> Result<(&str, &str), String> {
    let index = top_level_delimiter(input, '=')?
        .ok_or_else(|| "expected a key/value assignment".to_owned())?;
    let (key, value) = (input[..index].trim(), input[index + 1..].trim());
    if key.is_empty() || value.is_empty() {
        return Err("assignment key and value must be nonempty".to_owned());
    }
    Ok((key, value))
}

pub(crate) fn parse_value(input: &str) -> Result<Value, String> {
    let input = input.trim();
    if matches!(input.chars().next(), Some('"' | '\'')) {
        return parse_string(input).map(Value::String);
    }
    if input == "true" {
        return Ok(Value::Bool(true));
    }
    if input == "false" {
        return Ok(Value::Bool(false));
    }
    if input.starts_with('[') && input.ends_with(']') {
        return parse_collection(&input[1..input.len() - 1], ']').map(Value::Array);
    }
    if input.starts_with('{') && input.ends_with('}') {
        let mut table = BTreeMap::new();
        for item in collection_items(&input[1..input.len() - 1], '}')? {
            let (key, value) = split_assignment(item)?;
            let key = parse_single_key(key)?;
            let value = parse_value(value)?;
            if table.insert(key, value).is_some() {
                return Err("duplicate inline-table key".to_owned());
            }
        }
        return Ok(Value::Table(table));
    }
    Err("unsupported TOML value".to_owned())
}

fn finish_statement(
    statements: &mut Vec<String>,
    current: &mut String,
    square_depth: usize,
    brace_depth: usize,
) {
    if square_depth == 0 && brace_depth == 0 {
        let statement = current.trim();
        if !statement.is_empty() {
            statements.push(statement.to_owned());
        }
        current.clear();
    } else if !current.ends_with(' ') {
        current.push(' ');
    }
}

fn parse_collection(input: &str, closing: char) -> Result<Vec<Value>, String> {
    collection_items(input, closing)?
        .into_iter()
        .map(parse_value)
        .collect()
}

fn collection_items(input: &str, closing: char) -> Result<Vec<&str>, String> {
    let items = split_top_level(input, ',')?;
    let mut values = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let item = item.trim();
        if item.is_empty() {
            if index + 1 == items.len() {
                continue;
            }
            return Err(format!("empty item before `{closing}`"));
        }
        values.push(item);
    }
    Ok(values)
}

fn parse_single_key(input: &str) -> Result<String, String> {
    let path = parse_key_path(input)?;
    if path.len() != 1 {
        return Err("dotted assignment keys are unsupported".to_owned());
    }
    path.into_iter()
        .next()
        .ok_or_else(|| "empty key".to_owned())
}

fn parse_key(input: &str) -> Result<String, String> {
    let input = input.trim();
    if matches!(input.chars().next(), Some('"' | '\'')) {
        return parse_string(input);
    }
    if input.is_empty()
        || !input
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err("unsupported bare key".to_owned());
    }
    Ok(input.to_owned())
}

fn parse_string(input: &str) -> Result<String, String> {
    let delimiter = input
        .chars()
        .next()
        .ok_or_else(|| "empty string".to_owned())?;
    if input.len() < 2 || !input.ends_with(delimiter) {
        return Err("unterminated string".to_owned());
    }
    let content = &input[1..input.len() - 1];
    if delimiter == '"' && content.contains('\\') {
        return Err("escaped basic strings are unsupported".to_owned());
    }
    Ok(content.to_owned())
}

fn split_top_level(input: &str, delimiter: char) -> Result<Vec<&str>, String> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut offset = 0usize;
    while let Some(index) = top_level_delimiter(&input[offset..], delimiter)? {
        let absolute = offset + index;
        parts.push(&input[start..absolute]);
        start = absolute + delimiter.len_utf8();
        offset = start;
    }
    parts.push(&input[start..]);
    Ok(parts)
}

fn top_level_delimiter(input: &str, delimiter: char) -> Result<Option<usize>, String> {
    let mut quote = None;
    let mut escaped = false;
    let (mut square_depth, mut brace_depth) = (0usize, 0usize);

    for (index, character) in input.char_indices() {
        if let Some(string_delimiter) = quote {
            if string_delimiter == '"' && escaped {
                escaped = false;
            } else if string_delimiter == '"' && character == '\\' {
                escaped = true;
            } else if character == string_delimiter {
                quote = None;
            }
            continue;
        }

        match character {
            '"' | '\'' => quote = Some(character),
            '[' => square_depth += 1,
            ']' => {
                square_depth = square_depth
                    .checked_sub(1)
                    .ok_or_else(|| "unmatched closing bracket".to_owned())?;
            }
            '{' => brace_depth += 1,
            '}' => {
                brace_depth = brace_depth
                    .checked_sub(1)
                    .ok_or_else(|| "unmatched closing brace".to_owned())?;
            }
            _ if character == delimiter && square_depth == 0 && brace_depth == 0 => {
                return Ok(Some(index));
            }
            _ => {}
        }
    }

    if quote.is_some() || square_depth != 0 || brace_depth != 0 {
        return Err("unterminated nested TOML value".to_owned());
    }
    Ok(None)
}
