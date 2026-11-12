---
name: xiaoxiong-ai
description: Operate 小熊AI through its official local app interface for projects, scripts, assets, storyboards, generation tasks, exports, accounts and public channel configuration. Use with the xiaoxiong CLI; pair with creative Agent Skills for content creation.
---

# 小熊AI operator

Requires Node.js 22+, xiaoxiong CLI, and a running 小熊AI app with the v2 agent authorization endpoint.

Use this skill to turn the user's requested creative work into operations in their running 小熊AI application. Creative Skills produce content; this operator applies it through the official app. Never bypass the app by opening its database, scraping internal credentials or directly contacting model providers.

## Connect once through the app

1. Run `xiaoxiong auth status`. If not authorized, run `xiaoxiong auth login` and have the user approve the displayed request inside 小熊AI. Do not ask them for a copied application API key or token. Request only the scopes needed; `xiaoxiong auth login --scopes all` explicitly requests the full workflow, including write-only channel credentials. Scope expansion requires another visible app approval.
2. Run `xiaoxiong capabilities` to discover the current method catalog and scopes. The application is the authority: only invoke advertised methods and fields. If v2 authorization is unavailable, request an app upgrade. Never fall back to reading an old descriptor.token.
3. Use `xiaoxiong account status` for the application's safe account summary. Authentication to the agent interface does not replace account login, subscription, balance or generation permissions.
4. `xiaoxiong auth logout` revokes/clears the agent connection through its supported workflow. Never log authentication material.

## Work

Read [operations.md](references/operations.md) for CLI routing, write safety and task lifecycle. Before creating or importing scripts, shots or prompts from an external creative Skill, read [content-format.md](references/content-format.md). For third-party packages, also read [creative-skills.md](references/creative-skills.md).

External creative Skills own the writing, shot design and prompt wording. Each imported shot needs its script content (`scriptContent`) and seconds (`duration`); internal production metadata such as `cutPlan` and visual beats is optional. The script layout is a recommendation. The required prompt convention is the application's asset-reference syntax so the app can match, upload and submit the actual media. Do not regenerate the creator's work with application templates unless the user requests it.

Read the requested draft and relevant rows before editing. Preserve IDs and unrelated values. Use the bundled versioned command contracts and the installed app capability list; never invent an entity field, command type, task action or generation option. All calls, including generic `xiaoxiong call METHOD --file JSON`, go through the official app endpoint and its validation.

Show material changes and costs for the user's approval where required. Generation, paid operations, deletion, channel changes and exports remain subject to application permissions and approval gates. A Skill is not authorization. Stop on denied access or a rejected approval.

Keep the original commandId and requestId across a write's lifecycle. A timeout is an unknown outcome, not a failed operation: inspect status first and never automatically resend, generate a fresh ID, or repeat a paid generation. Report unresolved outcomes explicitly.

## Channel configuration boundary

Public channel configuration may be read and updated through `channels list/get/update` when advertised by the app. These responses must be safe projections, not raw provider records. Never read or output private keys, credentials, tokens, cookies or other secrets. Ordinary patches preserve existing secrets. New credentials may be written only through advertised channels create/update using stdin or a JSON file, never command-line arguments; they must not be printed, logged or returned. Use the app UI when no safe write-only route exists. If an authentication destination changes, existing secrets must not be forwarded: require reauthorization or a new key.
