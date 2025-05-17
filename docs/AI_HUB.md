# AI organization with AI Hub Universal

AI mode replaces manual matching rules with model-proposed categories. File execution, approved folders, collision handling, timestamp/attribute preservation and undo still use the ordinary journal. The model receives no filesystem tools and cannot run commands.

## Set up

1. Start your existing AI Hub and a local text model. `universal` routes to the most recently started running local model; it is a route, not a downloaded model. FileOrganizer does not start or install models.
2. In **Settings → Folders**, use **Add source folder…** and **Add destination folder…** to pick existing folders. The first allowed destination initializes an empty organized-folder setting.
3. In **Settings → AI Hub**, expand **Hub connection & access token**, enter the base URL, normally `http://127.0.0.1:8080/v1`, and save your hub access token if authentication is enabled. **Test connection** calls `/v1/models` only. A reachable hub can still have no model ready.
4. Use **Choose…** or **Use a saved destination** for the **Organized folder** and describe your desired filing scheme. A newly chosen folder is also added as an exact allowed destination if necessary; no separate manual permission-list entry is needed. Pasted paths must still be inside an allowed root. For example: “Group by subject, then project. Keep invoices by business and personal use. Prefer a small number of stable folders.”
5. On **Review**, select **AI Hub**, then **Create AI plan**. After the file-readiness interval, the model proposes subfolders with explanations and confidence estimates. Select individual proposals or **Select recommended**, then organize the selection. Organizing automatically pauses active monitoring and rejects stale previews. Nothing is preselected in a new plan.
6. Optionally enable automatic organization under **Settings → Automation** after reviewing the setup. Only proposals at or above your confidence threshold are applied; lower estimates remain for review. Each launch starts paused.

Manual rules remain available as an alternative on Review. Existing installations keep their saved rule mode; an empty installation defaults to AI mode. The modes do not combine actions for the same file.

## Data sent to the hub

By default each request contains a filename, relative source subfolder, byte size, batch-local ID, your organization goal and known category names. It contains no absolute source paths or file content. **Include short text excerpts** explicitly opts into sending the first 1,024 bytes of TXT, MD, CSV, JSON, XML, YAML/YML and LOG files up to 1 MiB. Binary-looking text is skipped; malformed/truncated UTF-8 is replaced. These reads preserve access time on Windows. PDF/image/OCR analysis and archive extraction are not implemented. Files with ambiguous names may therefore receive poor categories or no category; review the results.

The default endpoint is local. If you configure a remote hub, the same data goes to that server. Use HTTPS; remote plain HTTP requires an explicit trusted-network setting. Connections are direct, without environment proxy forwarding. Redirects are refused. FileOrganizer makes no AI request at startup and never downloads a model. Watching explicitly enables further analysis of settled files.

Tokens are stored only in this application's generic Windows Credential Manager entry, scoped by a SHA-256 hash of the normalized hub address. Changing the address does not reuse the old address's saved token. Tokens are excluded from settings, SQLite and diagnostic messages. Only explicit AI actions read that entry. Advanced deployments may supply `FILEORGANIZER_AI_TOKEN`, which overrides saved credentials; removing a saved token does not remove that environment variable. No credentials from AI Hub's repository are discovered or imported.

## Validation, caching and limits

Requests use OpenAI-compatible `POST /v1/chat/completions`, `model: "universal"`, non-streaming JSON, temperature 0.1 and up to 2,048 output tokens. The active hub backend must support this interface. A 503 means no local model is ready. Access failures, rate limits, incomplete responses and invalid classification JSON pause monitoring; a failed AI job publishes no partial plan or cache.

Responses must contain batch IDs, a relative category of at most three levels, an optional permitted filename, integer confidence 0–100 and a short explanation. Unknown fields/IDs, duplicate IDs, tool calls, truncated completions, parent traversal, absolute paths, alternate streams, Windows reserved names, internal category names and bidirectional path controls are rejected. Renaming is opt-in and cannot change extensions. Missing/uncertain decisions leave files unclassified. These controls constrain file actions; they do not guarantee semantically correct model judgments or immunity to misleading text. Confidence is the model's uncalibrated estimate.

Each request handles at most 10 files; each plan handles at most 100. Requests are capped at 48 KiB, responses at 1 MiB. Successful classifications, including unclassified decisions, are cached locally by path, size, modification timestamp and AI settings. Unchanged files are not repeatedly sent during monitoring. A replacement that preserves size and modification time may reuse an old decision. **Reclassify** clears the AI cache and asks again for eligible source files; use it after changing the model behind Universal. It does not erase operation history or suppression of already processed/undone versions.

Analysis runs in a separate single-flight job. The UI and filesystem worker remain responsive. **Cancel AI** discards that job's result and stops monitoring; an already submitted HTTP call can take up to 45 seconds to finish, and no further batches are requested. Another request waits until it finishes. Shutdown cancels the job without waiting for the HTTP timeout. File publication/undo remain serialized and journaled.

## Verification status

Deterministic AI clients exercise requests, response rejection, batching/cache, cancellation, confidence gating and actual fixture move/copy/undo with metadata comparisons. No real model was queried or downloaded and no credential store was read/written during development. The managed environment refuses client sockets even to localhost with Windows error 10013. The bounded local HTTP-server test is retained as an explicitly ignored test for a network-enabled host:

```powershell
cargo test --locked --test ai http_transport -- --ignored --nocapture
```

Live Hub authentication, JSON support of a chosen model, category quality and model-switch behavior remain to be checked on disposable files. See [validation](VALIDATION.md).
