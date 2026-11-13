# Changelog

## Unreleased

Protocol version `0`, unchanged. Built against the public Pumpkin 0.2.1+26.3-26.51 API.

### Added

- Adapters: the provider asks PVault and SimpleLang in their own IPC protocols, so
  `%pvault_balance%`, `%pvault_balance_formatted%`, `%pvault_currency_name%`,
  `%pvault_currency_plural%`, `%pvault_currency_symbol%` and `%simplelang_lang%` resolve with no
  change to either plugin. A plugin that registers its namespace takes over from its adapter. See
  [docs/adapters.md](docs/adapters.md).

Verified on **Pumpkin 0.2.2+26.3-26.60 and 0.2.1+26.3-26.51, Minecraft Java 26.3 (protocol 777)**
with PVault 1.0.7 and SimpleLang 0.2.2 as published on the Pumpkin Market. A consumer plugin asked
over IPC, with two clients online as viewers, one set to `en_us` and one to `de_de`. Both plugins
loaded before the provider. Every row held on both servers.

| Check | Result |
| :-- | :-- |
| `/pie expansions` | lists `adapter for PVault` and `adapter for simplelang` with their names |
| The currency without a viewer | `coin/coins/$`, and `%pvault_balance%` stayed in the text |
| A new player | `%pvault_balance% %pvault_balance_formatted%` gave `0.00 $0.00` |
| `/eco give` of `1205.5`, asked again at once | still `0.00`, from the cache |
| The same after 2 s | `1205.50 $1205.50`, and `%PVault_Balance%` resolved too |
| The second viewer | his own `0.00` |
| `get_placeholder_value` | `known: true`, `1205.50` |
| `%pvault_balance:x%` and `%pvault_nonsense%` | both left in the text and reported unresolved |
| `get_registered_placeholders` | the six names, with `PVault` and `simplelang` as `source` |
| `%simplelang_lang%` for both viewers | `en_us` and `de_de`, the languages of their clients |
| The same after `/lang pt_br` and 5 s | `pt_br` |
| Another plugin registers `pvault` | its `%pvault_x%` resolved, `%pvault_balance%` no longer did, `/pie expansions` showed the expansion and no PVault adapter |
| That plugin unregisters | `%pvault_balance%` gave `1205.50` again |
| Neither plugin installed | `%pvault_balance% %simplelang_lang%` stayed as written, `/pie expansions` and `get_registered_placeholders` listed no adapter |

One line with an uncached `%pvault_balance%` took 80 to 100 µs through the consumer, and 40 to 65 µs
from the cache. Every run ended on `stop` with no error in the log and no warning from the
provider. The last row ran on 0.2.2 only.

Still unverified: any other version of either plugin.

## 0.2.5

Protocol version `0`. Workspace version `0.2.5`. Built against the public
Pumpkin 0.2.1+26.3-26.51 API; the changes below have not been verified on a live server.

### Fixed

- Cache expansion values separately by optional argument and bound cache memory.
- Limit placeholder output and encoded responses, and refuse oversized batches whole.
- Normalize callback namespaces for mixed-case placeholder IDs.

- `set_placeholders_batch` no longer sends a line over 32 KiB. It checked nothing,
  so the line was sent and then dropped by the provider's filter, which returned a
  `results` array shorter than the request. Both arrays are indexed positionally,
  so every line after the dropped one received its neighbour's answer. The batch is
  now refused whole, and the caller keeps its own text.

Earlier verification on **Pumpkin 0.2.0+26.3-26.51, Minecraft Java 26.3 (protocol 777)** drove the server
over its console so no client is needed. Three components load together: the provider, an expansion,
and a consumer.

The expansion path, via `crates/pumpkin-pie-testexp`:

| Check | Result |
| :-- | :-- |
| `register_expansion` on two namespaces | accepted, and the applied TTL comes back as asked |
| `on_request` round trip | the expansion's answer reaches `/pie parse` |
| `Cache::Never` | `%testexp_count%` re-asked the expansion on every resolve, incrementing each time |
| `Cache::Ttl` | asked 3 times, called back **once** |
| Declining with `None` | `%testexp_nope%` stayed literal and was reported unresolved |
| Case insensitivity | `%testexp_COUNT%` resolved, reaching the expansion as `name=count` |

The consumer path, via `crates/pumpkin-pie-consumer` and its `/piecheck`, which is the direction a
tab list or scoreboard plugin actually uses:

| Check | Result |
| :-- | :-- |
| `set_placeholders` | resolved a built in and an expansion, left `%nope_nope%` literal, reported it unresolved |
| `set_placeholders` with no viewer | `%server_online%` resolved, player placeholders stayed in the text as written |
| `set_placeholders_batch` | 3 lines in one message, all resolved, in order |
| `get_placeholder_value` | a built in and an expansion returned values; an unknown id returned `None`, not an error |
| `get_registered_placeholders` | 15 entries across `player`, `server`, `testexp`, `testexpcached` |
| `unregister_expansion` from a plugin that registered nothing | returned empty and left the other plugin's namespaces intact |

The cache survives the IPC boundary, which is the part that only a real consumer can show: three
identical consumer runs asked for `%testexpcached_value%` 3 times and got **1** callback, while
`Cache::Never` placeholders got one callback per request.

Last run was clean: no errors, no warnings from any of the three plugins, no panics.

Still unverified: no real Minecraft client this round, so the player built-ins were exercised with a
name that is not online. No other Pumpkin version has been tried.
