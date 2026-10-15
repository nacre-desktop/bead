// SPDX-License-Identifier: GPL-3.0-only
//! Native desktop font selection stays in the application, not Bead's dependency graph.
use bead::render::{Primitive, Result, Scene};
use skrifa::MetadataProvider;
use std::{collections::BTreeSet, ffi::CString, path::PathBuf, time::Instant};
type Selection = (BTreeSet<String>, BTreeSet<char>, Vec<PathBuf>);
#[derive(Default)]
pub struct Selector {
    entries: std::collections::VecDeque<Selection>,
}
impl Selector {
    pub fn clear(&mut self) {
        self.entries.clear();
    }
    pub fn select(&mut self, scene: &Scene) -> Result<Vec<PathBuf>> {
        let _profile = bead::core::profile::Span::new("font_select");
        let mut families = BTreeSet::new();
        let mut chars = BTreeSet::new();
        for p in &scene.primitives {
            if let Primitive::Text { family, text, .. } = p {
                families.insert(family.clone());
                chars.extend(text.chars().filter(|c| !c.is_control()));
            }
        }
        if let Some((_, _, paths)) = self
            .entries
            .iter()
            .find(|(f, c, _)| *f == families && c.is_superset(&chars))
        {
            return Ok(paths.clone());
        }
        let paths = select(scene)?;
        if families.len() <= 32 && chars.len() <= 4096 {
            self.entries.push_back((families, chars, paths.clone()));
            if self.entries.len() > 8 {
                self.entries.pop_front();
            }
        } else {
            self.entries.clear();
        }
        Ok(paths)
    }
}
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
