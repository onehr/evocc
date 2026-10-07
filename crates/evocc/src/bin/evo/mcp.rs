//! The stdio tool surface delegates to the same JSON commands as the CLI.
use super::{code, json, quote, Result};
use json::Json;
use std::io::{self, BufRead, Read, Write};
use std::process::Command;

struct Tool {
    name: &'static str,
    description: &'static str,
    fields: &'static [&'static str],
}
const TOOLS: &[Tool] = &[
    Tool {
        name: "run",
        description: "Execute one C main in the interpreter.",
        fields: &["file", "args"],
    },
    Tool {
        name: "check",
        description: "Check the executed path of one C main.",
        fields: &["file"],
    },
    Tool {
        name: "explain",
        description: "Read a diagnostic explanation.",
        fields: &["code"],
    },
    Tool {
        name: "prove",
        description: "Check Core source to assembly and write the requested output.",
        fields: &["file", "output", "entries"],
    },
    Tool {
        name: "fix_plan",
        description: "List source repairs without changing the source.",
        fields: &["file"],
    },
];

fn is_array(field: &str) -> bool {
    matches!(field, "args" | "entries")
}

fn tools() -> String {
    let rows = TOOLS.iter().map(|tool| {
        let properties = tool.fields.iter().map(|field| {
            let schema = if is_array(field) { r#"{"type":"array","items":{"type":"string"}}"# } else { r#"{"type":"string"}"# };
            format!("{}:{schema}", quote(field))
        }).collect::<Vec<_>>().join(",");
        let required = tool.fields.iter().filter(|f| !is_array(f)).map(|s| quote(s)).collect::<Vec<_>>().join(",");
        format!("{{\"name\":{},\"description\":{},\"inputSchema\":{{\"type\":\"object\",\"properties\":{{{properties}}},\"required\":[{required}],\"additionalProperties\":false}}}}", quote(tool.name), quote(tool.description))
    }).collect::<Vec<_>>().join(",");
    format!("{{\"tools\":[{rows}]}}")
}

fn command(params: &Json) -> Result<Command> {
    let name = params.get("name")?.string()?;
    let tool = TOOLS
        .iter()
        .find(|t| t.name == name)
        .ok_or("unknown tool")?;
    let Json::Object(fields) = params.get("arguments")? else {
        return Err("arguments must be an object".into());
    };
    if fields.keys().any(|k| !tool.fields.contains(&k.as_str())) {
        return Err("unknown tool argument".into());
    }
    for field in tool.fields {
        if is_array(field) {
            if let Some(value) = fields.get(*field) {
                for v in value.array()? {
                    v.string()?;
                }
            }
        } else {
            fields
                .get(*field)
                .ok_or("missing tool argument")?
                .string()?;
        }
    }
    let mut command = Command::new(std::env::current_exe()?);
    command.arg(if name == "fix_plan" { "fix" } else { name });
    if name == "explain" {
        command.arg(fields["code"].string()?);
    } else {
        command.arg(std::path::absolute(fields["file"].string()?)?);
    }
    command.arg("--json");
    if name == "fix_plan" {
        command.arg("--plan");
    }
    if name == "prove" {
        command
            .arg("-o")
            .arg(std::path::absolute(fields["output"].string()?)?);
    }
    for key in ["args", "entries"] {
        if let Some(values) = fields.get(key) {
            if key == "args" {
                command.arg("--");
            }
            for value in values.array()? {
                if key == "entries" {
                    command.arg("--entry");
                }
                command.arg(value.string()?);
            }
        }
    }
    Ok(command)
}

fn call(mut command: Command) -> Result<String> {
    let output = command.output()?;
    let text = std::str::from_utf8(&output.stdout)?.trim();
    let record = json::parse(text)?;
    if record.get("exit_code")?.render() != code(output.status).to_string() {
        return Err("command record and process status differ".into());
    }
    Ok(format!("{{\"content\":[{{\"type\":\"text\",\"text\":{}}}],\"structuredContent\":{},\"isError\":{}}}", quote(text), record.render(), !output.status.success()))
}

fn initialize(params: &Json) -> Result<String> {
    params.get("protocolVersion")?.string()?;
    params.get("clientInfo")?.get("name")?.string()?;
    params.get("clientInfo")?.get("version")?.string()?;
    if !matches!(params.get("capabilities")?, Json::Object(_)) {
        return Err("capabilities must be an object".into());
    }
    // Negotiate the explicit initialize-based protocol requested by this CLI.
    Ok(format!("{{\"protocolVersion\":\"2025-11-25\",\"capabilities\":{{\"tools\":{{}}}},\"serverInfo\":{{\"name\":\"evo\",\"version\":{}}}}}", quote(env!("CARGO_PKG_VERSION"))))
}

fn error(id: &str, code: i32, message: &str) -> String {
    format!(
        "{{\"jsonrpc\":\"2.0\",\"id\":{id},\"error\":{{\"code\":{code},\"message\":{}}}}}",
        quote(message)
    )
}

pub fn serve() -> Result<i32> {
    let mut input = io::stdin().lock();
    let mut output = io::stdout().lock();
    let mut ready = false;
    loop {
        // Bound a frame before allocating its parse tree. Drain only this frame.
        let mut bytes = Vec::new();
        if (&mut input).take(1_048_577).read_until(b'\n', &mut bytes)? == 0 {
            break;
        }
        let parsed = if bytes.len() > 1_048_576 {
            if !bytes.ends_with(b"\n") {
                input.skip_until(b'\n')?;
            }
            Err("JSON message exceeds 1 MiB".into())
        } else {
            std::str::from_utf8(&bytes)
                .map_err(Into::into)
                .and_then(json::parse)
        };
        let message = match parsed {
            Ok(message) => message,
            Err(e) => {
                writeln!(output, "{}", error("null", -32700, &e.to_string()))?;
                output.flush()?;
                continue;
            }
        };
        let id = message.get("id").ok();
        let valid_id = matches!(id, None | Some(Json::String(_) | Json::Number(_)));
        let method = message.get("method").and_then(Json::string);
        if !valid_id
            || message.get("jsonrpc").and_then(Json::string).ok() != Some("2.0")
            || method.is_err()
        {
            writeln!(output, "{}", error("null", -32600, "invalid request"))?;
            output.flush()?;
            continue;
        }
        // Notifications never invoke tools and never produce a response.
        let Some(id) = id else {
            continue;
        };
        let params = message.get("params");
        if params.as_ref().is_ok_and(|p| !matches!(p, Json::Object(_))) {
            writeln!(
                output,
                "{}",
                error(&id.render(), -32602, "params must be an object")
            )?;
            output.flush()?;
            continue;
        }
        let result = match method? {
            "initialize" if !ready => params
                .and_then(initialize)
                .inspect(|_| ready = true)
                .map_err(|e| (-32602, e.to_string())),
            _ if !ready => Err((-32002, "initialize first".into())),
            "tools/list" => Ok(tools()),
            "tools/call" => params
                .and_then(command)
                .map_err(|e| (-32602, e.to_string()))
                .and_then(|c| call(c).map_err(|e| (-32603, e.to_string()))),
            _ => Err((-32601, "method not found".into())),
        };
        let response = match result {
            Ok(value) => format!(
                "{{\"jsonrpc\":\"2.0\",\"id\":{},\"result\":{value}}}",
                id.render()
            ),
            Err((code, message)) => error(&id.render(), code, &message),
        };
        writeln!(output, "{response}")?;
        output.flush()?;
    }
    Ok(0)
}
