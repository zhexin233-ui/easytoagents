fn parse_document(
    target: &TargetDescriptor,
    raw: ObservedRaw,
) -> Result<ObservedDocument, AppError> {
    match (target.format, raw) {
        (TargetFormat::Json, ObservedRaw::File(bytes)) => {
            if bytes.iter().all(u8::is_ascii_whitespace) {
                return Err(AppError::parse(
                    target.path_for_error(),
                    target.format.as_str(),
                ));
            }
            let value = serde_json::from_slice::<Value>(&bytes).map_err(|error| {
                AppError::parse(target.path_for_error(), target.format.as_str()).with_source(error)
            })?;
            if !value.is_object() {
                return Err(AppError::parse(
                    target.path_for_error(),
                    target.format.as_str(),
                ));
            }
            Ok(ObservedDocument::Json(value))
        }
        (TargetFormat::Jsonc, ObservedRaw::File(bytes)) => {
            if bytes.iter().all(u8::is_ascii_whitespace) {
                return Err(AppError::parse(
                    target.path_for_error(),
                    target.format.as_str(),
                ));
            }
            let source = String::from_utf8(bytes).map_err(|error| {
                AppError::parse(target.path_for_error(), target.format.as_str()).with_source(error)
            })?;
            let value = parse_jsonc(&source).map_err(|error| {
                AppError::parse(target.path_for_error(), target.format.as_str()).with_source(error)
            })?;
            if !value.is_object() {
                return Err(AppError::parse(
                    target.path_for_error(),
                    target.format.as_str(),
                ));
            }
            Ok(ObservedDocument::Jsonc { value, source })
        }
        (TargetFormat::Toml, ObservedRaw::File(bytes)) => {
            let text = std::str::from_utf8(&bytes).map_err(|error| {
                AppError::parse(target.path_for_error(), target.format.as_str()).with_source(error)
            })?;
            if text.trim().is_empty() {
                return Err(AppError::parse(
                    target.path_for_error(),
                    target.format.as_str(),
                ));
            }
            let document = text.parse::<DocumentMut>().map_err(|error| {
                AppError::parse(target.path_for_error(), target.format.as_str()).with_source(error)
            })?;
            let semantic = toml_edit::de::from_str::<Value>(text).map_err(|error| {
                AppError::parse(target.path_for_error(), target.format.as_str()).with_source(error)
            })?;
            Ok(ObservedDocument::Toml { document, semantic })
        }
        (TargetFormat::Markdown, ObservedRaw::File(bytes)) => {
            let text = String::from_utf8(bytes).map_err(|error| {
                AppError::parse(target.path_for_error(), target.format.as_str()).with_source(error)
            })?;
            Ok(ObservedDocument::Markdown(text))
        }
        (TargetFormat::CursorMdc, ObservedRaw::File(bytes)) => {
            let text = String::from_utf8(bytes).map_err(|error| {
                AppError::parse(target.path_for_error(), target.format.as_str()).with_source(error)
            })?;
            Ok(ObservedDocument::Markdown(strip_mdc_frontmatter(&text)))
        }
        (TargetFormat::SymlinkDirectory, ObservedRaw::Directory(entries)) => {
            Ok(ObservedDocument::SymlinkDirectory(entries))
        }
        _ => Err(AppError::parse(
            target.path_for_error(),
            target.format.as_str(),
        )),
    }
}

/// Parse the stable OpenCode JSONC surface without treating comments as data.
/// This small lexer intentionally accepts only JSON plus comments/trailing
/// commas; JSON5 extensions such as unquoted keys are rejected fail-closed.
pub(crate) fn parse_jsonc(source: &str) -> Result<Value, serde_json::Error> {
    let stripped = strip_jsonc_comments(source);
    let normalized = strip_jsonc_trailing_commas(&stripped);
    let mut deserializer = serde_json::Deserializer::from_str(&normalized);
    let value = StrictJsonValue::deserialize(&mut deserializer)?.0;
    deserializer.end()?;
    Ok(value)
}

struct StrictJsonValue(Value);

impl<'de> Deserialize<'de> for StrictJsonValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(StrictJsonValueVisitor)
    }
}

struct StrictJsonValueVisitor;

