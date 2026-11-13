//! PVault, the economy plugin, over its `pvault.economy.v1` protocol.
//!
//! The contract is PVault's `proto/pvault/economy/v1/economy.proto` and
//! `docs/spec/economy-v1.md`. The messages are protobuf, and the two requests a
//! placeholder needs are small enough to write by hand, so the provider gains
//! no dependency for them:
//!
//! ```text
//! Request  { version = 1: Version { major = 1 }, get_currency = 16 | get_balance = 17 }
//! Response { version = 1, error = 2 | currency = 16 | balance = 17 }
//! ```
//!
//! Only the two read requests are ever sent. An adapter that could move money
//! would be a way to move money by writing a placeholder.

use pumpkin_plugin_api::uuid::Uuid;

use super::{Adapter, Ask, Viewer};

/// The spec major this adapter speaks. PVault serves every minor of the major a
/// request names and refuses any other major with an error body, which reads
/// here as no value.
const MAJOR: u8 = 1;

/// `Request.get_currency` and `Response.currency`.
const CURRENCY: u64 = 16;
/// `Request.get_balance` and `Response.balance`.
const BALANCE: u64 = 17;

/// PVault caps `fraction_digits` well below this. The clamp only keeps the
/// arithmetic in range if something else answers under its name.
const MAX_DIGITS: u32 = 18;

pub const ADAPTER: Adapter = Adapter {
    namespace: "pvault",
    plugin: "PVault",
    names: &[
        ("balance", "The viewer's balance"),
        (
            "balance_formatted",
            "The viewer's balance with the currency symbol",
        ),
        ("currency_name", "The currency, for one"),
        ("currency_plural", "The currency, for many"),
        ("currency_symbol", "The currency symbol"),
    ],
    // A balance moves when a player buys something, and a scoreboard that
    // lags a purchase by more than a moment reads as broken.
    ttl_ms: 2_000,
    probe: |ask| currency(ask).is_some(),
    resolve,
};

/// What `GetCurrency` says about the server's one currency.
struct Currency {
    name: String,
    plural: String,
    symbol: String,
    digits: u32,
}

/// A balance costs two round trips, the currency and the amount, because the
/// amount is in minor units and only the currency says how many digits that
/// is. The provider's cache is what keeps that cheap; if it ever shows up in a
/// profile, keep the currency for a minute here.
fn resolve(name: &str, viewer: Option<&Viewer>, ask: Ask<'_>) -> Option<String> {
    match name {
        "currency_name" => Some(currency(ask)?.name),
        "currency_plural" => Some(currency(ask)?.plural),
        "currency_symbol" => Some(currency(ask)?.symbol),
        "balance" | "balance_formatted" => {
            // Before any message, because a balance without a viewer is
            // nobody's and there is nothing to ask.
            let viewer = viewer?;
            let currency = currency(ask)?;
            let amount = amount(balance(viewer, ask)?, currency.digits);
            Some(if name == "balance" {
                amount
            } else {
                // The symbol goes first with no space, which is how PVault
                // prints a balance in its own messages.
                format!("{}{amount}", currency.symbol)
            })
        }
        _ => None,
    }
}

fn currency(ask: Ask<'_>) -> Option<Currency> {
    let reply = ask(&request(CURRENCY, &[]))?;
    let info = body(&reply, CURRENCY)?;
    Some(Currency {
        name: text(&info, 1),
        plural: text(&info, 2),
        symbol: text(&info, 3),
        digits: u32::try_from(number(&info, 4)).ok()?,
    })
}

/// The viewer's balance in minor units.
fn balance(viewer: &Viewer, ask: Ask<'_>) -> Option<i64> {
    let reply = ask(&balance_request(&viewer.uuid))?;
    let balance = body(&reply, BALANCE)?;
    // An `int64` travels as its two's complement in a varint.
    Some(number(&balance, 2) as i64)
}

/// `GetBalance { account: AccountId { player } }`, where `player` is the
/// sixteen bytes of the uuid, big endian.
fn balance_request(uuid: &Uuid) -> Vec<u8> {
    let mut id = [0u8; 16];
    id[..8].copy_from_slice(&uuid.high.to_be_bytes());
    id[8..].copy_from_slice(&uuid.low.to_be_bytes());
    let mut account = Vec::new();
    put(&mut account, 1, &id);
    let mut get_balance = Vec::new();
    put(&mut get_balance, 1, &account);
    request(BALANCE, &get_balance)
}

/// A `Request` carrying the version and one body, which goes under `number`.
fn request(number: u64, message: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    // `Version { major }`. A zero minor and patch are left out, as protobuf
    // leaves out every default.
    put(&mut out, 1, &[0x08, MAJOR]);
    put(&mut out, number, message);
    out
}

