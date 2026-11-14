//! Read menu items from compiled Interface Builder files inside an app bundle,
//! so menus of apps that are not running can be listed.
//! Supports the "NIBArchive" binary format and keyed-archive binary plists.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct NibMenuItem {
    pub menu: String,
    pub title: String,
    pub key_equiv: String,
    /// NSEvent modifier flags; None when the file does not say.
    pub mod_mask: Option<i64>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
enum Val {
    Int(i64),
    Bool(bool),
    Float(f64),
    Data(Vec<u8>),
    Nil,
    Obj(usize),
}

struct NibObj {
    class: String,
    values: Vec<(String, Val)>,
}

fn varint(b: &[u8], p: &mut usize) -> Option<u64> {
    let mut r: u64 = 0;
    let mut shift = 0;
    loop {
        let byte = *b.get(*p)?;
        *p += 1;
        r |= ((byte & 0x7f) as u64) << shift;
        shift += 7;
        if byte & 0x80 != 0 || shift > 56 {
            break;
        }
    }
    Some(r)
}

fn u32le(b: &[u8], p: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(p..p + 4)?.try_into().ok()?))
}

fn parse_nib_archive(b: &[u8]) -> Option<Vec<NibObj>> {
    if !b.starts_with(b"NIBArchive") {
        return None;
    }
    let h = 18;
    let obj_count = u32le(b, h)? as usize;
    let obj_off = u32le(b, h + 4)? as usize;
    let key_count = u32le(b, h + 8)? as usize;
    let key_off = u32le(b, h + 12)? as usize;
    let val_count = u32le(b, h + 16)? as usize;
    let val_off = u32le(b, h + 20)? as usize;
    let class_count = u32le(b, h + 24)? as usize;
    let class_off = u32le(b, h + 28)? as usize;

    let mut keys = Vec::with_capacity(key_count);
    let mut p = key_off;
    for _ in 0..key_count {
        let len = varint(b, &mut p)? as usize;
        keys.push(String::from_utf8_lossy(b.get(p..p + len)?).into_owned());
        p += len;
    }

    let mut classes = Vec::with_capacity(class_count);
    let mut p = class_off;
    for _ in 0..class_count {
        let len = varint(b, &mut p)? as usize;
        let extra = varint(b, &mut p)? as usize;
        p += extra * 4;
        let raw = b.get(p..p + len)?;
        classes.push(String::from_utf8_lossy(raw).trim_end_matches('\0').to_string());
        p += len;
    }

    let mut vals = Vec::with_capacity(val_count);
    let mut p = val_off;
    for _ in 0..val_count {
        let k = varint(b, &mut p)? as usize;
        let t = *b.get(p)?;
        p += 1;
        let v = match t {
            0 => { let v = *b.get(p)? as i8 as i64; p += 1; Val::Int(v) }
            1 => { let v = i16::from_le_bytes(b.get(p..p + 2)?.try_into().ok()?) as i64; p += 2; Val::Int(v) }
            2 => { let v = i32::from_le_bytes(b.get(p..p + 4)?.try_into().ok()?) as i64; p += 4; Val::Int(v) }
            3 => { let v = i64::from_le_bytes(b.get(p..p + 8)?.try_into().ok()?); p += 8; Val::Int(v) }
            4 => Val::Bool(false),
            5 => Val::Bool(true),
            6 => { let v = f32::from_le_bytes(b.get(p..p + 4)?.try_into().ok()?) as f64; p += 4; Val::Float(v) }
            7 => { let v = f64::from_le_bytes(b.get(p..p + 8)?.try_into().ok()?); p += 8; Val::Float(v) }
            8 => { let len = varint(b, &mut p)? as usize; let d = b.get(p..p + len)?.to_vec(); p += len; Val::Data(d) }
            9 => Val::Nil,
            10 => { let v = u32le(b, p)? as usize; p += 4; Val::Obj(v) }
            _ => return None,
        };
        vals.push((keys.get(k).cloned().unwrap_or_default(), v));
    }

    let mut objs = Vec::with_capacity(obj_count);
    let mut p = obj_off;
    for _ in 0..obj_count {
        let c = varint(b, &mut p)? as usize;
        let start = varint(b, &mut p)? as usize;
        let n = varint(b, &mut p)? as usize;
        let values = vals.get(start..start + n).map(|s| s.to_vec()).unwrap_or_default();
        objs.push(NibObj { class: classes.get(c).cloned().unwrap_or_default(), values });
    }
    Some(objs)
}

fn get<'a>(o: &'a NibObj, key: &str) -> Option<&'a Val> {
    o.values.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

fn string_of(objs: &[NibObj], v: Option<&Val>) -> Option<String> {
    match v? {
        Val::Obj(i) => {
            let o = objs.get(*i)?;
            match get(o, "NS.bytes")? {
                Val::Data(d) => Some(String::from_utf8_lossy(d).into_owned()),
                _ => None,
            }
        }
        Val::Data(d) => Some(String::from_utf8_lossy(d).into_owned()),
        _ => None,
    }
}