impl<'de> serde::de::Visitor<'de> for StrictJsonValueVisitor {
    type Value = StrictJsonValue;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a JSON value")
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(StrictJsonValue(Value::Bool(value)))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(StrictJsonValue(Value::Number(value.into())))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(StrictJsonValue(Value::Number(value.into())))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        serde_json::Number::from_f64(value)
            .map(|number| StrictJsonValue(Value::Number(number)))
            .ok_or_else(|| E::custom("JSON number must be finite"))
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(StrictJsonValue(Value::String(value.to_owned())))
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(StrictJsonValue(Value::String(value)))
    }

    fn visit_none<E>(self) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(StrictJsonValue(Value::Null))
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(StrictJsonValue(Value::Null))
    }

    fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        StrictJsonValue::deserialize(deserializer)
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        let mut values = Vec::new();
        while let Some(value) = sequence.next_element::<StrictJsonValue>()? {
            values.push(value.0);
        }
        Ok(StrictJsonValue(Value::Array(values)))
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        let mut object = Map::new();
        while let Some(key) = map.next_key::<String>()? {
            let value = map.next_value::<StrictJsonValue>()?;
            if object.insert(key, value.0).is_some() {
                return Err(serde::de::Error::custom("duplicate JSON object key"));
            }
        }
        Ok(StrictJsonValue(Value::Object(object)))
    }
}

fn strip_jsonc_comments(source: &str) -> String {
    let mut output = String::with_capacity(source.len());
    let bytes = source.as_bytes();
    let mut index = 0;
    let mut in_string = false;
    let mut escaped = false;
    while index < bytes.len() {
        // `index` 总是落在字符边界上（按 `len_utf8` 推进）；万一不是，跳到下一字节
        // 而不是 panic：JSONC 剥离只是解析前的宽松预处理，解析器会报告真正的错误。
        let Some(character) = source.get(index..).and_then(|rest| rest.chars().next()) else {
            index += 1;
            continue;
        };
        let width = character.len_utf8();
        if in_string {
            output.push(character);
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
            index += width;
            continue;
        }
        if character == '"' {
            in_string = true;
            output.push(character);
            index += width;
            continue;
        }
        if character == '/' && bytes.get(index + 1) == Some(&b'/') {
            index += 2;
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
            output.push('\n');
            continue;
        }
        if character == '/' && bytes.get(index + 1) == Some(&b'*') {
            index += 2;
            while index + 1 < bytes.len() && !(bytes[index] == b'*' && bytes[index + 1] == b'/') {
                if bytes[index] == b'\n' {
                    output.push('\n');
                }
                index += 1;
            }
            if index + 1 < bytes.len() {
                index += 2;
            }
            continue;
        }
        output.push(character);
        index += width;
    }
    output
}

fn strip_jsonc_trailing_commas(source: &str) -> String {
    let mut output = String::with_capacity(source.len());
    let bytes = source.as_bytes();
    let mut index = 0;
    let mut in_string = false;
    let mut escaped = false;
    while index < bytes.len() {
        // `index` 总是落在字符边界上（按 `len_utf8` 推进）；万一不是，跳到下一字节
        // 而不是 panic：JSONC 剥离只是解析前的宽松预处理，解析器会报告真正的错误。
        let Some(character) = source.get(index..).and_then(|rest| rest.chars().next()) else {
            index += 1;
            continue;
        };
        let width = character.len_utf8();
        if in_string {
            output.push(character);
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
            index += width;
            continue;
        }
        if character == '"' {
            in_string = true;
            output.push(character);
            index += width;
            continue;
        }
        if character == ',' {
            let mut next = index + 1;
            while next < bytes.len() && bytes[next].is_ascii_whitespace() {
                next += 1;
            }
            if matches!(bytes.get(next), Some(b'}' | b']')) {
                index += 1;
                continue;
            }
        }
        output.push(character);
        index += width;
    }
    output
}

