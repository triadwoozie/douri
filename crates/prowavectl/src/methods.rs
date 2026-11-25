//! The list of control methods: reading it (from the app's `rpc.describe` answer or from the
//! copy built into this program), printing it, and turning it into MCP tool definitions.

use serde_json::{json, Map, Value};

/// The method list built into this program, in the shape of an `rpc.describe` answer. Used when
/// the app is not running. The file is generated from the app's method table.
pub const EMBEDDED: &str = include_str!("../methods.json");

/// Every MCP tool name starts with this.
pub const TOOL_PREFIX: &str = "prowave_";

/// One parameter of a method.
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    /// One of string, number, integer, boolean, object, array, any.
    pub kind: String,
    pub required: bool,
    pub description: String,
    pub unit: Option<String>,
    /// The allowed values, when the app lists them.
    pub values: Vec<Value>,
    pub min: Option<Value>,
    pub max: Option<Value>,
}

/// One control method.
#[derive(Debug, Clone, PartialEq)]
pub struct Method {
    pub name: String,
    pub group: String,
    pub summary: String,
    pub changes_project: Option<bool>,
    pub params: Vec<Param>,
    /// Plain-English description of the result.
    pub result: String,
}

/// Reads the methods out of an `rpc.describe` answer.
pub fn parse_methods(describe: &Value) -> Result<Vec<Method>, String> {
    let list = describe
        .get("methods")
        .and_then(Value::as_array)
        .ok_or_else(|| "the method list has no \"methods\" array".to_string())?;
    list.iter().map(parse_method).collect()
}

/// Reads the method list built into this program.
pub fn embedded_methods() -> Result<Vec<Method>, String> {
    let describe: Value = serde_json::from_str(EMBEDDED)
        .map_err(|error| format!("the built-in method list is not valid JSON: {error}"))?;
    parse_methods(&describe)
}

fn text(value: &Value, key: &str) -> String {
    match value.get(key) {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    }
}

fn parse_method(value: &Value) -> Result<Method, String> {
    let name = value
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
        .ok_or_else(|| "a method in the list has no name".to_string())?
        .to_string();
    let mut group = text(value, "group");
    if group.is_empty() {
        // No group given: use the part of the name before the first dot.
        group = name.split('.').next().unwrap_or(&name).to_string();
    }
    let params = match value.get("params") {
        Some(Value::Array(params)) => params
            .iter()
            .map(|param| parse_param(param, &name))
            .collect::<Result<Vec<_>, _>>()?,
        _ => Vec::new(),
    };
    Ok(Method {
        group,
        summary: text(value, "summary"),
        changes_project: value.get("changes_project").and_then(Value::as_bool),
        params,
        result: text(value, "result"),
        name,
    })
}

fn parse_param(value: &Value, method: &str) -> Result<Param, String> {
    let name = value
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
        .ok_or_else(|| format!("a param of {method} has no name"))?
        .to_string();
    let mut kind = text(value, "type");
    if kind.is_empty() {
        kind = "any".to_string();
    }
    let number = |key: &str| value.get(key).filter(|v| v.is_number()).cloned();
    Ok(Param {
        name,
        kind,
        required: value
            .get("required")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        description: text(value, "description"),
        unit: Some(text(value, "unit")).filter(|unit| !unit.is_empty()),
        values: value
            .get("values")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        min: number("min"),
        max: number("max"),
    })
}

/// The grouped list printed by `prowavectl methods`: one line per method, then a count.
pub fn format_method_list(methods: &[Method]) -> String {
    // Groups in the order they first appear.
    let mut groups: Vec<&str> = Vec::new();
    for method in methods {
        if !groups.contains(&method.group.as_str()) {
            groups.push(&method.group);
        }
    }
    let width = methods
        .iter()
        .map(|m| m.name.chars().count())
        .max()
        .unwrap_or(0);
    let mut out = String::new();
    for group in &groups {
        out.push_str(group);
        out.push('\n');
        for method in methods.iter().filter(|m| m.group == *group) {
            let line = format!("  {:width$}  {}", method.name, method.summary);
            out.push_str(line.trim_end());
            out.push('\n');
        }
        out.push('\n');
    }
    let noun = if methods.len() == 1 {
        "method"
    } else {
        "methods"
    };
    out.push_str(&format!("{} {noun}\n", methods.len()));
    out
}