/// Minor units as PVault prints them: `1205` with two digits is `12.05`.
fn amount(minor: i64, digits: u32) -> String {
    let digits = digits.min(MAX_DIGITS);
    let sign = if minor < 0 { "-" } else { "" };
    let magnitude = minor.unsigned_abs();
    if digits == 0 {
        return format!("{sign}{magnitude}");
    }
    let scale = 10u64.pow(digits);
    format!(
        "{sign}{}.{:0width$}",
        magnitude / scale,
        magnitude % scale,
        width = digits as usize
    )
}

/// One field of a protobuf message.
enum Field<'a> {
    Varint(u64),
    Bytes(&'a [u8]),
}

/// Appends a length-delimited field.
fn put(out: &mut Vec<u8>, number: u64, payload: &[u8]) {
    put_varint(out, (number << 3) | 2);
    put_varint(out, payload.len() as u64);
    out.extend_from_slice(payload);
}

fn put_varint(out: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        out.push(value as u8 | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

fn take_varint(bytes: &mut &[u8]) -> Option<u64> {
    let mut value = 0;
    for shift in (0..64).step_by(7) {
        let (&byte, rest) = bytes.split_first()?;
        *bytes = rest;
        value |= u64::from(byte & 0x7f) << shift;
        if byte < 0x80 {
            return Some(value);
        }
    }
    None
}

/// Every field of one message, or [`None`] when the bytes are not protobuf.
fn fields(mut bytes: &[u8]) -> Option<Vec<(u64, Field<'_>)>> {
    let mut out = Vec::new();
    while !bytes.is_empty() {
        let tag = take_varint(&mut bytes)?;
        let field = match tag & 7 {
            0 => Field::Varint(take_varint(&mut bytes)?),
            2 => {
                let length = usize::try_from(take_varint(&mut bytes)?).ok()?;
                let payload = bytes.get(..length)?;
                bytes = &bytes[length..];
                Field::Bytes(payload)
            }
            // The two fixed widths. The protocol has none today, and stepping
            // over them keeps a later minor that adds one readable.
            1 => {
                bytes = bytes.get(8..)?;
                continue;
            }
            5 => {
                bytes = bytes.get(4..)?;
                continue;
            }
            _ => return None,
        };
        out.push((tag >> 3, field));
    }
    Some(out)
}

/// The field under `number`. When a number repeats, the last one counts.
fn field<'a, 'b>(message: &'b [(u64, Field<'a>)], number: u64) -> Option<&'b Field<'a>> {
    let (_, field) = message.iter().rev().find(|(found, _)| *found == number)?;
    Some(field)
}

/// The message under `number` of a `Response`. [`None`] for an error body, a
/// different body, or bytes that are not a response at all.
fn body(reply: &[u8], number: u64) -> Option<Vec<(u64, Field<'_>)>> {
    match field(&fields(reply)?, number)? {
        Field::Bytes(message) => fields(message),
        Field::Varint(_) => None,
    }
}

