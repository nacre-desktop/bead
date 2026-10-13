// SPDX-License-Identifier: GPL-3.0-only
//! Native desktop font selection stays in the application, not Bead's dependency graph.
use bead_render::{Primitive, Result, Scene};
use skrifa::MetadataProvider;
use std::{collections::BTreeSet, ffi::CString, path::PathBuf, time::Instant};
pub fn select(scene: &Scene) -> Result<Vec<PathBuf>> {
    let start = Instant::now();
    let mut families = BTreeSet::new();
    let mut needed = BTreeSet::new();
    for p in &scene.primitives {
        if let Primitive::Text { family, text, .. } = p {
            families.insert(family.as_str());
            needed.extend(text.chars().filter(|c| !c.is_control()));
        }
    }
    if families.is_empty() {
        return Ok(Vec::new());
    }
    let fc = fontconfig::Fontconfig::new().ok_or("native fontconfig unavailable")?;
    let mut paths = BTreeSet::new();
    let mut db = fontdb::Database::new();
    for family in &families {
        let mut pattern = fontconfig::Pattern::new(&fc)?;
        pattern.add_string(fontconfig::FC_FAMILY, &CString::new(*family)?)?;
        let matched = pattern.font_match()?;
        load(
            &mut db,
            &mut paths,
            PathBuf::from(matched.filename()?),
            &mut needed,
        )?;
    }
    while let Some(ch) = needed.pop_first() {
        let mut pattern = fontconfig::Pattern::new(&fc)?;
        pattern.add_string(
            fontconfig::FC_FAMILY,
            &CString::new(*families.first().unwrap())?,
        )?;
        let mut charset = fontconfig::CharSet::new(&fc)?;
        charset.add_char(ch)?;
        pattern.add_charset(charset)?;
        let matched = pattern.font_match()?;
        load(
            &mut db,
            &mut paths,
            PathBuf::from(matched.filename()?),
            &mut needed,
        )?;
    }
    println!(
        "PROFILE stage=font_select ms={:.3} files={} faces={}",
        start.elapsed().as_secs_f64() * 1000.,
        paths.len(),
        db.len()
    );
    Ok(paths.into_iter().collect())
}
fn load(
    db: &mut fontdb::Database,
    paths: &mut BTreeSet<PathBuf>,
    path: PathBuf,
    needed: &mut BTreeSet<char>,
) -> Result<()> {
    if paths.insert(path.clone()) {
        db.load_font_file(path)?;
        for face in db.faces() {
            db.with_face_data(face.id, |bytes, index| {
                if let Ok(font) = skrifa::FontRef::from_index(bytes, index) {
                    let map = font.charmap();
                    needed.retain(|ch| map.map(*ch).is_none());
                }
            });
        }
    }
    Ok(())
}