fn replace_jsonc_roots(source: &str, desired: &Value, roots: &[String]) -> String {
    let all_roots = desired
        .as_object()
        .map(|object| object.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    replace_jsonc_roots_with_rendered(
        source,
        desired,
        if roots.is_empty() { &all_roots } else { roots },
    )
}

fn replace_jsonc_roots_with_rendered(source: &str, desired: &Value, roots: &[String]) -> String {
    let mut output = source.to_owned();
    for root in roots {
        let Some(value) = desired.get(root) else {
            continue;
        };
        let rendered = serde_json::to_string_pretty(value).unwrap_or_else(|_| "null".to_owned());
        if let Some((start, end)) = jsonc_top_level_value_span(&output, root) {
            let rendered = preserve_jsonc_comments(&rendered, &output[start..end]);
            output.replace_range(start..end, &rendered);
        } else if let Some(close) = output.rfind('}') {
            let semantic = strip_jsonc_comments(&output[..close]);
            let needs_comma = semantic
                .find('{')
                .and_then(|open| semantic.get(open + 1..))
                .is_some_and(|body| body.chars().any(|character| !character.is_whitespace()));
            let insertion = if needs_comma {
                format!(",\n  \"{root}\": {rendered}\n")
            } else {
                format!("\n  \"{root}\": {rendered}\n")
            };
            output.insert_str(close, &insertion);
        } else {
            return format!("{rendered}\n");
        }
    }
    if output.trim().is_empty() {
        return serde_json::to_string_pretty(desired)
            .map(|value| format!("{value}\n"))
            .unwrap_or_default();
    }
    if !output.ends_with('\n') {
        output.push('\n');
    }
    output
}

/// A managed JSONC root is rendered from semantic JSON, but comments inside
/// that root still belong to the user's file. Keep them as leading comments
/// on the replacement value so an ownership update never silently erases
/// comment text while preserving unmanaged semantic fields through `merged`.
fn preserve_jsonc_comments(rendered: &str, original: &str) -> String {
    let comments = extract_jsonc_comments(original);
    if comments.is_empty() {
        rendered.to_owned()
    } else {
        format!("{}\n{rendered}", comments.join("\n"))
    }
}

fn extract_jsonc_comments(source: &str) -> Vec<String> {
    let bytes = source.as_bytes();
    let mut comments = Vec::new();
    let mut index = 0;
    let mut in_string = false;
    let mut escaped = false;
    while index < bytes.len() {
        let byte = bytes[index];
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            index += 1;
            continue;
        }
        if byte == b'"' {
            in_string = true;
            index += 1;
            continue;
        }
        if byte == b'/' && bytes.get(index + 1) == Some(&b'/') {
            let start = index;
            index += 2;
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
            comments.push(source[start..index].trim_end().to_owned());
            continue;
        }
        if byte == b'/' && bytes.get(index + 1) == Some(&b'*') {
            let start = index;
            index += 2;
            while index + 1 < bytes.len() && !(bytes[index] == b'*' && bytes[index + 1] == b'/') {
                index += 1;
            }
            index = (index + 2).min(bytes.len());
            comments.push(source[start..index].to_owned());
            continue;
        }
        index += 1;
    }
    comments
}

fn jsonc_top_level_value_span(source: &str, wanted: &str) -> Option<(usize, usize)> {
    let bytes = source.as_bytes();
    let mut index = 0;
    let mut depth = 0usize;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'"' {
            let start = index;
            index += 1;
            let mut escaped = false;
            while index < bytes.len() {
                let current = bytes[index];
                index += 1;
                if escaped {
                    escaped = false;
                } else if current == b'\\' {
                    escaped = true;
                } else if current == b'"' {
                    break;
                }
            }
            if depth == 1 {
                let key = serde_json::from_str::<String>(&source[start..index]).ok()?;
                let mut cursor = skip_jsonc_space_and_comments(source.as_bytes(), index);
                if bytes.get(cursor) != Some(&b':') {
                    continue;
                }
                cursor = skip_jsonc_space_and_comments(source.as_bytes(), cursor + 1);
                if key == wanted {
                    let end = jsonc_value_end(source.as_bytes(), cursor)?;
                    return Some((cursor, end));
                }
                index = cursor;
                continue;
            }
            continue;
        }
        match byte {
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                index += 2;
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
            }
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                index += 2;
                while index + 1 < bytes.len() && !(bytes[index] == b'*' && bytes[index + 1] == b'/')
                {
                    index += 1;
                }
                index = (index + 2).min(bytes.len());
            }
            b'{' | b'[' => {
                depth += 1;
                index += 1;
            }
            b'}' | b']' => {
                depth = depth.saturating_sub(1);
                index += 1;
            }
            _ => index += 1,
        }
    }
    None
}

