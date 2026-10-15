// SPDX-License-Identifier: GPL-3.0-only
//! Audited compatibility signatures; features without native handlers fail explicitly.
use bead::services::ipc::{Call, Reply};
use serde::Deserialize;
use serde_json::Value;
#[derive(Deserialize)]
pub struct Catalog {
    pub routes: Vec<Route>,
}
#[derive(Deserialize)]
pub struct Route {
    pub patterns: Vec<Pattern>,
    pub method: String,
    pub parameters: Vec<Parameter>,
    pub returns: String,
}
#[derive(Deserialize)]
pub struct Pattern {
    pub prefix: String,
    pub dynamic: bool,
}
#[derive(Deserialize)]
pub struct Parameter {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
}
pub fn catalog() -> Result<Catalog, String> {
    serde_json::from_str(include_str!("../../../shell/command-catalog.json"))
        .map_err(|e| e.to_string())
}
impl Catalog {
    pub fn route(&self, target: &str, method: &str) -> Option<&Route> {
        self.routes.iter().find(|r| {
            r.method == method
                && r.patterns.iter().any(|p| {
                    if p.dynamic {
                        target.starts_with(&p.prefix) && target.len() > p.prefix.len()
                    } else {
                        target == p.prefix
                    }
                })
        })
    }
    pub fn validate(&self, call: &Call) -> Result<&Route, String> {
        call.validate()?;
        let route = self
            .route(&call.target, &call.method)
            .ok_or("unknown target/method")?;
        if call.args.len() != route.parameters.len() {
            return Err("wrong argument count".into());
        }
        for (p, v) in route.parameters.iter().zip(&call.args) {
            let valid = match p.kind.as_str() {
                "string" => v
                    .as_str()
                    .is_some_and(|s| s.len() <= 65536 && !s.contains('\0')),
                "int" => v.as_i64().is_some(),
                "real" => v.as_f64().is_some_and(f64::is_finite),
                "bool" => v.is_boolean(),
                _ => false,
            };
            if !valid {
                return Err(format!("invalid {} argument", p.name));
            }
        }
        Ok(route)
    }
}
pub fn client(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let mut args = args.iter();
    let mut socket = None;
    let mut json = false;
    loop {
        match args.clone().next().map(String::as_str) {
            Some("--socket") => {
                args.next();
                socket = Some(std::path::PathBuf::from(
                    args.next().ok_or("--socket needs path")?,
                ));
            }
            Some("--json") => {
                args.next();
                json = true;
            }
            _ => break,
        }
    }
    let target = args.next().ok_or("ipc needs target")?.clone();
    let method = args.next().ok_or("ipc needs method")?.clone();
    let values: Vec<_> = args.cloned().collect();
    let c = catalog()?;
    let parameters = c.route(&target, &method).map(|r| r.parameters.as_slice());
    let mut converted = Vec::new();
    for (i, arg) in values.into_iter().enumerate() {
        let kind = parameters
            .and_then(|p| p.get(i))
            .map(|p| p.kind.as_str())
            .unwrap_or("string");
        converted.push(match kind {
            "int" => Value::from(arg.parse::<i64>()?),
            "real" => serde_json::to_value(arg.parse::<f64>()?)?,
            "bool" => Value::from(arg.parse::<bool>()?),
            _ => Value::String(arg),
        });
    }
    let call = Call {
        version: 1,
        id: 1,
        target,
        method,
        args: converted,
        output: None,
    };
    if call.target != "bead" && call.target != "bead.panel" {
        c.validate(&call)?;
    }
    let reply = bead::services::ipc::call(
        &socket.ok_or("explicit --socket required")?,
        &call,
        std::time::Duration::from_secs(3),
    )?;
    if json {
        println!("{}", serde_json::to_string(&reply)?);
    } else if let Some(result) = &reply.result {
        if let Some(text) = result.as_str() {
            println!("{text}");
        } else {
            println!("{result}");
        }
    }
    if let Some(error) = reply.error {
        return Err(format!("{}: {}", error.code, error.message).into());
    }
    Ok(())
}
pub fn validate(call: &Call) -> Result<(), Reply> {
    let catalog = catalog().map_err(|e| Reply::error(call.id, "catalog", e))?;
    let route = catalog
        .validate(call)
        .map_err(|e| Reply::error(call.id, "invalid-route", e))?;
    if !["void", "string", "bool"].contains(&route.returns.as_str()) {
        return Err(Reply::error(
            call.id,
            "unsupported-return",
            "unknown return contract",
        ));
    }
    Ok(())
}
pub fn helper_client(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() < 3 {
        return Err("helper needs PROGRAM SCRIPT TOOL [ARG ...]".into());
    }
    let mut command = bead::services::process::command(&args[0]);
    command.arg(&args[1]);
    let request = bead::services::process::HelperRequest {
        id: 1,
        tool: args[2].clone(),
        arguments: args[3..].to_vec(),
        input: None,
    };
    let reply = bead::services::process::helper(&mut command, &request, Default::default())?;
    print!("{}", reply.output);
    if reply.code != 0 {
        return Err(format!("helper code {}", reply.code).into());
    }
    Ok(())
}

