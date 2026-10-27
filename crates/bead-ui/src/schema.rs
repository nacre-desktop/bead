// SPDX-License-Identifier: MIT OR Apache-2.0
//! Analyzer module generated directly from the Rust constructors/property schema.
pub fn luau() -> String {
    let mut out = String::from(
        "-- Generated from bead-ui Rust schema; do not edit.\nexport type Node = {kind:string, props:{[string]:any}, children:{Node}}\nexport type Props = {\n    [number]:Node,\n",
    );
    for p in crate::layout::PROPERTIES {
        out.push_str(&format!("    {p}:any?,\n"));
    }
    out.push_str("}\nexport type SDK = {\n");
    for (name, _) in crate::layout::Widget::constructors() {
        out.push_str(&format!("    {name}:(Props)->Node,\n"));
    }
    for (name, ty) in [
        ("Repeat", "(any)->Node"),
        ("Host", "(any)->Node"),
        ("bind", "(string)->any"),
        ("action", "(string,any?)->any"),
        ("animate", "(string,number,number,string)->number"),
        ("scalar_target", "(any)->any"),
        ("alpha", "(string,number)->any"),
        ("tint", "(string,string,number)->any"),
        ("component", "(string,any,any)->any"),
        ("memo", "(string,any,any)->Node"),
        ("resolve_for_check", "(any)->any"),
        ("preview_value", "(string,number)->number"),
        (
            "text_width",
            "(string|{__bind:string},string,number)->number",
        ),
        ("model_count", "(any)->number"),
        ("tr", "(string,any?)->string"),
    ] {
        out.push_str(&format!("    {name}:{ty},\n"));
    }
    out.push_str("}\nlocal sdk:SDK = {} :: any\nreturn sdk\n");
    out
}
#[cfg(test)]
mod tests {
    #[test]
    fn generated_schema_covers_every_constructor_and_property() {
        let text = super::luau();
        for (name, _) in crate::layout::Widget::constructors() {
            assert!(text.contains(&format!("{name}:(Props)")));
        }
        for name in crate::layout::PROPERTIES {
            assert!(text.contains(&format!("{name}:any?")));
        }
    }
}