fn skip_jsonc_space_and_comments(bytes: &[u8], mut index: usize) -> usize {
    loop {
        while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
            index += 1;
        }
        if bytes.get(index) == Some(&b'/') && bytes.get(index + 1) == Some(&b'/') {
            index += 2;
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
            continue;
        }
        if bytes.get(index) == Some(&b'/') && bytes.get(index + 1) == Some(&b'*') {
            index += 2;
            while index + 1 < bytes.len() && !(bytes[index] == b'*' && bytes[index + 1] == b'/') {
                index += 1;
            }
            index = (index + 2).min(bytes.len());
            continue;
        }
        return index;
    }
}

fn jsonc_value_end(bytes: &[u8], mut index: usize) -> Option<usize> {
    let start = index;
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    while index < bytes.len() {
        let byte = bytes[index];
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            index += 1;
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'{' | b'[' => depth += 1,
            b'}' | b']' if depth > 0 => depth -= 1,
            b',' | b'}' if depth == 0 => return Some(index),
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                index += 2;
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
                continue;
            }
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                index += 2;
                while index + 1 < bytes.len() && !(bytes[index] == b'*' && bytes[index + 1] == b'/')
                {
                    index += 1;
                }
                index = (index + 2).min(bytes.len());
                continue;
            }
            _ => {}
        }
        index += 1;
    }
    (index > start).then_some(index)
}

fn project_document(
    document: &ObservedDocument,
    ownership: &ManagedOwnership,
) -> Result<Value, AppError> {
    match (document, ownership) {
        (ObservedDocument::Json(value), ManagedOwnership::WholeDocument)
        | (ObservedDocument::Jsonc { value, .. }, ManagedOwnership::WholeDocument)
        | (
            ObservedDocument::Toml {
                semantic: value, ..
            },
            ManagedOwnership::WholeDocument,
        ) => Ok(value.clone()),
        (ObservedDocument::Json(value), ManagedOwnership::Selectors(selectors))
        | (ObservedDocument::Jsonc { value, .. }, ManagedOwnership::Selectors(selectors))
        | (
            ObservedDocument::Toml {
                semantic: value, ..
            },
            ManagedOwnership::Selectors(selectors),
        ) => project_selectors(value, selectors),
        (ObservedDocument::Markdown(text), ManagedOwnership::WholeDocument) => {
            Ok(Value::String(text.clone()))
        }
        (ObservedDocument::SymlinkDirectory(entries), ManagedOwnership::SymlinkNames(names)) => {
            let selected = names
                .iter()
                .filter_map(|name| {
                    entries.get(name).map(|entry| {
                        (
                            name.clone(),
                            serde_json::to_value(entry).unwrap_or(Value::Null),
                        )
                    })
                })
                .collect::<Map<_, _>>();
            Ok(Value::Object(selected))
        }
        _ => Err(AppError::invalid_input(
            "managedOwnership",
            "受管选择器与目标格式不匹配",
        )),
    }
}

fn project_selectors(source: &Value, selectors: &[Vec<String>]) -> Result<Value, AppError> {
    let mut output = Value::Object(Map::new());
    for selector in selectors {
        if let Some(value) = get_json_path(source, selector)? {
            set_json_path(&mut output, selector, value.clone())?;
        }
    }
    Ok(output)
}

