/** Methods map to application-owned capabilities; nothing routes arbitrary IPC. */
export const METHODS = {
  capabilities: { method: "capabilities", read: true },
  "account status": { method: "account.status", read: true },
  "draft list": { method: "draft.list", read: true },
  "draft search": { method: "draft.search", read: true, id: "query" },
  "draft get": { method: "draft.get", read: true, id: "draftId" },
  "draft rows": { method: "draft.rows", read: true, id: "draftId", file: true },
  "draft apply": { method: "draft.apply", file: true },
  "draft create": { method: "draft.create", file: true },
  "draft copy": { method: "draft.copy", file: true },
  "task actions": { method: "task.actions", read: true },
  "task list": { method: "task.list", read: true },
  "task get": { method: "task.get", read: true, id: "taskId" },
  "task estimate": { method: "task.estimate", read: true, file: true },
  "task intent": { method: "task.intent", file: true },
  "task cancel": { method: "task.cancel", id: "taskId" },
  "task retry": { method: "task.retry", id: "taskId" },
  "task recheck": { method: "task.recheck", id: "taskId" },
  "batch list": { method: "batch.list", read: true },
  "batch get": { method: "batch.get", read: true, id: "draftId" },
  "batch estimate": { method: "batch.estimate", read: true, file: true },
  "batch start": { method: "batch.start", file: true },
  "batch progress": { method: "batch.progress", read: true, file: true },
  "batch pause": { method: "batch.pause", file: true },
  "batch stop": { method: "batch.stop", file: true },
  "batch resume": { method: "batch.resume", file: true },
  "batch retry-stage": { method: "batch.retryStage", file: true },
  "workflow estimate": { method: "workflow.estimate", read: true, file: true },
  "workflow start": { method: "workflow.start", file: true },
  "workflow get": { method: "workflow.get", read: true, id: "workflowId" },
  "workflow list": { method: "workflow.list", read: true },
  "workflow resume": { method: "workflow.resume", file: true },
  "workflow cancel": { method: "workflow.cancel", id: "workflowId" },
  "channels list": { method: "channels.list", read: true },
  "channels get": { method: "channels.get", read: true, id: "providerId" },
  "channels create": { method: "channels.create", file: true },
  "channels update": { method: "channels.update", file: true },
  "channels models": {
    method: "channels.models",
    read: true,
    id: "providerId",
  },
  "channels options": { method: "channels.options", read: true, file: true },
  "plugins list": { method: "plugins.list", read: true },
  "plugins get": { method: "plugins.get", read: true, id: "pluginId" },
  "plugins schema": { method: "plugins.schema", read: true, id: "pluginId" },
  "plugins docs": { method: "plugins.docs", read: true, id: "pluginId" },
  "plugins data": { method: "plugins.data", read: true, file: true },
  "style list": { method: "style.list", read: true },
  "style refs": { method: "style.setReferences", file: true },
};
export function toolName(method) {
  return "xiaoxiong_" + method.replace(/\./g, "_");
}
export function toolsCatalog() {
  return [
    ...new Map(Object.values(METHODS).map((v) => [v.method, v])).values(),
  ].map((v) => ({
    name: toolName(v.method),
    description: `小熊AI ${v.method}. Requires an application-approved session. ${v.read ? "Read-only." : "Writes through the application; do not retry an uncertain outcome."}`,
    inputSchema: inputSchema(v.method),
    annotations: {
      readOnlyHint: !!v.read,
      destructiveHint: !v.read,
      idempotentHint: false,
      openWorldHint: !v.read,
    },
  }));
}
const id = (description) => ({ type: "string", minLength: 1, description });
const obj = { type: "object", additionalProperties: true };
const defs = {
  draftId: id("Existing 小熊AI draft ID"),
  taskId: id("Existing task ID"),
  providerId: id("Saved provider/channel ID"),
  pluginId: id("Installed/catalog plugin ID"),
  workflowId: id("Existing workflow ID"),
  runId: id("Existing batch run ID"),
  episodeId: id("Existing episode ID"),
  stepKey: id("Existing workflow step key"),
  stage: id("Existing stage name"),
  requestId: id(
    "Stable unique request ID. Preserve it for this exact original request.",
  ),
  commandId: id("Stable command ID for this exact command payload"),
  capability: id("Capability declared by the saved provider/plugin"),
  action: id(
    "Action advertised by task.actions or plugin config optionsSource",
  ),
  query: { type: "string" },
  status: { type: "string" },
  type: { type: "string" },
  options: obj,
  run: obj,
  draft: obj,
  target: {
    type: "object",
    properties: {
      draftId: id("Authorized draft ID"),
      episodeId: id("Episode ID"),
      ids: { type: "array", items: { type: "string" } },
    },
    additionalProperties: true,
  },
  kind: id("Existing workflow kind"),
  label: { type: "string" },
  expectedVersion: {
    type: "integer",
    minimum: 0,
    description:
      "Optional configurationVersion previously read from the provider; protects against concurrent changes",
  },
  patch: {
    ...obj,
    description:
      "Only public configuration fields. Do not place credentials here.",
  },
  credentials: {
    ...obj,
    writeOnly: true,
    description:
      "New credential values only. Never ask to read saved secrets. Input is never returned.",
  },
  capabilityCredentials: { ...obj, writeOnly: true },
  styleId: id("Existing style ID"),
  refImages: { type: "array", items: { type: "string" } },
  entities: { type: "array", items: obj },
  commands: {
    type: "array",
    items: {
      type: "object",
      properties: {
        op: id("Typed operation from the app command contract"),
        baseVersion: {
          type: "number",
          description: "Previously read numeric revision, never latest",
        },
      },
      required: ["op"],
      additionalProperties: true,
    },
  },
  origin: {
    type: "object",
    properties: {
      kind: { const: "user" },
      windowId: { type: "string", pattern: "^agent:" },
    },
    required: ["kind", "windowId"],
    additionalProperties: false,
  },
};
const inputs = {
  "draft.search": ["query"],
  "draft.get": ["draftId"],
  "draft.rows": ["draftId", "entities"],
  "draft.apply": ["commandId", "draftId", "commands", "origin?"],
  "draft.create": ["draft"],
  "draft.copy": ["draftId", "options?"],
  "task.list": ["draftId?", "status?", "type?"],
  "task.get": ["taskId"],
  "task.cancel": ["taskId"],
  "task.retry": ["taskId"],
  "task.recheck": ["taskId"],
  "task.intent": ["requestId", "action", "target", "options?"],
  "task.estimate": ["requestId", "action", "target", "options?"],
  "batch.get": ["draftId"],
  "batch.estimate": ["draftId", "run"],
  "batch.start": ["draftId", "run"],
  "batch.progress": ["draftId", "runId"],
  "batch.pause": ["draftId", "runId"],
  "batch.stop": ["draftId", "runId"],
  "batch.resume": ["draftId", "runId"],
  "batch.retryStage": ["draftId", "runId", "episodeId", "stage"],
  "workflow.list": ["draftId"],
  "workflow.get": ["workflowId"],
  "workflow.cancel": ["workflowId"],
  "workflow.resume": ["workflowId", "stepKey"],
  "workflow.start": ["kind", "target", "options?", "label?"],
  "workflow.estimate": ["kind", "target", "options?", "label?"],
  "style.setReferences": ["styleId", "refImages"],
  "channels.get": ["providerId"],
  "channels.create": [
    "providerId",
    "patch",
    "credentials?",
    "capabilityCredentials?",
    "expectedVersion?",
  ],
  "channels.update": [
    "providerId",
    "patch",
    "credentials?",
    "capabilityCredentials?",
    "expectedVersion?",
  ],
  "channels.models": ["providerId", "capability"],
  "channels.options": ["providerId", "capability", "action"],
  "plugins.data": ["providerId", "capability", "action"],
  "plugins.get": ["pluginId"],
  "plugins.schema": ["pluginId"],
  "plugins.docs": ["pluginId"],
};
export function inputSchema(method) {
  const keys = inputs[method] || [];
  return {
    type: "object",
    properties: Object.fromEntries(
      keys.map((k) => {
        const key = k.replace(/\?$/, "");
        return [key, defs[key] || obj];
      }),
    ),
    required: keys.filter((k) => !k.endsWith("?")),
    additionalProperties: false,
  };
}
