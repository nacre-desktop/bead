// SPDX-License-Identifier: MIT OR Apache-2.0
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let lua = mlua::Lua::new();
    for name in ["panel.luau", "behavior.luau"] {
        let text = std::fs::read_to_string(root.join(name))?;
        lua.load(&text).set_name(name).into_function()?;
        println!(
            "{name}: {} physical lines; Luau syntax valid",
            text.lines().count()
        );
    }
    let text = std::fs::read_to_string(root.join("panel.kdl"))?;
    let _: kdl::KdlDocument = text.parse()?;
    println!(
        "panel.kdl: {} physical lines; KDL v2 syntax valid",
        text.lines().count()
    );
    let text = std::fs::read_to_string(root.join("panel.bqml"))?;
    println!(
        "panel.bqml: {} physical lines; proposed grammar, not parsed",
        text.lines().count()
    );
    Ok(())
}