/// A value as shown in a table: strings without their quotes.
fn plain(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// The "allowed values" cell of the params table: the listed values and/or the range.
fn allowed(param: &Param) -> String {
    let mut parts = Vec::new();
    if !param.values.is_empty() {
        parts.push(
            param
                .values
                .iter()
                .map(plain)
                .collect::<Vec<_>>()
                .join(", "),
        );
    }
    match (&param.min, &param.max) {
        (Some(min), Some(max)) => parts.push(format!("{min} to {max}")),
        (Some(min), None) => parts.push(format!("{min} or more")),
        (None, Some(max)) => parts.push(format!("{max} or less")),
        (None, None) => {}
    }
    parts.join("; ")
}

/// The text printed by `prowavectl describe <method>`.
pub fn format_method(method: &Method) -> String {
    let mut out = format!("{}  (group: {})\n", method.name, method.group);
    if !method.summary.is_empty() {
        out.push_str(&format!("  {}\n", method.summary));
    }
    match method.changes_project {
        Some(true) => out.push_str("  Changes the project: yes\n"),
        Some(false) => out.push_str("  Changes the project: no\n"),
        None => {}
    }

    out.push_str("\nParams:\n");
    if method.params.is_empty() {
        out.push_str("  none\n");
    } else {
        let header =
            ["NAME", "TYPE", "REQUIRED", "UNIT", "ALLOWED", "DESCRIPTION"].map(String::from);
        let mut rows = vec![header];
        for param in &method.params {
            rows.push([
                param.name.clone(),
                param.kind.clone(),
                if param.required { "yes" } else { "no" }.to_string(),
                param.unit.clone().unwrap_or_default(),
                allowed(param),
                param.description.clone(),
            ]);
        }
        // Every column but the last is padded to its widest cell.
        let mut widths = [0usize; 6];
        for row in &rows {
            for (width, cell) in widths.iter_mut().zip(row) {
                *width = (*width).max(cell.chars().count());
            }
        }
        for row in &rows {
            let mut line = String::from(" ");
            for (index, cell) in row.iter().enumerate() {
                line.push(' ');
                if index + 1 == row.len() {
                    line.push_str(cell);
                } else {
                    line.push_str(&format!("{:width$} ", cell, width = widths[index]));
                }
            }
            out.push_str(line.trim_end());
            out.push('\n');
        }
    }

    out.push_str("\nResult:\n");
    if method.result.is_empty() {
        out.push_str("  not described\n");
    } else {
        out.push_str(&format!("  {}\n", method.result));
    }
    out
}

/// The MCP tool name for a control method: the prefix, then the method name with every character
/// other than a letter, digit or underscore turned into an underscore.
pub fn tool_name(method: &str) -> String {
    let mut name = String::from(TOOL_PREFIX);
    for c in method.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            name.push(c);
        } else {
            name.push('_');
        }
    }
    name
}

/// The JSON Schema for a method's params, as MCP's `inputSchema`.
pub fn input_schema(method: &Method) -> Value {
    if method.params.is_empty() {
        // The form the MCP specification recommends for a tool without parameters.
        return json!({"type": "object", "additionalProperties": false});
    }
    let mut properties = Map::new();
    let mut required = Vec::new();
    for param in &method.params {
        let mut schema = Map::new();
        // "any" (or a type this program does not know) gets no type key: anything goes.
        const KINDS: [&str; 6] = ["string", "number", "integer", "boolean", "object", "array"];
        if KINDS.contains(&param.kind.as_str()) {
            schema.insert("type".into(), json!(param.kind));
        }
        let mut description = param.description.clone();
        if let Some(unit) = &param.unit {
            if !description.is_empty() {
                description.push(' ');
            }
            description.push_str(&format!("(unit: {unit})"));
        }
        if !description.is_empty() {
            schema.insert("description".into(), json!(description));
        }
        if param.kind == "string" && !param.values.is_empty() {
            schema.insert("enum".into(), Value::Array(param.values.clone()));
        }
        if let Some(min) = &param.min {
            schema.insert("minimum".into(), min.clone());
        }
        if let Some(max) = &param.max {
            schema.insert("maximum".into(), max.clone());
        }
        properties.insert(param.name.clone(), Value::Object(schema));
        if param.required {
            required.push(json!(param.name));
        }
    }
    let mut schema = Map::new();
    schema.insert("type".into(), json!("object"));
    schema.insert("properties".into(), Value::Object(properties));
    if !required.is_empty() {
        schema.insert("required".into(), Value::Array(required));
    }
    Value::Object(schema)
}

