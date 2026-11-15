# Operations

Use `xiaoxiong --help` for the installed CLI's exact argument syntax and `xiaoxiong capabilities` for the connected app's live method/scope catalog. Use commands.ts for the versioned command shapes and parameters.md for the verified route parameters. The routes below identify command families, not permission to invent flags or JSON fields.

- Connection: `auth login`, `auth status`, `auth logout`.
- Drafts/projects: `draft list`, `draft get`, `draft rows`; validated modifications through `draft apply`.
- Work scheduling: `task actions` discovers supported operations; `task intent` submits an authorized action; `task list`, `task get`, `task wait` track work.
- Batch assistant (小助手): read [batch-assistant.md](batch-assistant.md) for episode/stage selection, local `batch prepare`, estimates, start and control. Use `batch get/list/progress` to inspect the app's run; do not build a second scheduler or replace an existing run to resume it.
- Recovery: `task recheck` reconciles a provider outcome; `task retry` is a new deliberate retry only after checking status and approval; `task cancel` requests cancellation and does not promise a refund or immediate stop.
- Accounts and quotas: `account status`; never expose stored credentials or assume eligibility from agent authorization.
- Channel configuration: `channels list/get/update` for safe public projections and approved patches. Secret writes use advertised write-only create/update with stdin or a JSON file, never argv, stdout, logs or response data. Changed authentication destinations require reauthorization/new keys.
- Exports: use advertised `export ...` commands. Check the exact export scope, destination and overwrite behavior before changing user files.
- Additional app capabilities: `call METHOD --file JSON` forwards a supported method with a JSON parameter object. It is an escape hatch for the method catalog, not a raw HTTP/provider/database tunnel.

For script, episode, asset, prompt, storyboard, image, audio, video, model selection, batch generation, media import or project management requests, start with live capabilities and task actions. If no validated method supports the request, report the missing capability rather than claiming success or falling back to direct storage writes.

## Safe write lifecycle

1. Resolve the draft/entity ID from a read operation and inspect current values.
2. Construct commands using the current app schema. Preserve commandId/requestId; validate references and partial update semantics. Summarize intended changes before consequential actions.
3. Submit once through `draft apply`, `task intent` or the advertised method. Let the app perform its existing permission, account and task checks. The calling agent must obtain the required user approval for consequential or paid work; a scope grant is not an unlimited spending budget. A missing cost estimate is unknown, not free.
4. Read the resulting state or task record. Keep task IDs and return useful results, not credentials.
5. On ambiguous transport failure, query state/task status using existing identifiers. Never resend automatically. A cancellation or error is not proof that the provider charged nothing.