fn render_document(
    target: &TargetDescriptor,
    current: Option<&ObservedDocument>,
    desired_projection: &Value,
    ownership: &ManagedOwnership,
) -> Result<RenderedTarget, AppError> {
    match (target.format, current, ownership) {
        (TargetFormat::Json, _, ManagedOwnership::WholeDocument) => {
            if !desired_projection.is_object() {
                return Err(AppError::invalid_input(
                    "desiredProjection",
                    "JSON 配置根必须是对象",
                ));
            }
            let mut bytes = serde_json::to_vec_pretty(desired_projection).map_err(|error| {
                AppError::parse(target.path_for_error(), target.format.as_str()).with_source(error)
            })?;
            bytes.push(b'\n');
            Ok(RenderedTarget::File(bytes))
        }
        (TargetFormat::Json, current, ManagedOwnership::Selectors(selectors)) => {
            let mut merged = match current {
                Some(ObservedDocument::Json(value)) => value.clone(),
                None => Value::Object(Map::new()),
                _ => {
                    return Err(AppError::parse(
                        target.path_for_error(),
                        target.format.as_str(),
                    ))
                }
            };
            for selector in selectors {
                match get_json_path(desired_projection, selector)? {
                    Some(value) => set_json_path(&mut merged, selector, value.clone())?,
                    None => remove_json_path(&mut merged, selector),
                }
            }
            let mut bytes = serde_json::to_vec_pretty(&merged).map_err(|error| {
                AppError::parse(target.path_for_error(), target.format.as_str()).with_source(error)
            })?;
            bytes.push(b'\n');
            Ok(RenderedTarget::File(bytes))
        }
        (TargetFormat::Jsonc, current, ManagedOwnership::WholeDocument) => {
            if !desired_projection.is_object() {
                return Err(AppError::invalid_input(
                    "desiredProjection",
                    "JSONC 配置根必须是对象",
                ));
            }
            match current {
                Some(ObservedDocument::Jsonc { source, .. }) => Ok(RenderedTarget::File(
                    replace_jsonc_roots(source, desired_projection, &[]).into_bytes(),
                )),
                _ => {
                    let mut bytes =
                        serde_json::to_vec_pretty(desired_projection).map_err(|error| {
                            AppError::parse(target.path_for_error(), target.format.as_str())
                                .with_source(error)
                        })?;
                    bytes.push(b'\n');
                    Ok(RenderedTarget::File(bytes))
                }
            }
        }
        (TargetFormat::Jsonc, current, ManagedOwnership::Selectors(selectors)) => {
            let mut merged = match current {
                Some(ObservedDocument::Jsonc { value, .. }) => value.clone(),
                Some(ObservedDocument::Json(value)) => value.clone(),
                None => Value::Object(Map::new()),
                _ => {
                    return Err(AppError::parse(
                        target.path_for_error(),
                        target.format.as_str(),
                    ))
                }
            };
            for selector in selectors {
                match get_json_path(desired_projection, selector)? {
                    Some(value) => set_json_path(&mut merged, selector, value.clone())?,
                    None => remove_json_path(&mut merged, selector),
                }
            }
            let roots = selectors
                .iter()
                .filter_map(|selector| selector.first().cloned())
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            let rendered = serde_json::to_string_pretty(&merged).map_err(|error| {
                AppError::parse(target.path_for_error(), target.format.as_str()).with_source(error)
            })?;
            match current {
                Some(ObservedDocument::Jsonc { source, .. }) => Ok(RenderedTarget::File(
                    replace_jsonc_roots_with_rendered(source, &merged, &roots).into_bytes(),
                )),
                _ => Ok(RenderedTarget::File(format!("{rendered}\n").into_bytes())),
            }
        }
        (TargetFormat::Toml, _, ManagedOwnership::WholeDocument) => {
            let mut document = toml_edit::ser::to_document(desired_projection).map_err(|error| {
                AppError::parse(target.path_for_error(), target.format.as_str()).with_source(error)
            })?;
            if target.artifact_kind == ArtifactKind::Agent {
                // `toml_edit::ser::to_document` represents nested serde maps as
                // inline tables.  Agent settings use a conventional `[features]`
                // table so Codex can merge and inspect feature flags naturally;
                // move top-level inline tables after scalar keys while preserving
                // deterministic key order.  Other TOML whole-document artifacts
                // keep their existing renderer byte-for-byte contract.
                let nested_keys = document
                    .as_table()
                    .iter()
                    .filter_map(|(key, item)| {
                        item.as_value()
                            .and_then(|value| value.as_inline_table())
                            .map(|_| key.to_owned())
                    })
                    .collect::<Vec<_>>();
                for key in nested_keys {
                    if let Some(item) = document.as_table_mut().remove(&key) {
                        let table = item.into_table().map_err(|_| {
                            AppError::parse(target.path_for_error(), target.format.as_str())
                        })?;
                        document
                            .as_table_mut()
                            .insert(&key, toml_edit::Item::Table(table));
                    }
                }
            }
            Ok(RenderedTarget::File(document.to_string().into_bytes()))
        }
        (TargetFormat::Toml, current, ManagedOwnership::Selectors(selectors)) => {
            let mut document = match current {
                Some(ObservedDocument::Toml { document, .. }) => document.clone(),
                None => DocumentMut::new(),
                _ => {
                    return Err(AppError::parse(
                        target.path_for_error(),
                        target.format.as_str(),
                    ))
                }
            };
            for selector in selectors {
                let value = get_json_path(desired_projection, selector)?;
                set_toml_path(document.as_table_mut(), selector, value)?;
            }
            Ok(RenderedTarget::File(document.to_string().into_bytes()))
        }
        (TargetFormat::Markdown, _, ManagedOwnership::WholeDocument) => {
            let text = desired_projection.as_str().ok_or_else(|| {
                AppError::invalid_input("desiredProjection", "Markdown 投影必须是字符串")
            })?;
            Ok(RenderedTarget::File(text.as_bytes().to_vec()))
        }
        (TargetFormat::CursorMdc, _, ManagedOwnership::WholeDocument) => {
            let body = desired_projection.as_str().ok_or_else(|| {
                AppError::invalid_input("desiredProjection", "Markdown 投影必须是字符串")
            })?;
            Ok(RenderedTarget::File(render_cursor_mdc(body).into_bytes()))
        }
        (TargetFormat::SymlinkDirectory, _, _) => Err(AppError::invalid_input(
            "targetFormat",
            "Phase 2 不渲染或写入 Skills 链接",
        )),
        _ => Err(AppError::invalid_input(
            "managedOwnership",
            "受管选择器与目标格式不匹配",
        )),
    }
}

