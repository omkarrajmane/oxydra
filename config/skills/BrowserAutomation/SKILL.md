---
name: browser-automation
description: Control a headless Chrome browser via Pinchtab's REST API
activation: auto
requires:
  - shell_exec
env:
  - PINCHTAB_URL
priority: 50
---

## Browser Automation (Pinchtab)

You can control a headless Chrome browser via Pinchtab at `{{PINCHTAB_URL}}`.
All curl commands require the auth header: `-H "Authorization: Bearer $BRIDGE_TOKEN"`
Chrome starts lazily on the first request.

### Core Loop

1. **Navigate** → opens URL in a new tab, returns `tabId`
2. **Snapshot** → accessibility tree with clickable refs (e.g., `e5`)
3. **Act** → click/type/fill/press using refs
4. **Snapshot again** → use `diff=true` to see only changes (~90% fewer tokens)
5. Repeat 3-4 until done

```bash
# Navigate (creates new tab, returns tabId)
TAB=$(curl -s -X POST {{PINCHTAB_URL}}/navigate \
  -H "Authorization: Bearer $BRIDGE_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"url":"https://example.com","newTab":true}' | jq -r '.tabId')

# Snapshot (interactive elements with refs)
curl -s "{{PINCHTAB_URL}}/snapshot?tabId=$TAB&filter=interactive&maxTokens=2000" \
  -H "Authorization: Bearer $BRIDGE_TOKEN"

# Click a ref
curl -s -X POST "{{PINCHTAB_URL}}/action" \
  -H "Authorization: Bearer $BRIDGE_TOKEN" \
  -H 'Content-Type: application/json' \
  -d "{\"kind\":\"click\",\"ref\":\"e5\",\"tabId\":\"$TAB\"}"

# Diff snapshot (only changes)
curl -s "{{PINCHTAB_URL}}/snapshot?tabId=$TAB&filter=interactive&diff=true" \
  -H "Authorization: Bearer $BRIDGE_TOKEN"
```

### Key Endpoints

All endpoints use flat paths. Multi-tab targeting uses `?tabId=ID` query parameter or `"tabId"` in POST body.

| Endpoint | Method | Purpose |
|---|---|---|
| `/navigate` | POST | Navigate URL → `{tabId, url, title}`. **Must include `"newTab":true`** to get `tabId` back |
| `/tabs` | GET | List all tabs → `{tabs: [{id, title, url, type}]}` |
| `/tab` | POST | `{"action":"new","url":"..."}` or `{"action":"close","tabId":"..."}` |
| `/snapshot` | GET | Accessibility tree. Params: `tabId`, `filter=interactive`, `diff=true`, `maxTokens=2000`, `format=compact` |
| `/text` | GET | Readable text. Params: `tabId`, `mode=raw`, `maxChars=N` |
| `/action` | POST | `{"kind":"click\|type\|fill\|press\|hover\|scroll\|select\|focus", "ref":"e5", "tabId":"..."}` |
| `/actions` | POST | Batch: `{"actions":[...], "stopOnError":true, "tabId":"..."}` |
| `/screenshot` | GET | Binary PNG. Params: `tabId`, `raw=true` → save with `curl -o /shared/file.png` |
| `/pdf` | GET | `?tabId=...&output=file&path=/shared/.pinchtab/page.pdf` |
| `/evaluate` | POST | Run JS: `{"expression":"document.title", "tabId":"..."}` |
| `/cookies` | GET/POST | Get/set cookies. Param: `tabId` |
| `/download` | GET | Download file: `?url=...&output=file&path=/shared/.pinchtab/file.ext` |
| `/upload` | POST | Upload: `{"selector":"input[type=file]","paths":["/shared/file.jpg"],"tabId":"..."}` |
| `/health` | GET | Health check → `{status, tabs}` |

### Best Practices

- Always use `maxTokens=2000` on snapshots (full trees can exceed 10K tokens)
- Use `filter=interactive` to see only clickable/input elements
- Use `diff=true` after actions for ~90% token savings
- Use `/text` for reading content (~800 tokens/page)
- Use `format=compact` for most token-efficient snapshots
- Batch interactions with `POST /actions` for fewer round-trips
- **Always include `"newTab":true`** when navigating to get a `tabId`
- Save Pinchtab-generated files to `/shared/.pinchtab/` (server restriction), then copy to `/shared/` if needed
- Wait 2-3 seconds after navigation before snapshot for complex pages: `sleep 3`
- Store `$TAB` and reuse it — tab IDs are stable

### File Integration

- Save screenshots: `curl "{{PINCHTAB_URL}}/screenshot?tabId=$TAB&raw=true" -H "Authorization: Bearer $BRIDGE_TOKEN" -o /shared/screenshot.png`
- Save PDFs: `curl "{{PINCHTAB_URL}}/pdf?tabId=$TAB&output=file&path=/shared/.pinchtab/page.pdf" -H "Authorization: Bearer $BRIDGE_TOKEN" && cp /shared/.pinchtab/page.pdf /shared/page.pdf`
- Download files: `curl "{{PINCHTAB_URL}}/download?url=...&output=file&path=/shared/.pinchtab/report.csv" -H "Authorization: Bearer $BRIDGE_TOKEN" && cp /shared/.pinchtab/report.csv /shared/report.csv`
- Upload files: First write to /shared/, then `curl -X POST "{{PINCHTAB_URL}}/upload" -H "Authorization: Bearer $BRIDGE_TOKEN" -H 'Content-Type: application/json' -d '{"selector":"input[type=file]","paths":["/shared/file.jpg"]}'`
- Send to user: After saving to /shared/, use `send_media` to deliver the file

### If Blocked

If you encounter CAPTCHAs, 2FA, or login walls, call `request_human_assistance`
with a clear description of the blocker.

For the full API reference (all params, PDF options, upload/download, stealth):
`cat /shared/.oxydra/skills/BrowserAutomation/references/pinchtab-api.md`
