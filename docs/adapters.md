# Adapters

For a server owner who wants `%pvault_balance%` in a tab list, and for the author of a plugin that
has an adapter here. If your plugin can register its own placeholders, that is still the way to do
it: see [expansions.md](expansions.md).

An adapter covers one case. A published plugin already answers other plugins over a documented IPC
protocol of its own, and it has not registered an expansion. Its values are one message away, and
no placeholder reaches them, because nobody claimed the namespace. For such a plugin the provider
asks in the plugin's own protocol and hands the answer back as a placeholder.

```
your plugin  ->  PumpkinPIE  ->  PVault
  "%pvault_balance%"     GetBalance, in PVault's protocol
            "12.05"
```

There is nothing to install or configure. Put the plugin next to PumpkinPIE, in any load order, and
its placeholders resolve. Without the plugin they stay in the text as written, like any placeholder
nobody can answer.

## What is adapted

| Placeholder | Example | Notes |
| :-- | :-- | :-- |
| `%pvault_balance%` | `1205.50` | The viewer's balance, with the digits the currency has |
| `%pvault_balance_formatted%` | `$1205.50` | The same with the currency symbol, as PVault prints it |
| `%pvault_currency_name%` | `coin` | |
| `%pvault_currency_plural%` | `coins` | |
| `%pvault_currency_symbol%` | `$` | |
| `%simplelang_lang%` | `pt_br` | The language SimpleLang shows the viewer |

The two balances and the language need a viewer, and are left in the text without one. The three
currency placeholders do not.

| Plugin | Its protocol | Checked against | A value is reused for |
| :-- | :-- | :-- | :-- |
| [PVault](https://github.com/vyPal/PVault) | `pvault.economy.v1`, protobuf | 1.0.7 | 2 s |
| [SimpleLang](https://github.com/Kawwabi/SimpleLang) | JSON with an `op` | 0.2.2 | 5 s |

`%simplelang_lang%` is the player's `/lang` choice. For a player who never chose, it is what
SimpleLang falls back to: the server's default language if one is set, otherwise the language of
the player's client.

## How an adapter behaves

- **A registered expansion always wins.** An adapter is only asked for a namespace nobody
  registered. See [below](#if-your-plugin-has-an-adapter-here).
- **The names are a fixed list.** An expansion is forwarded any `%ranks_*%`. An adapter answers the
  names in the table and nothing else, so `%pvault_nonsense%` is left in the text without PVault
  being asked.
- **No arguments.** `%pvault_balance:x%` is left in the text.
- **It only reads.** No adapter sends a request that changes anything in the other plugin. The
  PVault adapter sends `GetCurrency` and `GetBalance` and nothing else.
- **Values are cached per viewer** for the time in the table, the way an expansion's are with
  `Cache::Ttl`. A miss is cached too, so a plugin that is not installed costs one failed call per
  viewer and placeholder in that time, and no line in the log.
- **It spends the callback budget.** A value read through an adapter counts as one of the sixteen
  callbacks a line may make, like an expansion's.
- **It is listed while its plugin answers.** `/pie expansions` shows `adapter for PVault`, and
  `get_registered_placeholders` returns the names with the plugin's name as `source`. With the
  plugin gone, both drop it.

A balance costs two messages to PVault, one for the currency and one for the amount, because the
amount travels in minor units and only the currency says how many digits that is. The cache is what
keeps a tab list that shows a balance to every player from sending them every second.

## If your plugin has an adapter here

Register the namespace yourself whenever you are ready:

```rust
PieClient::new().register_expansion("pvault", &["balance"], Cache::Ttl { ms: 2_000 })?;
```

From that call on the provider forwards every `%pvault_*%` to you and skips the adapter, in
resolving and in `/pie`. Nothing has to be coordinated with this repository. Keep the names in the
table if existing configs should keep working, then open an issue here so the adapter gets deleted.

## What gets an adapter

An adapter is a stopgap for a plugin that is one step from having placeholders, not a second way to
provide them. It needs all of this:

- **A protocol the author published for other plugins**: a spec, a `.proto`, a README section. Not
  one read out of a binary.
- **Reads that need no secret and change nothing.**
- **A plugin that loads on the Pumpkin release PumpkinPIE is built for**, so the adapter can be
  checked on a live server before it ships.
- **No expansion registered by the plugin itself.**

A plugin with no API for other plugins cannot get one. Its data lives in its own data folder, which
no other plugin can read, so its author registers an expansion.

## Writing one

One file per plugin in `crates/pumpkin-pie-plugin/src/adapters/`, holding an `Adapter` and added to
`ADAPTERS` in `adapters.rs`:

```rust
pub const ADAPTER: Adapter = Adapter {
    namespace: "simplelang",
    plugin: "simplelang",
    names: &[("lang", "The language SimpleLang shows the viewer")],
    ttl_ms: 5_000,
    probe: |ask| reply(ask, br#"{"op":"languages"}"#).is_some(),
    resolve,
};
```

`plugin` is the other plugin's exact name, which is its IPC address. `probe` says whether it is
installed and speaks the protocol. `resolve` gets the name, the viewer and `ask`, a closure that
sends bytes to the plugin and returns its reply. Because the IPC call arrives as a closure, the
whole adapter is tested on the host with a closure in place of the other plugin. Test the request
bytes against the plugin's spec, and a missing plugin, an error reply and garbage, which must all
resolve to nothing.

Then add the rows to the tables on this page, and say in the pull request which version of the
plugin you ran it against.