/// Cursor `.mdc` 规则文件的固定 frontmatter：官方要求规则带 `alwaysApply: true`
/// 才会作为常驻指令进入每次会话（cursor.com/docs/rules，2026-09-06 核验）。
/// frontmatter 由应用固定产出，档案正文保持工具无关。
const CURSOR_MDC_FRONTMATTER: &str = "---\nalwaysApply: true\n---\n\n";

fn render_cursor_mdc(body: &str) -> String {
    format!("{CURSOR_MDC_FRONTMATTER}{body}")
}

/// 剥离 `.mdc` 首部的 frontmatter 块，得到与档案正文同域的纯正文。
/// 与 [`render_cursor_mdc`] 严格互逆：渲染在闭合 `---` 后写 `\n\n` 分隔，
/// 剥离去掉块尾换行后仅吞掉后续的空行分隔。无 frontmatter 的文件按原文返回。
fn strip_mdc_frontmatter(text: &str) -> String {
    let after_open = match text
        .strip_prefix("---\r\n")
        .or_else(|| text.strip_prefix("---\n"))
    {
        Some(rest) => rest,
        None => return text.to_owned(),
    };
    let mut offset = 0usize;
    for line in after_open.lines() {
        let line_len = line.len();
        let rest_starts = offset + line_len;
        let eol_len = if after_open[rest_starts..].starts_with("\r\n") {
            2
        } else if after_open[rest_starts..].starts_with('\n') {
            1
        } else {
            0
        };
        let is_closing = line == "---";
        offset = rest_starts + eol_len;
        if is_closing {
            return after_open[offset..]
                .trim_start_matches(['\n', '\r'])
                .to_owned();
        }
        if eol_len == 0 {
            break;
        }
    }
    text.to_owned()
}

fn get_json_path<'a>(value: &'a Value, path: &[String]) -> Result<Option<&'a Value>, AppError> {
    let mut current = value;
    for segment in path {
        let object = current.as_object().ok_or_else(|| {
            AppError::invalid_input(
                "managedProjection",
                "受管选择器的中间节点必须是对象或 TOML 表",
            )
        })?;
        let Some(next) = object.get(segment) else {
            return Ok(None);
        };
        current = next;
    }
    Ok(Some(current))
}

fn set_json_path(root: &mut Value, path: &[String], value: Value) -> Result<(), AppError> {
    if path.is_empty() {
        *root = value;
        return Ok(());
    }
    let mut current = root;
    for segment in &path[..path.len() - 1] {
        if !current.is_object() {
            return Err(AppError::invalid_input(
                "managedProjection",
                "受管选择器不能覆盖非对象中间节点",
            ));
        }
        current = current
            .as_object_mut()
            .ok_or_else(|| AppError::internal("受管投影中间节点必须是对象"))?
            .entry(segment.clone())
            .or_insert_with(|| Value::Object(Map::new()));
    }
    if !current.is_object() {
        return Err(AppError::invalid_input(
            "managedProjection",
            "受管选择器不能覆盖非对象中间节点",
        ));
    }
    current
        .as_object_mut()
        .ok_or_else(|| AppError::internal("受管投影叶节点必须是对象"))?
        .insert(path[path.len() - 1].clone(), value);
    Ok(())
}