/// The MCP tool definition for a control method.
pub fn tool_definition(method: &Method) -> Value {
    let mut description = method.summary.clone();
    if !method.result.is_empty() {
        if !description.is_empty() {
            description.push('\n');
        }
        description.push_str(&format!("Result: {}", method.result));
    }
    if description.is_empty() {
        description = format!("Calls the Dōri control method {}.", method.name);
    }
    json!({
        "name": tool_name(&method.name),
        "description": description,
        "inputSchema": input_schema(method),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<Method> {
        parse_methods(&json!({"methods": [
            {"name": "transport.play", "group": "transport", "summary": "Start playback.",
             "changes_project": false, "params": [], "result": "The transport state."},
            {"name": "transport.locate", "group": "transport", "summary": "Move the playhead.",
             "changes_project": false,
             "params": [
                {"name": "bar", "type": "number", "required": true, "description": "Bar to go to.",
                 "unit": "bars", "min": 1, "max": 999},
                {"name": "snap", "type": "string", "required": false, "description": "Rounding.",
                 "values": ["none", "bar"]},
                {"name": "extra", "type": "any", "required": false, "description": ""},
                {"name": "count", "type": "integer", "min": 0}],
             "result": "The transport state."},
            {"name": "ui.dialog.answer", "summary": "Answer the open dialog."}
        ]}))
        .unwrap()
    }

    #[test]
    fn tool_names() {
        assert_eq!(tool_name("transport.play"), "prowave_transport_play");
        assert_eq!(tool_name("ui.dialog.answer"), "prowave_ui_dialog_answer");
        assert_eq!(tool_name("a-b/c d_e9"), "prowave_a_b_c_d_e9");
        assert_eq!(tool_name("tést"), "prowave_t_st");
    }

    #[test]
    fn schema_for_a_method_without_params() {
        let methods = sample();
        assert_eq!(
            input_schema(&methods[0]),
            json!({"type": "object", "additionalProperties": false})
        );
    }

    #[test]
    fn schema_from_params() {
        let methods = sample();
        assert_eq!(
            input_schema(&methods[1]),
            json!({
                "type": "object",
                "properties": {
                    "bar": {"type": "number", "description": "Bar to go to. (unit: bars)",
                            "minimum": 1, "maximum": 999},
                    "snap": {"type": "string", "description": "Rounding.",
                             "enum": ["none", "bar"]},
                    "extra": {},
                    "count": {"type": "integer", "minimum": 0}
                },
                "required": ["bar"]
            })
        );
    }

    #[test]
    fn allowed_values_become_an_enum_only_for_strings() {
        let methods = parse_methods(&json!({"methods": [{"name": "a.b", "params": [
            {"name": "n", "type": "number", "values": ["1", "2"]}]}]}))
        .unwrap();
        assert_eq!(
            input_schema(&methods[0])["properties"]["n"],
            json!({"type": "number"})
        );
    }

    #[test]
    fn tool_definition_has_summary_and_result() {
        let methods = sample();
        let tool = tool_definition(&methods[1]);
        assert_eq!(tool["name"], "prowave_transport_locate");
        assert_eq!(
            tool["description"],
            "Move the playhead.\nResult: The transport state."
        );
        assert_eq!(tool["inputSchema"]["type"], "object");
    }

    #[test]
    fn missing_fields_get_defaults() {
        let methods = sample();
        let answer = &methods[2];
        assert_eq!(answer.group, "ui");
        assert_eq!(answer.changes_project, None);
        assert!(answer.params.is_empty());
        assert_eq!(answer.result, "");
        assert!(parse_methods(&json!({})).is_err());
        assert!(parse_methods(&json!({"methods": [{"summary": "no name"}]})).is_err());
    }

    #[test]
    fn built_in_list_parses_and_has_methods() {
        let methods = embedded_methods().unwrap();
        assert!(!methods.is_empty());
        for method in &methods {
            assert!(!method.name.is_empty());
            // Every built-in method must give a usable tool definition.
            assert_eq!(tool_definition(method)["inputSchema"]["type"], "object");
        }
    }

    #[test]
    fn method_list_is_grouped_and_counted() {
        let text = format_method_list(&sample());
        assert_eq!(
            text,
            "transport\n\
             \x20 transport.play    Start playback.\n\
             \x20 transport.locate  Move the playhead.\n\
             \n\
             ui\n\
             \x20 ui.dialog.answer  Answer the open dialog.\n\
             \n\
             3 methods\n"
        );
    }

    #[test]
    fn one_method_is_shown_with_a_params_table() {
        let text = format_method(&sample()[1]);
        assert!(text.starts_with("transport.locate  (group: transport)\n  Move the playhead.\n"));
        assert!(text.contains("Changes the project: no"));
        assert!(text.contains("NAME   TYPE     REQUIRED  UNIT  ALLOWED    DESCRIPTION"));
        assert!(text.contains("bar    number   yes       bars  1 to 999   Bar to go to."));
        assert!(text.contains("snap   string   no              none, bar  Rounding."));
        assert!(text.contains("count  integer  no              0 or more"));
        assert!(text.contains("\nResult:\n  The transport state.\n"));
        assert!(format_method(&sample()[0]).contains("Params:\n  none\n"));
    }
}
