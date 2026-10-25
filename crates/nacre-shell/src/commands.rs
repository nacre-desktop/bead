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
    /// B0068: a trailing optional argument (`nacre settings [PAGE]`); the audited QML signature has none.
    #[serde(default)]
    pub optional: bool,
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
        let required = route.parameters.iter().filter(|p| !p.optional).count();
        if call.args.len() < required || call.args.len() > route.parameters.len() {
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
    let call = from_strings(target, method, &values)?;
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
/// Same typed legacy call contract for CLI and daemon UI forwarding.
pub fn from_strings(target: String, method: String, values: &[String]) -> Result<Call, String> {
    let c = catalog()?;
    let parameters = c.route(&target, &method).map(|r| r.parameters.as_slice());
    let mut converted = Vec::new();
    for (i, arg) in values.iter().enumerate() {
        let kind = parameters
            .and_then(|p| p.get(i))
            .map(|p| p.kind.as_str())
            .unwrap_or("string");
        converted.push(match kind {
            "int" => Value::from(arg.parse::<i64>().map_err(|_| "invalid integer argument")?),
            "real" => {
                serde_json::to_value(arg.parse::<f64>().map_err(|_| "invalid real argument")?)
                    .map_err(|_| "invalid real argument")?
            }
            "bool" => Value::from(
                arg.parse::<bool>()
                    .map_err(|_| "invalid boolean argument")?,
            ),
            _ => Value::String(arg.clone()),
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
    if !["bead", "bead.panel"].contains(&call.target.as_str()) {
        c.validate(&call)?;
    }
    Ok(call)
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

pub fn map_views(mut call: Call) -> Result<Call, Reply> {
    // Native extras have their own lazy surfaces; menu routes remain engine-owned.
    if call.target == "nacre"
        && call.method == "mode"
        && let Some(mode) = call.args.first().and_then(Value::as_str)
        && matches!(mode, "overview" | "palette" | "keys")
    {
        validate(&call)?;
        let view = if mode == "keys" { "keybindings" } else { mode }.to_owned();
        call.target = "bead.panel".into();
        call.method = "toggle".into();
        call.args = vec![Value::String(view)];
        return Ok(call);
    }

    // The AI sidebar (QML left drawer): `nacre left` and `drawers toggle left` both
    // toggle it, as in the QML shell. It is an on-demand app process (B0052).
    let left = (call.target == "nacre" && call.method == "left")
        || (call.target == "drawers"
            && call.method == "toggle"
            && call.args.first().and_then(Value::as_str) == Some("left"));
    if left {
        validate(&call)?;
        return Err(crate::app::route(call.id, "chat", "toggle", &[]));
    }
    // B0068: `welcomeApp refresh` re-reads the first-run preference after `nacre-welcome
    // --enable/--disable`. Bead's Welcome reads nothing of it (the once-only claim stays with Nacre), so
    // there is nothing to refresh and the app is not started for it.
    if call.target == "welcomeApp" && call.method == "refresh" {
        validate(&call)?;
        return Err(Reply::ok(call.id, Value::Null));
    }
    // B0093: the selector prompt is its own on-demand app (`--app prompt ID`); `cancel` closes it.
    if call.target == "menuPrompt" && matches!(call.method.as_str(), "open" | "cancel") {
        validate(&call)?;
        let ident = call.args[0].as_str().unwrap_or_default().to_owned();
        if !crate::prompt::is_id(&ident) {
            return Err(Reply::error(
                call.id,
                "invalid-route",
                "invalid selector id",
            ));
        }
        return Err(crate::app::prompt(call.id, &call.method, &ident));
    }
    // `drawers toggle launcher|session` are the launcher and session keys; the other QML drawer
    // names (osd preview, pinned flags, preview-only) are QML preview state with no Bead view.
    if call.target == "drawers" && call.method == "toggle" {
        match call.args.first().and_then(Value::as_str) {
            Some("launcher") => {
                validate(&call)?;
                call.target = "nacre".into();
                call.method = "launcher".into();
                call.args = vec![Value::String(String::new())];
                return Ok(call);
            }
            Some("session") => {
                validate(&call)?;
                call.target = "bead.panel".into();
                call.args = vec![Value::String("session".into())];
                return Ok(call);
            }
            Some("osd" | "leftPinned" | "dashboardPinned" | "previewOnly") => {
                validate(&call)?;
                return Err(Reply::error(
                    call.id,
                    "unavailable",
                    "this QML drawer is preview state with no Bead view",
                ));
            }
            _ => {}
        }
    }
    // The control view owns its docked pages. Keep the section for the bar runtime,
    // which selects the dock and sends the page through CompanionModel.
    if call.target == "nacre"
        && call.method == "controls"
        && call.args.first().and_then(Value::as_str) == Some("updates")
    {
        validate(&call)?;
        call.target = "bead.panel".into();
        call.method = "open".into();
        call.args = vec![Value::String("updates".into())];
        return Ok(call);
    }
    if call.target == "nacre"
        && call.method == "controls"
        && call
            .args
            .first()
            .and_then(Value::as_str)
            .is_some_and(|page| {
                matches!(
                    page,
                    "home" | "edit" | "network" | "bluetooth" | "battery" | "audio" | "sound"
                )
            })
    {
        validate(&call)?;
        return Ok(call);
    }
    // B0068: `nacre controls` sections the control centre does not own: `session` is the session view,
    // `notifications` the dashboard's notification page.
    if call.target == "nacre" && call.method == "controls" {
        let view = match call.args.first().and_then(Value::as_str) {
            Some("session") => Some("session".to_owned()),
            Some("notifications") => Some("dashboard-notifications".to_owned()),
            _ => None,
        };
        if let Some(view) = view {
            validate(&call)?;
            call.target = "bead.panel".into();
            call.method = "open".into();
            call.args = vec![Value::String(view)];
            return Ok(call);
        }
    }
    // The QML session menu (`nacre session`) is the top-right power view, not a launcher route.
    if call.target == "nacre" && call.method == "session" {
        validate(&call)?;
        call.target = "bead.panel".into();
        call.method = "toggle".into();
        call.args = vec![Value::String("session".into())];
        return Ok(call);
    }
    // B0045/B0052: `nacre settings [page]` opens the Settings app (deep link by page id).
    if call.target == "nacre"
        && call.method == "settings"
        && call.args.len() <= 1
        && call.args.iter().all(Value::is_string)
    {
        return Err(crate::app::route(call.id, "settings", "open", &call.args));
    }
    // B0060: `colorsApp open [image]` (what `nacre-colors` calls) opens the Colors app; an empty
    // image means "no image". Opening never applies a palette.
    if call.target == "colorsApp"
        && call.method == "open"
        && call.args.len() <= 1
        && call.args.iter().all(Value::is_string)
    {
        let args: Vec<Value> = call
            .args
            .into_iter()
            .filter(|image| image.as_str().is_some_and(|s| !s.is_empty()))
            .collect();
        return Err(crate::app::route(call.id, "colors", "open", &args));
    }
    // B0059: Welcome opens as its own app at a tour page. `welcomeApp open PAGE` is the route
    // Nacre's launcher row and first-login claim already call (it is in the route catalog).
    if call.target == "welcomeApp"
        && call.method == "open"
        && call.args.len() == 1
        && call.args.iter().all(Value::is_string)
    {
        return Err(crate::app::route(call.id, "welcome", "open", &call.args));
    }
    // The QML right sidebar (control center): `drawers toggle right` toggles it and
    // `nacre controls home|edit` opens it.
    let right = (call.target == "drawers"
        && call.method == "toggle"
        && call.args.first().and_then(Value::as_str) == Some("right"))
        || (call.target == "nacre"
            && call.method == "controls"
            && matches!(
                call.args.first().and_then(Value::as_str),
                Some("home" | "edit")
            ));
    if right {
        validate(&call)?;
        call.method = if call.target == "drawers" {
            "toggle"
        } else {
            "open"
        }
        .into();
        call.target = "bead.panel".into();
        call.args = vec![Value::String("control".into())];
        return Ok(call);
    }
    if call.target == "nacre" && call.method == "tab" {
        validate(&call)?;
        let page = call.args[0]
            .as_u64()
            .and_then(|index| usize::try_from(index).ok())
            .and_then(crate::dashboard::Page::from_tab)
            .ok_or_else(|| {
                Reply::error(call.id, "unavailable", "dashboard tab is not implemented")
            })?;
        call.target = "bead.panel".into();
        call.method = "open".into();
        call.args = vec![Value::String(format!("dashboard-{}", page.name()))];
        return Ok(call);
    }
    if call.target == "nacre"
        && matches!(call.method.as_str(), "popout" | "controls")
        && call
            .args
            .first()
            .and_then(Value::as_str)
            .is_some_and(|name| {
                matches!(
                    name,
                    "calendar" | "network" | "bluetooth" | "battery" | "sound" | "audio"
                )
            })
    {
        validate(&call)?;
        let view = match call.args[0].as_str() {
            Some("audio") => Value::String("sound".into()),
            _ => call.args[0].clone(),
        };
        let center = (call.method == "popout").then(|| call.args[1].clone());
        call.target = "bead.panel".into();
        call.method = if center.is_some() { "toggle" } else { "open" }.into();
        call.args = vec![view];
        if let Some(center) = center {
            call.args.push(center);
        }
        return Ok(call);
    }
    map(call)
}
pub fn map_launcher(call: Call) -> Result<Call, Reply> {
    let call = map(call)?;
    if call.target == "bead.panel"
        && call.method != "close"
        && call.args.first().and_then(Value::as_str) == Some("sound")
    {
        return Err(Reply::error(
            call.id,
            "unavailable",
            "sound panel is not configured",
        ));
    }
    Ok(call)
}
pub fn map(mut call: Call) -> Result<Call, Reply> {
    if ["bead", "bead.panel", "bead.osd"].contains(&call.target.as_str()) {
        return Ok(call);
    }
    validate(&call)?;
    if call.target == "nacre" && call.method == "hide" {
        // QML toggles one shell-wide hidden flag; panels keep their state.
        call.target = "bead.panel".into();
        call.args.clear();
        return Ok(call);
    }
    if call.target == "pearls" {
        return map_pearls(call);
    }
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
        ("nacre", "close") => Some("close"),
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
/// B0048: Nacre's `pearls` routes onto the panel layer. A QML pearl kind names the
/// view that fills it; pearls are numbered, and `dock`/`close` take that number.
fn map_pearls(mut call: Call) -> Result<Call, Reply> {
    let id = call.id;
    let (method, args) = match call.method.as_str() {
        "detach" => ("detach", vec![]),
        "state" => ("pearls", vec![]),
        "close" if call.args.first().and_then(Value::as_str) == Some("all") => {
            ("pearls-reset", vec![])
        }
        "open" => {
            let kind = call.args[0].as_str().unwrap_or_default();
            let view = match kind {
                "launcher" => "launcher".to_owned(),
                "ai" => "chat".to_owned(),
                "dashboard" => format!(
                    "dashboard-{}",
                    crate::dashboard::Page::from_tab(0).map_or("today", |p| p.name())
                ),
                "media" => "dashboard-media".to_owned(),
                // B0068: the control centre exists since B0049; the pearl policy still decides.
                "controls" => "control".to_owned(),
                _ => {
                    return Err(Reply::error(
                        id,
                        "unavailable",
                        "no panel for this pearl kind",
                    ));
                }
            };
            ("pearl-open", vec![Value::String(view)])
        }
        method @ ("dock" | "close") => {
            let number = call.args[0].as_str().and_then(|s| s.parse::<u64>().ok());
            let number = number.ok_or_else(|| {
                Reply::error(id, "invalid-route", "a pearl is named by its number")
            })?;
            (
                if method == "dock" {
                    "dock"
                } else {
                    "pearl-close"
                },
                vec![Value::from(number)],
            )
        }
        _ => {
            return Err(Reply::error(
                id,
                "unavailable",
                "the native gesture trace does not exist",
            ));
        }
    };
    call.target = "bead.panel".into();
    call.method = method.into();
    call.args = args;
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
    fn pearls_close_all_resets_open_and_remembered_pearls() {
        let call = from_strings("pearls".into(), "close".into(), &["all".into()]).unwrap();
        let mapped = map(call).unwrap();
        assert_eq!(mapped.target, "bead.panel");
        assert_eq!(mapped.method, "pearls-reset");
        assert!(mapped.args.is_empty());
    }
    #[test]
    fn hide_is_one_shell_wide_toggle_and_close_still_closes_the_panel() {
        let hide = from_strings("nacre".into(), "hide".into(), &[]).unwrap();
        for mapped in [
            map(hide.clone()).unwrap(),
            map_views(hide.clone()).unwrap(),
            map_launcher(hide).unwrap(),
        ] {
            assert_eq!(
                (mapped.target.as_str(), mapped.method.as_str()),
                ("bead.panel", "hide")
            );
            assert!(mapped.args.is_empty());
        }
        let close = from_strings("nacre".into(), "close".into(), &[]).unwrap();
        let mapped = map(close).unwrap();
        assert_eq!(mapped.method, "close");
    }
    #[test]
    fn welcome_open_is_an_app_route() {
        let open = from_strings("welcomeApp".into(), "open".into(), &["apps".into()]).unwrap();
        // The shell is not configured in unit tests: the app route answers, nothing else does.
        assert_eq!(
            map_views(open).unwrap_err().error.unwrap().code,
            "unavailable"
        );
    }
    #[test]
    fn updates_control_route_opens_the_status_popout() {
        let call = from_strings("nacre".into(), "controls".into(), &["updates".into()]).unwrap();
        let mapped = map_views(call).unwrap();
        assert_eq!(
            (mapped.target.as_str(), mapped.method.as_str()),
            ("bead.panel", "open")
        );
        assert_eq!(mapped.args, vec![Value::String("updates".into())]);
    }
    #[test]
    fn core_dashboard_tab_routes_keep_canonical_indices_and_refuse_extras() {
        for (index, page) in crate::dashboard::Page::ALL.into_iter().enumerate() {
            let call = from_strings("nacre".into(), "tab".into(), &[index.to_string()]).unwrap();
            let mapped = map_views(call).unwrap();
            assert_eq!(mapped.target, "bead.panel");
            assert_eq!(mapped.method, "open");
            assert_eq!(
                mapped.args,
                vec![Value::String(format!("dashboard-{}", page.name()))]
            );
            assert_eq!(
                crate::dashboard::Page::from_view(mapped.args[0].as_str().unwrap()),
                Some(page)
            );
            assert_eq!(page.tab(), index);
        }
        for index in ["-1", "12", "9223372036854775807"] {
            let call = from_strings("nacre".into(), "tab".into(), &[index.into()]).unwrap();
            assert!(map_views(call).is_err());
        }
        assert_eq!(crate::dashboard::Page::from_view("dashboard-unknown"), None);
    }
    #[test]
    fn calendar_view_keeps_the_audited_legacy_center_and_launcher_routes() {
        let call = from_strings(
            "nacre".into(),
            "popout".into(),
            &["calendar".into(), "1812.5".into()],
        )
        .unwrap();
        let mapped = map_views(call).unwrap();
        assert_eq!(
            (mapped.target.as_str(), mapped.method.as_str()),
            ("bead.panel", "toggle")
        );
        assert_eq!(
            mapped.args,
            vec![Value::String("calendar".into()), Value::from(1812.5)]
        );
        let launcher = from_strings("nacre".into(), "launcher".into(), &["a".into()]).unwrap();
        assert_eq!(map_views(launcher).unwrap().target, "nacre");
        let sound = from_strings("nacre".into(), "controls".into(), &["sound".into()]).unwrap();
        assert_eq!(
            map_views(sound).unwrap().args,
            vec![Value::String("sound".into())]
        );
    }
    #[test]
    fn read_only_status_routes_keep_named_views_and_their_popout_centers() {
        for view in ["network", "bluetooth", "battery"] {
            let call = from_strings(
                "nacre".into(),
                "popout".into(),
                &[view.into(), "1512.5".into()],
            )
            .unwrap();
            let mapped = map_views(call).unwrap();
            assert_eq!(
                (mapped.target.as_str(), mapped.method.as_str()),
                ("bead.panel", "toggle")
            );
            assert_eq!(
                mapped.args,
                vec![Value::String(view.into()), Value::from(1512.5)]
            );
            let call = from_strings("nacre".into(), "controls".into(), &[view.into()]).unwrap();
            let mapped = map_views(call).unwrap();
            if view == "calendar" {
                assert_eq!(mapped.method, "open");
            } else {
                assert_eq!(mapped.target, "nacre");
                assert_eq!(mapped.method, "controls");
            }
            assert_eq!(mapped.args, vec![Value::String(view.into())]);
        }
        for route in ["sound", "audio"] {
            let call = from_strings(
                "nacre".into(),
                "popout".into(),
                &[route.into(), "1512.5".into()],
            )
            .unwrap();
            let mapped = map_views(call).unwrap();
            assert_eq!(mapped.method, "toggle");
            assert_eq!(
                mapped.args,
                vec![Value::String("sound".into()), Value::from(1512.5)]
            );
        }
    }
    #[test]
    fn a_launcher_host_never_routes_sound_commands_into_its_panel() {
        let call = Call {
            version: 1,
            id: 1,
            target: "nacre".into(),
            method: "controls".into(),
            args: vec![Value::String("sound".into())],
            output: None,
        };
        assert_eq!(map(call.clone()).unwrap().target, "bead.panel");
        assert!(map_launcher(call).is_err());
        let call = Call {
            version: 1,
            id: 2,
            target: "nacre".into(),
            method: "launcher".into(),
            args: vec![Value::String("a".into())],
            output: None,
        };
        let result = map_launcher(call).unwrap();
        assert_eq!(result.target, "nacre");
        assert_eq!(result.args[0], "a");
    }
    #[test]
    fn every_audited_route_and_selector_is_typed_and_dispatchable() {
        let c = catalog().unwrap();
        assert_eq!(c.routes.len(), 121);
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
