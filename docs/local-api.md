# Local Ingest API

Other apps on the same Mac can push text into Better Clipboard over a small HTTP API. Better Clipboard puts the text on the system clipboard, records it in history with the sending app's name, and can post a macOS notification. Browsers can't write to the clipboard unless their page has focus, so this API gives local tools a way to do it from the background.

The first use case is one-time verification codes. A tool that receives a code sends it to Better Clipboard. The code goes onto the clipboard straight away and a notification confirms it, whichever app you're in.

## Turn it on

The API is off by default. Open Settings (`⚙` in the palette, or the 📋 menu) and tick **Let local apps send items**. Settings then shows the token, with these buttons:

- **Copy** copies the token without saving it to clipboard history.
- **Regenerate** issues a new token. Clients using the old token are rejected immediately.

The token is stored in `~/Library/Application Support/com.mgosal.Better-Clipboard/api-token`, and only your user account can read it (mode `0600`). Local tools can read it from that file instead of having it pasted in.

The port defaults to `47823`. To change it, set `api_port` in `settings.json` in the same directory, then restart Better Clipboard.

## Requests

Every request needs the header `Authorization: Bearer <token>`.

### `POST /v1/clip`

```sh
curl -s http://127.0.0.1:47823/v1/clip \
  -H "Authorization: Bearer $(cat ~/Library/Application\ Support/com.mgosal.Better-Clipboard/api-token)" \
  -H "Content-Type: application/json" \
  -d '{"text":"482913","source":"Mail","label":"Example verification code","kind":"one-time-code","notify":true}'
```

| Field | Required | Meaning |
| --- | --- | --- |
| `text` | yes | The text to place on the clipboard. Maximum 64 KB. |
| `source` | yes | Name of the sending app, shown in the history row (up to 80 characters). |
| `label` | no | Short description shown with the item (up to 200 characters). |
| `kind` | no | `one-time-code`, or leave it out. Unknown kinds are treated as plain text. |
| `notify` | no | `true` posts a macOS notification. Defaults to `false`. |

On success, the API returns `201 {"id": "<history item id>"}`.

### `GET /v1/health`

Returns `200 {"ok": true, "version": "x.y.z"}`. A `401` also tells you Better Clipboard is running, but that your token is wrong. A refused connection means it isn't running or the API is turned off.

### Errors

Errors return JSON in the form `{"error": "..."}`.

| Status | Cause |
| --- | --- |
| `400` | Invalid JSON, or `text` or `source` is missing. |
| `401` | Missing or wrong bearer token. |
| `403` | The request came from a browser (it has an `Origin` header), from a non-loopback address, or with an unexpected `Host` header. |
| `404` / `405` | Unknown path, or the wrong method for that path. |
| `413` | `text` is larger than 64 KB, or the body is larger than 128 KB. |
| `429` | Too many requests. The limit is a burst of 5, refilling at 5 per second. |
| `500` | Better Clipboard couldn't write to the clipboard or save history. |

## Security

- The server listens on `127.0.0.1` only. It is never reachable from the network.
- Every request needs the bearer token. Tokens are compared in constant time.
- The server rejects any request that carries an `Origin` header and never sends CORS headers, so a web page can't POST to the API to overwrite your clipboard. Requests whose `Host` isn't `127.0.0.1:<port>` or `localhost:<port>` are also rejected, which blocks DNS-rebinding attacks.
- Request size and rate are limited as listed in the errors table.

## One-time codes

Items sent with `kind: "one-time-code"` show the code itself in the row tile, and the row is labelled `Code`. They aren't masked, because you need to read them. A code is removed from history as soon as it has been used. That happens when:

- you paste it through Better Clipboard (`Enter`, double-click, or the Paste button),
- anything else is copied, which supersedes the code, or
- 10 minutes pass.

The system clipboard itself isn't touched; only the history entry is removed. Better Clipboard can't detect a `⌘V` paste made directly in another app, so in that case the code stays until it is superseded or expires.

## Notifications

With `notify: true`, Better Clipboard posts a notification under its own name, for example:

> **482913 · copied to clipboard**
> Example verification code (from Mail)

Click the notification to open the palette with that item selected. macOS asks for notification permission the first time the API is turned on. Notifications only work from the app bundle (`Better Clipboard.app`), not from `cargo run`.