fn items_from_nib_archive(b: &[u8]) -> Option<Vec<NibMenuItem>> {
    let objs = parse_nib_archive(b)?;
    let mut out = Vec::new();
    for o in &objs {
        if o.class != "NSMenuItem" {
            continue;
        }
        let key = string_of(&objs, get(o, "NSKeyEquiv")).unwrap_or_default();
        if key.is_empty() {
            continue;
        }
        let title = string_of(&objs, get(o, "NSTitle")).unwrap_or_default();
        let mask = match get(o, "NSKeyEquivModMask") {
            Some(Val::Int(i)) => Some(*i),
            _ => None,
        };
        let menu = match get(o, "NSMenu") {
            Some(Val::Obj(i)) => objs.get(*i).and_then(|m| string_of(&objs, get(m, "NSTitle"))).unwrap_or_default(),
            _ => String::new(),
        };
        out.push(NibMenuItem { menu, title, key_equiv: key, mod_mask: mask });
    }
    Some(out)
}

/// Keyed-archive nib (binary plist with `$objects`).
fn items_from_keyed_archive(b: &[u8]) -> Option<Vec<NibMenuItem>> {
    let root: plist::Value = plist::from_bytes(b).ok()?;
    let objects = root.as_dictionary()?.get("$objects")?.as_array()?;
    let resolve = |v: &plist::Value| -> Option<plist::Value> {
        match v {
            plist::Value::Uid(u) => objects.get(u.get() as usize).cloned(),
            other => Some(other.clone()),
        }
    };
    let as_text = |v: Option<&plist::Value>| -> Option<String> {
        let r = resolve(v?)?;
        match r {
            plist::Value::String(s) => Some(s),
            plist::Value::Dictionary(d) => {
                if let Some(plist::Value::String(s)) = d.get("NS.string") {
                    return Some(s.clone());
                }
                if let Some(plist::Value::Data(x)) = d.get("NS.bytes") {
                    return Some(String::from_utf8_lossy(x).into_owned());
                }
                None
            }
            _ => None,
        }
    };
    let class_name = |d: &plist::Dictionary| -> Option<String> {
        let c = resolve(d.get("$class")?)?;
        c.as_dictionary()?.get("$classname")?.as_string().map(String::from)
    };
    let mut out = Vec::new();
    for o in objects {
        let Some(d) = o.as_dictionary() else { continue };
        if class_name(d).as_deref() != Some("NSMenuItem") {
            continue;
        }
        let key = as_text(d.get("NSKeyEquiv")).unwrap_or_default();
        if key.is_empty() {
            continue;
        }
        let title = as_text(d.get("NSTitle")).unwrap_or_default();
        let mask = d.get("NSKeyEquivModMask").and_then(|v| v.as_signed_integer());
        let menu = d
            .get("NSMenu")
            .and_then(|m| resolve(m))
            .and_then(|m| m.as_dictionary().and_then(|md| as_text(md.get("NSTitle"))))
            .unwrap_or_default();
        out.push(NibMenuItem { menu, title, key_equiv: key, mod_mask: mask });
    }
    Some(out)
}

pub fn read_menu_items(path: &Path) -> Vec<NibMenuItem> {
    // A .nib folder holds keyedobjects*.nib files.
    let files: Vec<PathBuf> = if path.is_dir() {
        std::fs::read_dir(path)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.path())
                    .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("keyedobjects")))
                    .take(1)
                    .collect()
            })
            .unwrap_or_default()
    } else {
        vec![path.to_path_buf()]
    };
    let mut out = Vec::new();
    for f in files {
        let Ok(b) = std::fs::read(&f) else { continue };
        if b.len() > 20_000_000 {
            continue;
        }
        let items = if b.starts_with(b"NIBArchive") {
            items_from_nib_archive(&b)
        } else if b.starts_with(b"bplist") {
            items_from_keyed_archive(&b)
        } else {
            None
        };
        out.extend(items.unwrap_or_default());
    }
    out
}

/// Menu-like nib files in an app bundle (base or English localisation, storyboards).
pub fn find_menu_nibs(app: &Path) -> Vec<PathBuf> {
    let res = app.join("Contents/Resources");
    let mut dirs = vec![res.clone()];
    for l in ["Base.lproj", "en.lproj", "English.lproj", "res"] {
        dirs.push(res.join(l));
    }
    let mut out = Vec::new();
    let mut seen = HashMap::new();
    for d in dirs {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            let lower = name.to_lowercase();
            if lower.ends_with(".storyboardc") {
                if let Ok(inner) = std::fs::read_dir(&p) {
                    for ie in inner.flatten() {
                        let n = ie.file_name().to_string_lossy().to_lowercase();
                        if n.ends_with(".nib") && n.contains("menu") {
                            out.push(ie.path());
                        }
                    }
                }
            } else if lower.ends_with(".nib") && lower.contains("menu") && seen.insert(lower.clone(), ()).is_none() {
                out.push(p);
            }
        }
    }
    out
}