fn remove_json_path(root: &mut Value, path: &[String]) {
    if path.is_empty() {
        *root = Value::Null;
        return;
    }
    let mut current = root;
    for segment in &path[..path.len() - 1] {
        let Some(next) = current
            .as_object_mut()
            .and_then(|object| object.get_mut(segment))
        else {
            return;
        };
        current = next;
    }
    if let Some(object) = current.as_object_mut() {
        object.remove(&path[path.len() - 1]);
    }
}

fn set_toml_path(
    table: &mut dyn TableLike,
    path: &[String],
    value: Option<&Value>,
) -> Result<(), AppError> {
    let Some((head, tail)) = path.split_first() else {
        return Err(AppError::invalid_input(
            "managedSelector",
            "TOML selector 不能为空",
        ));
    };
    if tail.is_empty() {
        if let Some(value) = value {
            let key_decor = table.key(head).map(|key| key.leaf_decor().clone());
            let mut replacement = json_to_toml_item(value)?;
            if let Some(current) = table.get(head) {
                preserve_toml_decor(current, &mut replacement);
            }
            table.insert(head, replacement);
            if let (Some(decor), Some(mut key)) = (key_decor, table.key_mut(head)) {
                *key.leaf_decor_mut() = decor;
            }
        } else {
            table.remove(head);
        }
        return Ok(());
    }

    if value.is_none() && !table.contains_key(head) {
        return Ok(());
    }
    match table.get(head) {
        Some(item) if item.as_table_like().is_none() => {
            return Err(AppError::invalid_input(
                "managedProjection",
                "TOML 受管选择器不能覆盖非表中间节点",
            ));
        }
        None => {
            table.insert(head, Item::Table(Table::new()));
        }
        Some(_) => {}
    }
    let child = table
        .get_mut(head)
        .and_then(Item::as_table_like_mut)
        .ok_or_else(|| AppError::invalid_input("managedSelector", "TOML 路径不是表"))?;
    set_toml_path(child, tail, value)
}

fn preserve_toml_decor(current: &Item, replacement: &mut Item) {
    match (current, replacement) {
        (Item::Value(current), Item::Value(replacement)) => {
            *replacement.decor_mut() = current.decor().clone();
        }
        (Item::Table(current), Item::Table(replacement)) => {
            *replacement.decor_mut() = current.decor().clone();
        }
        _ => {}
    }
}

fn json_to_toml_item(value: &Value) -> Result<Item, AppError> {
    match value {
        Value::String(value) => Ok(toml_edit::value(value.clone())),
        Value::Bool(value) => Ok(toml_edit::value(*value)),
        Value::Number(value) => {
            if let Some(number) = value.as_i64() {
                Ok(toml_edit::value(number))
            } else if let Some(number) = value.as_u64() {
                let number = i64::try_from(number).map_err(|error| {
                    AppError::invalid_input("desiredProjection", "TOML 整数超出范围")
                        .with_source(error)
                })?;
                Ok(toml_edit::value(number))
            } else {
                Ok(toml_edit::value(value.as_f64().ok_or_else(|| {
                    AppError::internal("JSON 数字既不是整数也不能转为 f64")
                })?))
            }
        }
        Value::Array(values) => {
            let mut array = Array::new();
            for value in values {
                let item = json_to_toml_item(value)?;
                let scalar = item.into_value().map_err(|error| {
                    AppError::invalid_input("desiredProjection", "TOML 数组只支持标量值")
                        .with_source(error)
                })?;
                array.push_formatted(scalar);
            }
            Ok(toml_edit::value(array))
        }
        Value::Object(values) => {
            let mut table = Table::new();
            for (key, value) in values {
                table.insert(key, json_to_toml_item(value)?);
            }
            Ok(Item::Table(table))
        }
        Value::Null => Err(AppError::invalid_input(
            "desiredProjection",
            "TOML 不支持 null",
        )),
    }
}
