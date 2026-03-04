# Pinchtab API Reference

Base URL for all examples: `http://localhost:9867`

> **Auth:** All requests require `-H "Authorization: Bearer $BRIDGE_TOKEN"`.
>
> **Multi-tab:** All endpoints use flat paths. Target a specific tab with `?tabId=ID` query parameter or `"tabId":"ID"` in POST body. Without `tabId`, the active (most recently used) tab is targeted.

## Navigate

```bash
# Navigate in a new tab (returns tabId)
curl -X POST /navigate \
  -H "Authorization: Bearer $BRIDGE_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"url": "https://example.com", "newTab": true}'
# Response: {"tabId":"ABC123","title":"Example","url":"https://example.com/"}

# Navigate in an existing tab (reuses tab, no tabId in response)
curl -X POST /navigate \
  -H "Authorization: Bearer $BRIDGE_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"url": "https://example.com", "tabId": "ABC123"}'
# Response: {"title":"Example","url":"https://example.com/"}

# With options: custom timeout, block images
curl -X POST /navigate \
  -H "Authorization: Bearer $BRIDGE_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"url": "https://example.com", "newTab": true, "timeout": 60, "blockImages": true}'
```

**IMPORTANT:** You must include `"newTab": true` to get a `tabId` back. Without it, the active tab is reused and no `tabId` is returned.

## Snapshot (accessibility tree)

```bash
# Full tree
curl "/snapshot" -H "Authorization: Bearer $BRIDGE_TOKEN"

# Interactive elements only (buttons, links, inputs) — much smaller
curl "/snapshot?filter=interactive" -H "Authorization: Bearer $BRIDGE_TOKEN"

# Smart diff — only changes since last snapshot (massive token savings)
curl "/snapshot?diff=true" -H "Authorization: Bearer $BRIDGE_TOKEN"

# Compact format — most token-efficient (recommended)
curl "/snapshot?format=compact" -H "Authorization: Bearer $BRIDGE_TOKEN"

# Scope to CSS selector (e.g. main content only)
curl "/snapshot?selector=main" -H "Authorization: Bearer $BRIDGE_TOKEN"

# Truncate to ~N tokens
curl "/snapshot?maxTokens=2000" -H "Authorization: Bearer $BRIDGE_TOKEN"

# Combine for maximum efficiency
curl "/snapshot?format=compact&selector=main&maxTokens=2000&filter=interactive" \
  -H "Authorization: Bearer $BRIDGE_TOKEN"

# Target specific tab
curl "/snapshot?tabId=ABC123&format=compact&filter=interactive&maxTokens=2000" \
  -H "Authorization: Bearer $BRIDGE_TOKEN"

# Disable animations before capture
curl "/snapshot?noAnimations=true" -H "Authorization: Bearer $BRIDGE_TOKEN"
```

Returns flat JSON array of nodes with `ref`, `role`, `name`, `depth`, `value`, `nodeId`.

**Token optimization**: Use `?format=compact` for best token efficiency. Add `?filter=interactive` for action-oriented tasks (~75% fewer nodes). Use `?selector=main` to scope to relevant content. Use `?maxTokens=2000` to cap output. Use `?diff=true` on multi-step workflows to see only changes.

## Act on elements

```bash
# Click by ref
curl -X POST /action -H "Authorization: Bearer $BRIDGE_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"kind": "click", "ref": "e5"}'

# Click in specific tab
curl -X POST /action -H "Authorization: Bearer $BRIDGE_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"kind": "click", "ref": "e5", "tabId": "ABC123"}'

# Type into focused element
curl -X POST /action -H "Authorization: Bearer $BRIDGE_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"kind": "type", "ref": "e12", "text": "hello world"}'

# Press a key
curl -X POST /action -H "Authorization: Bearer $BRIDGE_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"kind": "press", "key": "Enter"}'

# Fill (set value directly, no keystrokes)
curl -X POST /action -H "Authorization: Bearer $BRIDGE_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"kind": "fill", "selector": "#email", "text": "user@example.com"}'

# Hover (trigger dropdowns/tooltips)
curl -X POST /action -H "Authorization: Bearer $BRIDGE_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"kind": "hover", "ref": "e8"}'

# Select dropdown option
curl -X POST /action -H "Authorization: Bearer $BRIDGE_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"kind": "select", "ref": "e10", "value": "option2"}'

# Scroll by pixels (infinite scroll pages)
curl -X POST /action -H "Authorization: Bearer $BRIDGE_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"kind": "scroll", "scrollY": 800}'

# Click and wait for navigation
curl -X POST /action -H "Authorization: Bearer $BRIDGE_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"kind": "click", "ref": "e5", "waitNav": true}'
```

## Batch actions

```bash
curl -X POST /actions -H "Authorization: Bearer $BRIDGE_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"actions":[{"kind":"click","ref":"e3"},{"kind":"type","ref":"e3","text":"hello"},{"kind":"press","key":"Enter"}],"stopOnError":true}'
```

