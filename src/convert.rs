//! Converts `a{sv}` dicts from [`crate::proxy`] into the JSON shape
//! `gnome-software-plugin`'s C side already parses with json-glib.
//!
//! JSON is no longer the inter-process contract (the bus carries typed
//! `a{sv}`) — it is purely a detail of this shim, kept so the C plugin's
//! existing `json_parser_*`/`gs_modulix_json_*` call sites need no rewrite.

use serde_json::{Map, Value as Json};
use zbus::zvariant::Value;

use crate::proxy::Dict;

/// Converts one `a{sv}` row (an app/plugin entry) to a flat JSON object.
/// `screenshots` (enrichment entries only) gets the special reshaping in
/// [`shots_to_json`] — every other value is a scalar and converts directly.
pub fn dict_to_json(dict: Dict) -> Json {
    let mut map = Map::new();
    for (k, v) in dict {
        let json = if k == "screenshots" {
            shots_to_json(&v)
        } else {
            value_to_json(&v)
        };
        map.insert(k, json);
    }
    Json::Object(map)
}

fn value_to_json(v: &Value) -> Json {
    match v {
        Value::U8(x) => Json::from(*x),
        Value::Bool(x) => Json::from(*x),
        Value::I16(x) => Json::from(*x),
        Value::U16(x) => Json::from(*x),
        Value::I32(x) => Json::from(*x),
        Value::U32(x) => Json::from(*x),
        Value::I64(x) => Json::from(*x),
        Value::U64(x) => Json::from(*x),
        Value::F64(x) => Json::from(*x),
        Value::Str(x) => Json::from(x.as_str()),
        Value::Signature(x) => Json::from(x.to_string()),
        Value::ObjectPath(x) => Json::from(x.as_str()),
        Value::Value(inner) => value_to_json(inner),
        Value::Array(arr) => Json::Array(arr.iter().map(value_to_json).collect()),
        Value::Dict(dict) => {
            let mut map = Map::new();
            for (k, val) in dict.iter() {
                if let Value::Str(key) = k {
                    map.insert(key.as_str().to_string(), value_to_json(val));
                }
            }
            Json::Object(map)
        }
        Value::Structure(s) => Json::Array(s.fields().iter().map(value_to_json).collect()),
        // Fd, and Maybe when the "gvariant" feature is on: never emitted by
        // the daemon (see `modulix-daemon/src/store/entry.rs`).
        _ => Json::Null,
    }
}

/// Reconstructs `[{caption, default, images: [{url, width, height}]}]` (the
/// shape `gs_modulix_add_app_screenshots` parses, see
/// `plugin/src/gs-modulix-app.c`) from the daemon's `(sba(suu))` array
/// (see `ShotTuple` in `modulix-daemon/src/store/entry.rs`).
fn shots_to_json(v: &Value) -> Json {
    let Value::Array(arr) = v else {
        return Json::Array(Vec::new());
    };
    Json::Array(
        arr.iter()
            .filter_map(|shot| {
                let Value::Structure(s) = shot else {
                    return None;
                };
                let fields = s.fields();
                let caption = match fields.first() {
                    Some(Value::Str(s)) => s.as_str(),
                    _ => "",
                };
                let is_default = matches!(fields.get(1), Some(Value::Bool(true)));
                let images = match fields.get(2) {
                    Some(Value::Array(imgs)) => Json::Array(
                        imgs.iter()
                            .filter_map(|img| {
                                let Value::Structure(s) = img else {
                                    return None;
                                };
                                let f = s.fields();
                                let url = match f.first() {
                                    Some(Value::Str(s)) => s.as_str(),
                                    _ => return None,
                                };
                                let width = match f.get(1) {
                                    Some(Value::U32(w)) => *w,
                                    _ => 0,
                                };
                                let height = match f.get(2) {
                                    Some(Value::U32(h)) => *h,
                                    _ => 0,
                                };
                                let mut m = Map::new();
                                m.insert("url".into(), Json::from(url));
                                m.insert("width".into(), Json::from(width));
                                m.insert("height".into(), Json::from(height));
                                Some(Json::Object(m))
                            })
                            .collect(),
                    ),
                    _ => Json::Array(Vec::new()),
                };
                let mut m = Map::new();
                m.insert("caption".into(), Json::from(caption));
                m.insert("default".into(), Json::from(is_default));
                m.insert("images".into(), images);
                Some(Json::Object(m))
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn dict_to_json_converts_scalars() {
        let mut dict: Dict = HashMap::new();
        dict.insert("name".into(), Value::from("firefox").try_into().unwrap());
        dict.insert("score".into(), Value::from(42u32).try_into().unwrap());
        dict.insert(
            "flatpak_preferred".into(),
            Value::from(true).try_into().unwrap(),
        );

        let json = dict_to_json(dict);
        assert_eq!(json["name"], Json::from("firefox"));
        assert_eq!(json["score"], Json::from(42));
        assert_eq!(json["flatpak_preferred"], Json::from(true));
    }

    #[test]
    fn dict_to_json_reshapes_screenshots() {
        type ShotTuple = (String, bool, Vec<(String, u32, u32)>);
        let shots: Vec<ShotTuple> = vec![(
            "caption".to_string(),
            true,
            vec![("https://example.org/a.png".to_string(), 800, 600)],
        )];

        let mut dict: Dict = HashMap::new();
        dict.insert("screenshots".into(), Value::from(shots).try_into().unwrap());

        let json = dict_to_json(dict);
        let shot = &json["screenshots"][0];
        assert_eq!(shot["caption"], Json::from("caption"));
        assert_eq!(shot["default"], Json::from(true));
        let image = &shot["images"][0];
        assert_eq!(image["url"], Json::from("https://example.org/a.png"));
        assert_eq!(image["width"], Json::from(800));
        assert_eq!(image["height"], Json::from(600));
    }
}
