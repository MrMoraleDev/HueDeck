# SignalRGB-compatible local API

Tools and game integrations written for SignalRGB's local API work with HueDeck unchanged. HueDeck answers them on the same addresses:
- REST: `http://127.0.0.1:16038/api/v1/...`
- Canvas events: `POST http://127.0.0.1:16034/canvas/event?sender=<app>&event=<string>`

Both are loopback only. While SignalRGB itself holds the ports, HueDeck waits and does not fight it for them.

The route table is `huedeck_api::COMPAT_ROUTES` (`crates/huedeck-api/src/lib.rs`), and the router is `compat_router` in `crates/huedeck-api/src/server.rs`. The test `every_compat_route_answers_in_the_envelope` checks every route against the envelope below. For HueDeck's own, richer API, see [`docs/api/openapi.yaml`](../api/openapi.yaml).

## Envelope

Every answer is one JSON object. The one exception is a successful `effects/{id}/image`, which returns the picture itself.

```json
{ "api_version": "1.0", "id": 17, "method": "GET /api/v1/lighting", "params": {}, "status": "ok", "data": { } }
```

| Key | Content |
|---|---|
| `id` | A request counter. |
| `method` | `<HTTP method> <path>`. |
| `params` | The path parameters and the body fields the call used, such as `{"id": "evening"}`. |
| `status` | `ok` or `error`. |
| `data` | The answer, on success. |
| `errors` | On failure, instead of `data`: `[{"code", "title", "detail"}]`, sent with the matching HTTP status. |

SignalRGB's 404 texts are kept, for example "The requested effect was not found".

## Token

Reading never needs a token. Requests that change state (`POST`, `PATCH`) need `Authorization: Bearer <token>`, with the token from the `api-token` file in HueDeck's data directory.

The `config.toml` setting `[api] compat_open_mutations = true` drops this requirement for the compatible API. Even then, `lighting/loadEffect` always needs the token, because it runs a page that the caller wrote.

HueDeck has no Pro or Developer accounts, so it never answers 403 for them.

## Routes

| Route | HueDeck's answer |
|---|---|
| `GET app` | `name`, `version`, `api_version`, `compatible_with`, `platform`, `state`, and `links` to the other lists. |
| `GET new` | `{"items": [], "type": "new"}`. HueDeck has no news feed. |
| `GET partners/{id}` | `{"id", "permissions": {}, "type": "partner"}`. HueDeck grants no partner permissions. |
| `GET lighting` | `enabled`, `global_brightness`, `name` and `id` of the effect, `playlist` (`{id, name}` or `null`), `links`. |
| `PATCH lighting/global_brightness {global_brightness}` | Sets the brightness, 0-100. Answers like `GET lighting`. |
| `PATCH lighting/enabled {enabled}` | Pauses or resumes the canvas. While paused, every device shows black. Answers like `GET lighting`. |
| `GET lighting/effects` | `items` of `{id, name, links.apply, links.self}`. |
| `GET lighting/effects/{id}` | `name`, `description`, `publisher`, `image` (a link), `developer_effect`, `favorite` (always `false`), `uses_audio`, `uses_input`, `uses_video`, `uses_meters`, `uses_webgl`, `parameters` (`{label, type, value}` by property), `presets`, `links`. |
| `GET lighting/effects/{id}/image` | The effect's picture, if one sits next to the page: `<name>.png`, `.jpg`, `.jpeg` or `.webp`, and for catalog-cache effects `image.*` or `thumbnail.*`. Otherwise a 404 envelope. |
| `POST lighting/effects/{id}/apply` | Shows the effect. A playing playlist stops. |
| `GET lighting/effects/{id}/presets` | `items`: the preset names. |
| `PATCH lighting/effects/{id}/presets {preset}` | Loads the preset and shows the effect. |
| `GET` or `POST lighting/next`, `lighting/previous` | The next or previous effect of the library. While a playlist plays, the next or previous entry of the playlist. 409 "No Next Effect" when there is none. |
| `GET` or `POST lighting/shuffle` | A random effect. Game integrations (effects with screen meters) and developer effects are left out. A playing playlist stops. |
| `POST lighting/loadEffect {uid, html}` | Saves the page as the effect `developer:<uid>` and shows it. The file goes in `developer-effects/` in the data directory, and the effect is still there after a restart. The `uid` may hold letters, digits, spaces, `-`, `_` and `.`, up to 64 characters. Pages may be at most 4 MiB. |
| `GET lighting/playlists` | `items` of `{id, name, effects_count, last_update_time, image_url: null, shuffle, type, links.self, links.play}`. |
| `GET lighting/playlists/{id}` | The same fields as one item of the list, plus `playing` and `effects` (`[{id, name, preset, duration}]`). |
| `POST lighting/playlists/{id}/play {starting_effect_id?}` | Plays the playlist, from its start or from the entry that shows `starting_effect_id`. |
| `GET scenes/current_layout` | `{"current_layout": {"id", "type": "layout"}}`. |
| `PATCH scenes/current_layout {layout}` | Switches to that layout. |
| `GET scenes/layouts` | `items` of `{id, type: "layout"}`. |

Any other path gets a 404 envelope.

## Playlists and the auto-next timer

Playlists are made in the dashboard (Effects → Playlists) or through the native API (`/v1/playlists`).

**Entries.** A playlist is an ordered list of entries. Each entry names:
- an effect;
- optionally, a preset and control values, which load when the entry starts;
- optionally, a duration in seconds.

When an entry's preset and values load, they are stored on the effect, the same as when a preset is applied.

**Order.** A shuffled playlist plays its entries in a new random order every round. The first entry of a new round is never the one that was just shown.

**Missing effects.** Entries whose effect is not installed are skipped. This happens, for example, when SignalRGB's effects are turned off.

**Duration.** An entry with no duration lasts the timer's interval.

**The auto-next timer** is SignalRGB's "Effect Timer Interval":
- It is off by default.
- Its interval is 300 s by default.
- While no playlist plays and the timer is on, it shows the next effect of the library every interval. It skips game integrations and developer effects.

Set the timer with `PATCH /v1/lighting/timer {"enabled", "interval"}`, or in the dashboard.

**What ends a playlist.** Choosing an effect or a preset, or shuffling, ends it. Next and previous move within it. While the canvas is paused, nothing switches.

**After a restart.** The playing playlist and its entry are kept in the database, and playback resumes with that entry.