## Extract text

```bash
# Readability mode (strips nav/footer/ads)
curl "/text" -H "Authorization: Bearer $BRIDGE_TOKEN"

# Raw innerText
curl "/text?mode=raw" -H "Authorization: Bearer $BRIDGE_TOKEN"

# Specific tab
curl "/text?tabId=ABC123" -H "Authorization: Bearer $BRIDGE_TOKEN"
```

Returns `{url, title, text}`. Cheapest option (~1K tokens for most pages).

## PDF export

```bash
# Save to disk (must be under /shared/.pinchtab/)
curl "/pdf?output=file&path=/shared/.pinchtab/page.pdf" \
  -H "Authorization: Bearer $BRIDGE_TOKEN"
# Then copy to /shared/ if needed: cp /shared/.pinchtab/page.pdf /shared/page.pdf

# Specific tab
curl "/pdf?tabId=ABC123&output=file&path=/shared/.pinchtab/page.pdf" \
  -H "Authorization: Bearer $BRIDGE_TOKEN"

# Raw PDF bytes
curl "/pdf?raw=true" -H "Authorization: Bearer $BRIDGE_TOKEN" -o page.pdf

# Landscape with custom scale
curl "/pdf?landscape=true&scale=0.8&raw=true" \
  -H "Authorization: Bearer $BRIDGE_TOKEN" -o page.pdf
```

**Query Parameters:** `tabId`, `paperWidth`, `paperHeight`, `landscape`, `marginTop/Bottom/Left/Right`, `scale` (0.1–2.0), `pageRanges`, `displayHeaderFooter`, `headerTemplate`, `footerTemplate`, `preferCSSPageSize`, `generateTaggedPDF`, `generateDocumentOutline`, `output` (file/JSON), `path`, `raw`.

**Note:** When using `output=file`, the `path` must be under `/shared/.pinchtab/`. Copy the file to `/shared/` afterwards.

## Download files

```bash
# Save directly to disk (must be under /shared/.pinchtab/)
curl "/download?url=https://site.com/export.csv&output=file&path=/shared/.pinchtab/export.csv" \
  -H "Authorization: Bearer $BRIDGE_TOKEN"
# Then copy: cp /shared/.pinchtab/export.csv /shared/export.csv

# Raw bytes
curl "/download?url=https://site.com/image.jpg&raw=true" \
  -H "Authorization: Bearer $BRIDGE_TOKEN" -o /shared/image.jpg
```

## Upload files

```bash
curl -X POST "/upload" -H "Authorization: Bearer $BRIDGE_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"selector": "input[type=file]", "paths": ["/shared/photo.jpg"], "tabId": "ABC123"}'
```

## Screenshot

```bash
# Save directly (raw bytes)
curl "/screenshot?raw=true" -H "Authorization: Bearer $BRIDGE_TOKEN" -o /shared/screenshot.png

# Specific tab
curl "/screenshot?tabId=ABC123&raw=true" -H "Authorization: Bearer $BRIDGE_TOKEN" -o /shared/screenshot.png

# Lower quality JPEG
curl "/screenshot?raw=true&quality=50" -H "Authorization: Bearer $BRIDGE_TOKEN" -o /shared/screenshot.jpg
```

## Evaluate JavaScript

```bash
curl -X POST /evaluate -H "Authorization: Bearer $BRIDGE_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"expression": "document.title"}'

# Specific tab
curl -X POST /evaluate -H "Authorization: Bearer $BRIDGE_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"expression": "document.title", "tabId": "ABC123"}'
```

## Tab management

```bash
# List tabs
curl /tabs -H "Authorization: Bearer $BRIDGE_TOKEN"
# Response: {"tabs":[{"id":"ABC","title":"...","url":"...","type":"page"}]}

# Open new tab
curl -X POST /tab -H "Authorization: Bearer $BRIDGE_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"action": "new", "url": "https://example.com"}'
# Response: {"tabId":"NEW_ID",...}

# Close tab
curl -X POST /tab -H "Authorization: Bearer $BRIDGE_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"action": "close", "tabId": "TARGET_ID"}'
```

## Cookies

```bash
# Get cookies for current page
curl "/cookies" -H "Authorization: Bearer $BRIDGE_TOKEN"

# Get cookies for specific tab
curl "/cookies?tabId=ABC123" -H "Authorization: Bearer $BRIDGE_TOKEN"

# Set cookies
curl -X POST /cookies -H "Authorization: Bearer $BRIDGE_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"url":"https://example.com","cookies":[{"name":"session","value":"abc123"}]}'
```

## Stealth

```bash
# Check stealth status
curl /stealth/status -H "Authorization: Bearer $BRIDGE_TOKEN"

# Rotate fingerprint
curl -X POST /fingerprint/rotate -H "Authorization: Bearer $BRIDGE_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"os":"windows"}'
```

## Health check

```bash
curl /health -H "Authorization: Bearer $BRIDGE_TOKEN"
# Response: {"status":"ok","tabs":1}
```
