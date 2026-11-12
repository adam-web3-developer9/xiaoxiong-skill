# Agent capability routes

Baseline: AIStudio `5423be21112b61b0728cf10a0bc9883e0e308581`.

`v2_routes::invoke` is a typed adapter over the one application's `Host.invoke`. It never opens SQLite, dispatches arbitrary IPC, creates another task queue or bypasses DesktopService's build/account/update checks. `capabilities()` lists implemented method names and required scopes. The application remains responsible for policy, availability, deduplication, versions and actual execution.

## Operations

- Draft: list/search/get/rows/apply/create/copy. Search uses authorized draft summaries (title/name/description/textPreview), not full-text indexing. Apply retains the original explicit-version/user-origin/no-skip command rules. Create/copy require an all-drafts grant because the new ID is not part of a restricted existing-draft grant.
- Task: 48 production action names from `generic_owner::PreparationRegistry::builtin`; list/get/estimate/intent/cancel/retry/recheck. Only the existing intent entrypoint queues work. Retry requires generate, and retry of actual `type: export` additionally requires export. Jianying/comic Jianying intents require generate plus export.
- Batch: list/get/progress/estimate/start/pause/stop/resume/retryStage. Controls resolve the actual application's run using `batch:get(draftId)` and compare `runId`, never trust a caller's draft hint.
- Workflow: list/get/estimate/start/cancel/resume. Global video-case and derived-draft workflows require an all-drafts grant. Referenced task IDs are resolved before create/resume. Task controls likewise resolve real live/history ownership.
- Styles: list/setReferences. Reference writes use an atomic narrow owner channel, not a masked-settings read/replace cycle.
- Channels: list/get/create/update/models/options. Read and writes use typed owner channels. Each public provider includes configurationVersion; writes can supply optional expectedVersion for CAS. Ordinary patches cannot supply secret/reference fields; secrets use a separate credentials input and `channels.credentials.write`. Installed manifest schemas are resolved inside the application, never supplied by callers. Result of a write is a fixed success/providerId acknowledgment with optional numeric version, not the owner’s input echo.
- Plugins: list/get/schema/docs/data. Metadata comes from the application's existing catalog and installed-plugin owner, including retained versions, source, capabilities, public configuration schema and official documentation links. Secret and writeOnly schema entries expose `writeOnly` metadata without defaults, enums, dynamic options or values, including nested array items and inherited secret containers. Container defaults and presets are filtered through their recursive schema. Presets are filtered through their schema. `plugins.data` is the same persisted-connection dynamic UI-data path as `channels.options`, not an arbitrary plugin operation. Documentation returns the manifest's explanatory metadata and links; it does not download external documentation pages.

## Required typed owner channels

The integration includes corresponding owner changes. These are not aliases to unrestricted settings or arbitrary runtime dispatch:

- `local-data:channel-config-list []` → `{providers, version}`, already schema-filtered public records.
- `local-data:channel-config-patch [{providerId,create,patch,credentials?,capabilityCredentials?,expectedVersion?}]` → application acknowledgment. Deep patch and secret persistence belong to the existing config owner and preserve untouched settings/credentials.
- `local-data:channel-config-options [{providerId,capability,action}]` → application-projected dynamic UI data. The app resolves the persisted connection and installed plugin, hydrates credentials internally and removes credential values from output/errors.
- `local-data:style-references-patch [{styleId,refImages}]` → updated public style reference data.

## Output safety and useful failure information

Account output consists only of five booleans. Task output includes IDs, status/progress, target IDs, result generation ID and public result/local URLs. Frozen inputs, raw provider responses and arbitrary error strings are excluded. Known error codes receive local explanatory text; unknown codes direct the user to the app. Batch/workflow results use a strict public projection, including nested step progress and normalized errors, rather than recursively returning arbitrary options.

Raw credentials are never accepted as command-line flags by the client; route permission enforcement is independent of client transport choices. There is no secret-read/reveal or general settings-export method. Production channel public projections and credential storage are tested in the config-owner integration, not simulated by these route tests.

## Verification

24 Rust route tests run against a controlled Host in the actual agent crate with authentic config/task-domain dependencies. They cover every registered intent's existing IPC dispatch, scope rejection before side effects, real live/history ownership, write-only credential grants/no result echo, discovery config-injection rejection, public task result locations, schema/preset secret filtering, exact batch run identity, export retry permission, global workflow scope and nested orchestration error projection.

These are functional adapter tests, not real desktop/window E2E, not a real plugin login, and not paid image/video generation. Actual application patch installation, desktop pairing UI and real provider execution still require their separate platform acceptance checks.