/// A varint field, or zero, which is what protobuf means by leaving it out.
fn number(message: &[(u64, Field<'_>)], number: u64) -> u64 {
    match field(message, number) {
        Some(Field::Varint(value)) => *value,
        _ => 0,
    }
}

/// A string field, or the empty string when it was left out.
fn text(message: &[(u64, Field<'_>)], number: u64) -> String {
    match field(message, number) {
        Some(Field::Bytes(bytes)) => String::from_utf8_lossy(bytes).into_owned(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STEVE: Viewer = Viewer {
        uuid: Uuid {
            high: 0x0011_2233_4455_6677,
            low: 0x8899_aabb_ccdd_eeff,
        },
    };

    /// `Response { version: 1.0.7, currency: coin / coins / $ / 2 digits }`,
    /// byte for byte what PVault 1.0.7 sends with its default config.
    const CURRENCY_REPLY: &[u8] = &[
        0x0a, 0x04, 0x08, 0x01, 0x18, 0x07, // version
        0x82, 0x01, 0x12, // currency, 18 bytes
        0x0a, 0x04, b'c', b'o', b'i', b'n', // name
        0x12, 0x05, b'c', b'o', b'i', b'n', b's', // plural
        0x1a, 0x01, b'$', // symbol
        0x20, 0x02, // fraction_digits
    ];

    /// `Response { version, balance: Balance { account, amount, previous } }`.
    fn balance_reply(amount: i64) -> Vec<u8> {
        let mut balance = vec![0x0a, 0x00];
        for number in [2, 3] {
            put_varint(&mut balance, number << 3);
            put_varint(&mut balance, amount as u64);
        }
        let mut reply = vec![0x0a, 0x02, 0x08, 0x01];
        put(&mut reply, BALANCE, &balance);
        reply
    }

    /// `Response { version, error: Error { code: VERSION_MISMATCH } }`.
    const ERROR_REPLY: &[u8] = &[0x0a, 0x02, 0x08, 0x02, 0x12, 0x02, 0x08, 0x02];

    /// A PVault that holds `amount` for everyone.
    fn vault(amount: i64) -> impl Fn(&[u8]) -> Option<Vec<u8>> {
        move |request| {
            let request = fields(request)?;
            match request.last()?.0 {
                CURRENCY => Some(CURRENCY_REPLY.to_vec()),
                BALANCE => Some(balance_reply(amount)),
                _ => None,
            }
        }
    }

    #[test]
    fn the_currency_request_is_the_bytes_the_proto_describes() {
        assert_eq!(
            request(CURRENCY, &[]),
            [0x0a, 0x02, 0x08, 0x01, 0x82, 0x01, 0x00]
        );
    }

    #[test]
    fn the_balance_request_names_the_player_by_big_endian_uuid() {
        assert_eq!(
            balance_request(&STEVE.uuid),
            [
                0x0a, 0x02, 0x08, 0x01, // version { major: 1 }
                0x8a, 0x01, 0x14, // get_balance, 20 bytes
                0x0a, 0x12, // account, 18 bytes
                0x0a, 0x10, // player, 16 bytes
                0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd,
                0xee, 0xff,
            ]
        );
    }

    #[test]
    fn every_name_resolves_against_a_vault() {
        let ask = vault(120_550);
        let resolve = |name| resolve(name, Some(&STEVE), &ask);
        assert_eq!(resolve("balance").as_deref(), Some("1205.50"));
        assert_eq!(resolve("balance_formatted").as_deref(), Some("$1205.50"));
        assert_eq!(resolve("currency_name").as_deref(), Some("coin"));
        assert_eq!(resolve("currency_plural").as_deref(), Some("coins"));
        assert_eq!(resolve("currency_symbol").as_deref(), Some("$"));
        assert_eq!(resolve("nonsense"), None);
        for (name, _) in ADAPTER.names {
            assert!(resolve(name).is_some(), "{name}");
        }
    }

    #[test]
    fn a_balance_needs_a_viewer_and_the_currency_does_not() {
        let silent = |_: &[u8]| -> Option<Vec<u8>> { panic!("nothing to ask without a viewer") };
        assert_eq!(resolve("balance", None, &silent), None);
        assert_eq!(resolve("balance_formatted", None, &silent), None);
        assert_eq!(
            resolve("currency_name", None, &vault(1)).as_deref(),
            Some("coin")
        );
    }

    #[test]
    fn an_empty_balance_body_is_zero_not_a_missing_value() {
        // Protobuf leaves a zero out, so a new player's balance arrives as a
        // body with no amount in it.
        let ask = |request: &[u8]| match fields(request)?.last()?.0 {
            CURRENCY => Some(CURRENCY_REPLY.to_vec()),
            _ => Some(vec![0x0a, 0x02, 0x08, 0x01, 0x8a, 0x01, 0x00]),
        };
        assert_eq!(
            resolve("balance", Some(&STEVE), &ask).as_deref(),
            Some("0.00")
        );
    }

    #[test]
    fn a_missing_plugin_an_error_and_garbage_all_resolve_nothing() {
        let missing = |_: &[u8]| None;
        let refused = |_: &[u8]| Some(ERROR_REPLY.to_vec());
        let garbage = |_: &[u8]| Some(b"{\"ok\":true}".to_vec());
        let truncated = |_: &[u8]| Some(CURRENCY_REPLY[..12].to_vec());
        for ask in [&missing as Ask<'_>, &refused, &garbage, &truncated] {
            assert!(!(ADAPTER.probe)(ask));
            assert_eq!(resolve("balance", Some(&STEVE), ask), None);
            assert_eq!(resolve("currency_name", None, ask), None);
        }
        assert!((ADAPTER.probe)(&vault(0)));
    }

    #[test]
    fn amounts_print_the_way_pvault_prints_them() {
        assert_eq!(amount(1205, 2), "12.05");
        assert_eq!(amount(5, 2), "0.05");
        assert_eq!(amount(0, 2), "0.00");
        assert_eq!(amount(-1250, 2), "-12.50");
        assert_eq!(amount(12, 0), "12");
        assert_eq!(amount(i64::MIN, 2), "-92233720368547758.08");
        assert_eq!(amount(i64::MAX, 40), "9.223372036854775807");
    }

    #[test]
    fn a_negative_amount_survives_the_varint() {
        let reply = balance_reply(-1250);
        let balance = body(&reply, BALANCE).unwrap();
        assert_eq!(number(&balance, 2) as i64, -1250);
    }

    #[test]
    fn a_varint_that_never_ends_is_not_read_forever() {
        assert_eq!(take_varint(&mut [0xffu8; 11].as_slice()), None);
        assert!(fields(&[0x08]).is_none());
        assert!(fields(&[0x0a, 0x05, 0x00]).is_none());
        assert!(fields(&[0x0b]).is_none());
    }
}
