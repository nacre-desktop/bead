// SPDX-License-Identifier: GPL-3.0-only
//! B0093: the request file format, the responder call shape, and a golden for each mode.
use super::*;
use bead_test::Harness;
use std::os::unix::fs::PermissionsExt;

pub(crate) const IDENT: &str = "0123456789abcdef0123456789abcdef";

pub(crate) fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell")
}
/// What `menu-cli read-request` prints for a pick request (the helper has validated and normalised it).
pub(crate) fn pick_json(expires: f64) -> String {
    json!({"version":1,"id":IDENT,"kind":"pick","title":"Choose a network","initial":"","expires":expires,"choices":[
        {"label":"Home Wi-Fi","value":"ssid-home","description":"WPA2 · 92%","icon":"wifi","disabled":false},
        {"label":"Guest","value":"ssid-guest","description":"Open · 71%","icon":"wifi","disabled":false},
        {"label":"Office (out of range)","value":"ssid-office","description":"Saved","icon":"wifi_off","disabled":true},
        {"label":"Workshop","value":"ssid-workshop","description":"WPA3 · 48%","icon":"wifi","disabled":false}]})
    .to_string()
}
pub(crate) fn input_json(expires: f64) -> String {
    json!({"version":1,"id":IDENT,"kind":"input","title":"Name this workspace","initial":"Work","expires":expires,"choices":[]}).to_string()
}
/// A stand-in for `nacre-helper`: `read-request` prints `request.json`, `respond` records its argument
/// vector and stdin (one line per answer).
pub(crate) struct Fake {
    pub dir: PathBuf,
    pub helper: PathBuf,
}
impl Fake {
    pub fn new(name: &str, request: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("bead-prompt-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("request.json"), request).unwrap();
        let helper = dir.join("helper.sh");
        std::fs::write(
            &helper,
            format!(
                "#!/bin/sh\nd={}\ncase \"$2\" in\n read-request) cat $d/request.json;;\n respond) printf '%s\\n' \"$*\" >> $d/calls; cat >> $d/responses;;\nesac\n",
                dir.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o700)).unwrap();
        Self { dir, helper }
    }
    pub fn calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.join("calls"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }
    pub fn responses(&self) -> Vec<Json> {
        std::fs::read_to_string(self.dir.join("responses"))
            .unwrap_or_default()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }
    pub fn prompt(&self) -> Prompt {
        Prompt::new(
            self.helper.clone(),
            IDENT.into(),
            Request::read(&self.helper, IDENT).unwrap(),
        )
    }
}
impl Drop for Fake {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
pub(crate) fn far() -> f64 {
    now() + 600.
}
fn stores(pairs: &[(&str, Value)]) -> Stores {
    Stores::default()
        .reconcile(
            &pairs
                .iter()
                .map(|(k, v)| ((*k).to_owned(), v.clone()))
                .collect(),
        )
        .unwrap()
}

#[test]
fn the_request_is_read_through_the_helper_with_its_choices() {
    let fake = Fake::new("read", &pick_json(far()));
    let request = Request::read(&fake.helper, IDENT).unwrap();
    assert!(!request.input);
    assert_eq!(request.title, "Choose a network");
    assert_eq!(request.choices.len(), 4);
    assert_eq!(request.choices[1].value, "ssid-guest");
    assert_eq!(request.choices[2].icon, "wifi_off");
    assert!(request.choices[2].disabled);
    let input = Request::parse(&input_json(far())).unwrap();
    assert!(input.input && input.initial == "Work" && input.choices.is_empty());
    // Bad ids and malformed requests never reach the helper / the view.
    assert!(Request::read(&fake.helper, "../etc").is_err());
    assert!(Request::parse("{}").is_err());
    assert!(Request::parse(r#"{"kind":"other","expires":1}"#).is_err());
    assert!(Request::parse(r#"{"kind":"pick","expires":1,"choices":[{"value":"x"}]}"#).is_err());
}

#[test]
fn the_view_gets_row_ids_not_values() {
    let fake = Fake::new("stores", &pick_json(far()));
    let out = fake.prompt().stores();
    assert_eq!(out["prompt.kind"], Value::Text("pick".into()));
    let Value::List(rows) = &out["prompt.rows"] else {
        panic!()
    };
    assert_eq!(rows[0]["id"], "c0");
    assert_eq!(rows[3]["name"], "Workshop");
    assert!(
        !format!("{rows:?}").contains("ssid-"),
        "values must stay on the host"
    );
}

#[test]
fn choosing_answers_through_the_responder_exactly_once() {
    let fake = Fake::new("choose", &pick_json(far()));
    let prompt = fake.prompt();
    let none = Stores::default();
    // A disabled row, a row that does not exist and a submit on a pick are ignored.
    for ignored in ["choose:c2", "choose:c9", "choose:guest", "submit"] {
        let changes = prompt.act(ignored, &none);
        assert!(!changes.contains_key("panel.open"), "{ignored}");
    }
    assert!(fake.calls().is_empty());
    let changes = prompt.act("choose:c1", &none);
    assert_eq!(changes["panel.open"], Value::Bool(false));
    // Answers after the first are dropped; so is the cancel of the later close.
    prompt.act("choose:c0", &none);
    prompt.act("cancel", &none);
    drop(prompt);
    assert_eq!(fake.calls(), [format!("menu-cli respond {IDENT}")]);
    assert_eq!(
        fake.responses(),
        [json!({"value":"ssid-guest","cancelled":false,"error":""})]
    );
}

#[test]
fn input_submits_the_typed_text_and_truncates_to_the_responder_limit() {
    let fake = Fake::new("input", &input_json(far()));
    let prompt = fake.prompt();
    assert_eq!(prompt.stores()["prompt.input"], Value::Text("Work".into()));
    // "choose" is for picks only.
    prompt.act("choose:c0", &Stores::default());
    assert!(fake.calls().is_empty());
    let typed = "é".repeat(MAX_VALUE + 50);
    let changes = prompt.act("submit", &stores(&[("prompt.input", Value::Text(typed))]));
    assert_eq!(changes["panel.open"], Value::Bool(false));
    let answer = &fake.responses()[0];
    assert_eq!(answer["cancelled"], false);
    assert_eq!(answer["value"].as_str().unwrap().chars().count(), MAX_VALUE);
}

#[test]
fn cancel_and_dropping_an_unanswered_prompt_answer_cancelled() {
    let fake = Fake::new("cancel", &pick_json(far()));
    let prompt = fake.prompt();
    prompt.act("cancel", &Stores::default());
    assert_eq!(
        fake.responses(),
        [json!({"value":"","cancelled":true,"error":""})]
    );
    let again = Fake::new("drop", &pick_json(far()));
    drop(again.prompt());
    assert_eq!(
        again.responses(),
        [json!({"value":"","cancelled":true,"error":""})]
    );
}

#[test]
fn the_deadline_answers_cancelled_with_an_error_and_closes() {
    let fake = Fake::new("expired", &pick_json(now() - 1.));
    let prompt = Prompt::new(
        fake.helper.clone(),
        IDENT.into(),
        Request::parse(&pick_json(now() - 1.)).unwrap(),
    );
    let changes = prompt.project(&Stores::default());
    assert_eq!(changes["panel.open"], Value::Bool(false));
    let answer = &fake.responses()[0];
    assert_eq!(answer["cancelled"], true);
    assert_eq!(answer["error"], "Selector timed out");
}

#[test]
fn a_new_filter_restarts_the_selection_and_scroll() {
    let fake = Fake::new("filter", &pick_json(far()));
    let prompt = fake.prompt();
    let first = prompt.project(&Stores::default());
    assert!(first.is_empty(), "{first:?}");
    let typed = prompt.project(&stores(&[("prompt.query", Value::Text("gu".into()))]));
    assert_eq!(typed["prompt.selected"], Value::Text(String::new()));
    assert_eq!(typed["prompt.scroll"], Value::Number(0.));
    let same = prompt.project(&stores(&[("prompt.query", Value::Text("gu".into()))]));
    assert!(same.is_empty());
}

// ---- goldens -------------------------------------------------------------------

fn harness(out: BTreeMap<String, Value>) -> bead_test::Result<Harness> {
    crate::i18n::init();
    Harness::load(
        &root(),
        Path::new("prompt/main.luau"),
        &root().join("palette.json"),
        out,
        bead_test::Config {
            width: 1264,
            height: 720,
            scale: 1.,
        },
    )
}
fn golden(name: &str, h: &mut Harness) -> bead_test::Result<()> {
    let frame = h.render()?;
    let path = root().join(format!("tests/goldens/prompt-{name}-1.png"));
    if std::env::var("BEAD_UPDATE_GOLDENS").as_deref() == Ok("1") {
        frame.save(&path)?;
    }
    frame.compare(
        &path,
        Default::default(),
        &std::env::temp_dir().join(format!("bead-prompt-{name}-diff")),
    )
}

#[test]
fn pick_filtered_and_input_prompts_match_goldens() -> bead_test::Result<()> {
    let pick = Fake::new("golden-pick", &pick_json(far()))
        .prompt()
        .stores();
    let input = Fake::new("golden-input", &input_json(far()))
        .prompt()
        .stores();
    let mut h = harness(pick.clone())?;
    let ids: Vec<String> = h
        .session
        .view_nodes()
        .iter()
        .map(|n| n.id.clone())
        .collect();
    assert!(ids.iter().any(|i| i == "prompt.search"), "{ids:?}");
    golden("pick", &mut h)?;
    // The selection follows the filter: "wor" ranks Workshop first and highlights it.
    let mut filtered = pick;
    filtered.insert("prompt.query".into(), Value::Text("wor".into()));
    golden("filtered", &mut harness(filtered)?)?;
    let mut h = harness(input)?;
    let ids: Vec<String> = h
        .session
        .view_nodes()
        .iter()
        .map(|n| n.id.clone())
        .collect();
    assert!(ids.iter().any(|i| i == "prompt.value"), "{ids:?}");
    golden("input", &mut h)
}