pub fn map(mut call: Call) -> Result<Call, Reply> {
    if call.target == "bead" || call.target == "bead.panel" || call.target == "bead.osd" {
        return Ok(call);
    }
    validate(&call)?;
    let method = match (call.target.as_str(), call.method.as_str()) {
        ("nacre", "popout" | "controls")
            if call
                .args
                .first()
                .and_then(|v| v.as_str())
                .is_some_and(|s| s == "sound" || s == "audio") =>
        {
            Some("open")
        }
        ("nacre", "close" | "hide") => Some("close"),
        ("drawers", "toggle") if call.args.first().and_then(|v| v.as_str()) == Some("sound") => {
            Some("toggle")
        }
        _ => None,
    };
    if let Some(method) = method {
        call.target = "bead.panel".into();
        call.method = method.into();
        call.args = vec![Value::String("sound".into())];
    }
    Ok(call)
}
#[cfg(test)]
fn fixture_catalog() -> serde_json::Value {
    serde_json::from_str(include_str!("../../../shell/command-fixtures.json"))
        .expect("compiled fixture catalog")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_audited_route_and_selector_is_typed_and_dispatchable() {
        let c = catalog().unwrap();
        assert_eq!(c.routes.len(), 120);
        for route in &c.routes {
            for p in &route.patterns {
                let args = route
                    .parameters
                    .iter()
                    .map(|p| match p.kind.as_str() {
                        "string" => Value::String("fixture".into()),
                        "int" => Value::from(1),
                        "real" => Value::from(0.5),
                        "bool" => Value::from(true),
                        _ => panic!("unhandled type"),
                    })
                    .collect();
                let call = Call {
                    version: 1,
                    id: 1,
                    target: format!(
                        "{}{}",
                        p.prefix,
                        if p.dynamic { "fixture-output" } else { "" }
                    ),
                    method: route.method.clone(),
                    args,
                    output: Some("fixture-output".into()),
                };
                assert!(c.validate(&call).is_ok(), "{} {}", call.target, call.method);
                let decoded: Call =
                    serde_json::from_slice(&serde_json::to_vec(&call).unwrap()).unwrap();
                assert_eq!(decoded.output, call.output);
                let mut bad = call;
                bad.args.push(Value::Null);
                assert!(c.validate(&bad).is_err());
            }
        }
    }
    #[test]
    fn audited_cli_and_shortcut_ingress_fixtures_use_catalog_routes() {
        let c = catalog().unwrap();
        let f = fixture_catalog();
        assert_eq!(f["audited_cli_locations"].as_array().unwrap().len(), 30);
        assert_eq!(f["shortcuts"].as_array().unwrap().len(), 12);
        for row in f["calls"].as_array().unwrap() {
            let call = Call {
                version: 1,
                id: 1,
                target: row["target"].as_str().unwrap().into(),
                method: row["method"].as_str().unwrap().into(),
                args: row["args"].as_array().unwrap().clone(),
                output: None,
            };
            assert!(c.validate(&call).is_ok(), "{row}");
        }
    }
}
