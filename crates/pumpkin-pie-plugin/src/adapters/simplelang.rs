//! SimpleLang, the per-player language plugin, over its JSON protocol.
//!
//! The contract is the "Operations" table in SimpleLang's README: every request
//! is a JSON object with an `op`, every reply carries `"ok": true` or `false`.
//! Only `get_lang` and `languages` are ever sent, and both only read.

use serde::{Deserialize, Serialize};

use super::{Adapter, Ask, Viewer};

#[derive(Serialize)]
struct GetLang {
    op: &'static str,
    player: String,
    client_locale: String,
}

/// The fields of a reply that a placeholder reads. The rest is ignored.
#[derive(Deserialize)]
struct Reply {
    ok: bool,
    lang: Option<String>,
}

pub const ADAPTER: Adapter = Adapter {
    namespace: "simplelang",
    plugin: "simplelang",
    names: &[("lang", "The language SimpleLang shows the viewer")],
    ttl_ms: 5_000,
    probe: |ask| reply(ask, br#"{"op":"languages"}"#).is_some(),
    resolve,
};

fn resolve(name: &str, viewer: Option<&Viewer>, ask: Ask<'_>) -> Option<String> {
    if name != "lang" {
        return None;
    }
    let viewer = viewer?;
    // SimpleLang keys a player by the `Debug` text of the id, because that is
    // all its own code has to go on, and reads the client's locale the same
    // way. The locale is what a player who never ran `/lang` is shown.
    let request = serde_json::to_vec(&GetLang {
        op: "get_lang",
        player: format!("{:?}", viewer.uuid),
        client_locale: format!("{:?}", viewer.locale),
    })
    .ok()?;
    reply(ask, &request)?.lang
}

/// One round trip. [`None`] unless SimpleLang answered `"ok": true`.
fn reply(ask: Ask<'_>, request: &[u8]) -> Option<Reply> {
    let reply: Reply = serde_json::from_slice(&ask(request)?).ok()?;
    reply.ok.then_some(reply)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pumpkin_plugin_api::uuid::Uuid;
    use serde_json::{Value, json};

    fn steve() -> Viewer {
        Viewer {
            uuid: Uuid { high: 1, low: 2 },
            locale: "pt_br".to_string(),
        }
    }

    #[test]
    fn the_request_names_the_player_the_way_simplelang_keys_him() {
        let ask = |request: &[u8]| {
            assert_eq!(
                serde_json::from_slice::<Value>(request).unwrap(),
                json!({
                    "op": "get_lang",
                    "player": "Uuid { high: 1, low: 2 }",
                    "client_locale": "\"pt_br\"",
                })
            );
            Some(br#"{"ok":true,"lang":"pt_br"}"#.to_vec())
        };
        assert_eq!(
            resolve("lang", Some(&steve()), &ask).as_deref(),
            Some("pt_br")
        );
    }

    #[test]
    fn a_language_needs_a_viewer_and_a_listed_name() {
        let ask = |_: &[u8]| -> Option<Vec<u8>> { panic!("nothing to ask") };
        assert_eq!(resolve("lang", None, &ask), None);
        assert_eq!(resolve("nonsense", Some(&steve()), &ask), None);
    }

    #[test]
    fn a_missing_plugin_a_refusal_and_garbage_all_resolve_nothing() {
        let missing = |_: &[u8]| None;
        let refused = |_: &[u8]| Some(br#"{"ok":false,"error":"no"}"#.to_vec());
        let garbage = |_: &[u8]| Some(vec![0x0a, 0x02, 0x08, 0x01]);
        let no_lang = |_: &[u8]| Some(br#"{"ok":true}"#.to_vec());
        for ask in [&missing as Ask<'_>, &refused, &garbage, &no_lang] {
            assert_eq!(resolve("lang", Some(&steve()), ask), None);
        }
        for ask in [&missing as Ask<'_>, &refused, &garbage] {
            assert!(!(ADAPTER.probe)(ask));
        }
        assert!((ADAPTER.probe)(&|_| Some(
            br#"{"ok":true,"languages":["en_us"]}"#.to_vec()
        )));
    }
}
