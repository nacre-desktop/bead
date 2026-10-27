// SPDX-License-Identifier: GPL-3.0-only
//! Native desktop font selection stays in the application, not Bead's dependency graph.
use bead::render::{Primitive, Result, Scene};
use skrifa::MetadataProvider;
use std::{collections::BTreeSet, ffi::CString, path::PathBuf, time::Instant};
const OUTFIT: &[u8] = include_bytes!("../assets/fonts/Outfit-Variable.ttf");
static OUTFIT_PATH: std::sync::OnceLock<std::result::Result<PathBuf, String>> =
    std::sync::OnceLock::new();
static OUTFIT_REGISTERED: std::sync::OnceLock<std::result::Result<(), String>> =
    std::sync::OnceLock::new();
#[link(name = "fontconfig")]
unsafe extern "C" {
    fn FcConfigGetCurrent() -> *mut std::ffi::c_void;
    fn FcConfigAppFontAddFile(config: *mut std::ffi::c_void, file: *const u8) -> i32;
    fn FcConfigBuildFonts(config: *mut std::ffi::c_void) -> i32;
}

fn bundled_outfit_path() -> Result<PathBuf> {
    OUTFIT_PATH
        .get_or_init(|| -> std::result::Result<PathBuf, String> {
            let cache = std::env::var_os("XDG_CACHE_HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
                .unwrap_or_else(std::env::temp_dir)
                .join("bead/fonts");
            std::fs::create_dir_all(&cache).map_err(|error| error.to_string())?;
            let path = cache.join("Outfit-Variable.ttf");
            if std::fs::metadata(&path).is_ok_and(|metadata| metadata.len() == OUTFIT.len() as u64)
            {
                return Ok(path);
            }
            let temporary = cache.join(format!(".Outfit-{}.ttf", std::process::id()));
            std::fs::write(&temporary, OUTFIT).map_err(|error| error.to_string())?;
            std::fs::rename(&temporary, &path).map_err(|error| error.to_string())?;
            Ok(path)
        })
        .clone()
        .map_err(|error| error.into())
        .and_then(|path| {
            OUTFIT_REGISTERED
                .get_or_init(|| {
                    let filename = std::ffi::CString::new(path.to_string_lossy().as_bytes())
                        .map_err(|error| error.to_string())?;
                    // Register this bundled family only in the shell process; never install a
                    // system font or alter the host Fontconfig configuration.
                    let config = unsafe { FcConfigGetCurrent() };
                    if config.is_null()
                        || unsafe { FcConfigAppFontAddFile(config, filename.as_ptr().cast()) } == 0
                        || unsafe { FcConfigBuildFonts(config) } == 0
                    {
                        return Err("Fontconfig rejected bundled Outfit".to_owned());
                    }
                    Ok(())
                })
                .clone()
                .map(|()| path)
                .map_err(Into::into)
        })
}
type Selection = (BTreeSet<(String, u16)>, BTreeSet<char>);
#[link(name = "fontconfig")]
unsafe extern "C" {
    fn FcWeightFromOpenType(weight: std::ffi::c_int) -> std::ffi::c_int;
}
pub struct Selector {
    entries: bead::core::cache::Lru<Selection, Vec<PathBuf>>,
    ascii: bool,
}
impl Default for Selector {
    fn default() -> Self {
        Self {
            entries: bead::core::cache::Lru::new("font-selector", 512 * 1024),
            ascii: false,
        }
    }
}
impl Selector {
    /// Select for printable ASCII as well, so the first typed query reuses the opening
    /// selection instead of running Fontconfig again on the input path.
    pub fn with_ascii(mut self) -> Self {
        self.ascii = true;
        self
    }
    pub fn clear(&mut self) {
        self.entries.clear();
    }
    pub fn select(&mut self, scene: &Scene) -> Result<Vec<PathBuf>> {
        let _profile = bead::core::profile::Span::new("font_select");
        let mut families = BTreeSet::new();
        let mut chars = BTreeSet::new();
        for p in &scene.primitives {
            if let Primitive::Text {
                family,
                text,
                weight,
                ..
            } = p
            {
                families.insert((family.clone(), *weight));
                chars.extend(text.chars().filter(|c| !c.is_control()));
            }
        }
        if self.ascii {
            chars.extend(' '..='~');
        }
        let matched = self
            .entries
            .keys()
            .find(|(f, c)| *f == families && c.is_superset(&chars))
            .cloned();
        if let Some(key) = matched {
            return Ok(self
                .entries
                .get(&key)
                .ok_or("font selection evicted")?
                .clone());
        }
        let paths = select_chars(scene, self.ascii)?;
        if families.len() <= 32 && chars.len() <= 4096 {
            let bytes = families
                .iter()
                .map(|(f, _)| f.capacity() + 64)
                .sum::<usize>()
                + chars.len() * 40
                + paths
                    .iter()
                    .map(|p| p.as_os_str().len() + 32)
                    .sum::<usize>()
                + 128;
            // Admission failure merely disables this optional acceleration.
            let _ = self.entries.insert((families, chars), paths.clone(), bytes);
            if self.entries.len() > 8 {
                self.entries.pop_lru();
            }
        } else {
            self.entries.clear();
        }
        Ok(paths)
    }
}
pub fn select(scene: &Scene) -> Result<Vec<PathBuf>> {
    select_chars(scene, false)
}
fn select_chars(scene: &Scene, ascii: bool) -> Result<Vec<PathBuf>> {
    let start = Instant::now();
    let mut families = BTreeSet::new();
    let mut needed = BTreeSet::new();
    for p in &scene.primitives {
        if let Primitive::Text {
            family,
            text,
            weight,
            ..
        } = p
        {
            families.insert((family.as_str(), *weight));
            needed.extend(text.chars().filter(|c| !c.is_control()));
        }
    }
    if families.is_empty() {
        return Ok(Vec::new());
    }
    if ascii {
        needed.extend(' '..='~');
    }
    let fc = fontconfig::Fontconfig::new().ok_or("native fontconfig unavailable")?;
    let mut paths = BTreeSet::new();
    let mut db = fontdb::Database::new();
    for (family, weight) in &families {
        if family.eq_ignore_ascii_case("Outfit") {
            load(&mut db, &mut paths, bundled_outfit_path()?, &mut needed)?;
            continue;
        }
        let mut pattern = fontconfig::Pattern::new(&fc)?;
        pattern.add_string(fontconfig::FC_FAMILY, &CString::new(*family)?)?;
        pattern.add_integer(c"weight", unsafe { FcWeightFromOpenType((*weight).into()) })?;
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
        let fallback_family = families
            .iter()
            .map(|family| family.0)
            .find(|family| !family.eq_ignore_ascii_case("Outfit"))
            .unwrap_or("sans-serif");
        pattern.add_string(fontconfig::FC_FAMILY, &CString::new(fallback_family)?)?;
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
