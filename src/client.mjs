import { createHash, randomUUID } from "node:crypto";
import { setTimeout as sleep } from "node:timers/promises";
import { Transport } from "./transport.mjs";
import { discover } from "./discovery.mjs";
import {
  stateHome,
  loadCredential,
  saveCredential,
  deleteCredential,
  writeReceipt,
} from "./storage.mjs";
import { assert, failure, redact, secretValues } from "./errors.mjs";
export const CLIENT_VERSION = "0.1.0";
export const ALL_SCOPES = [
  "read",
  "edit",
  "generate",
  "export",
  "channels.read",
  "channels.write",
  "channels.credentials.write",
];
export const READ_METHODS = new Set([
  "capabilities",
  "capabilities.list",
  "account.status",
  "draft.list",
  "draft.search",
  "draft.get",
  "draft.rows",
  "task.list",
  "task.get",
  "task.actions",
  "task.estimate",
  "batch.estimate",
  "workflow.estimate",
  "workflow.get",
  "workflow.list",
  "batch.get",
  "batch.list",
  "batch.progress",
  "plugins.list",
  "plugins.get",
  "plugins.schema",
  "plugins.docs",
  "plugins.data",
  "channels.list",
  "channels.get",
  "channels.models",
  "channels.options",
  "style.list",
  "auth.status",
  "events.poll",
]);
export class AgentClient {
  constructor(options = {}) {
    this.options = options;
    this.home = options.home || stateHome();
    this.identity = {
      id: "xiaoxiong-agent",
      name: "小熊AI Agent Bridge",
      version: CLIENT_VERSION,
      ...options.client,
    };
    this.closed = false;
    this.warnings = [];
  }
  async connect() {
    if (this.transport?.socket && !this.transport.socket.destroyed)
      return this.hello;
    this.authenticatedTransport = null;
    this.descriptor = await discover(this.options);
    this.transport = new Transport(this.descriptor.endpoint, this.options);
    await this.transport.connect();
    this.hello = await this.transport.request("hello", {
      protocol: 2,
      schemaVersion: 1,
      client: this.identity,
      dataRoot: this.descriptor.dataRoot,
    });
    if (this.hello?.protocol !== 2) {
      this.close();
      throw failure(
        "PROTOCOL_UNSUPPORTED",
        "Application does not support the Agent v2 protocol",
      );
    }
    return this.hello;
  }
  async login({
    scopes = ["read", "edit"],
    draftIds = ["*"],
    onPending = () => {},
    signal,
  } = {}) {
    assert(
      Array.isArray(scopes) && scopes.every((x) => ALL_SCOPES.includes(x)),
      "Unknown authorization scope",
    );
    assert(
      Array.isArray(draftIds) &&
        draftIds.every((x) => typeof x === "string" && x),
      "draftIds must be an array of nonempty IDs; [] means no draft access",
    );
    await this.connect();
    const pending = await this.transport.request("auth.request", {
      scopes,
      draftIds,
    });
    onPending(
      redact({
        requestId: pending.requestId,
        userCode: pending.userCode,
        expiresAt: pending.expiresAt,
        scopes,
        draftIds,
        message: "在小熊AI的 Agent 连接面板核对验证码后，点击允许。",
      }),
    );
    const expiresAt = Number(pending.expiresAt) || Date.now() + 300_000;
    try {
      while (Date.now() < expiresAt) {
        if (signal?.aborted)
          throw failure("AUTH_CANCELLED", "Authorization cancelled");
        await sleep(
          Math.max(100, Number(pending.intervalMs) || 1000),
          undefined,
          { signal },
        );
        let result;
        try {
          result = await this.transport.request("auth.poll", {
            requestId: pending.requestId,
            pollSecret: pending.pollSecret,
          });
        } catch (e) {
          if (["AUTH_PENDING", "AUTHORIZATION_PENDING"].includes(e.code))
            continue;
          throw e;
        }
        if (["pending", "authorization_pending"].includes(result?.status))
          continue;
        if (result?.status === "denied")
          throw failure(
            "AUTH_DENIED",
            "Authorization was denied in the application",
          );
        if (result?.status === "expired")
          throw failure("AUTH_EXPIRED", "Authorization request expired");
        if (result?.token && result?.sessionId) {
          await this.transport.request("auth.resume", {
            sessionId: result.sessionId,
            token: result.token,
          });
          this.transport.token = result.token;
          this.credential = result;
          this.authenticatedTransport = this.transport;
          await saveCredential(this.home, this.descriptor.dataRoot, {
            ...result,
            client: this.identity,
            instanceId: this.descriptor.instanceId,
            dataRoot: this.descriptor.dataRoot,
          });
          return this.publicSession(result);
        }
        throw failure("PROTOCOL_ERROR", "Invalid authorization result");
      }
    } catch (e) {
      if (signal?.aborted)
        throw failure(
          "AUTH_CANCELLED",
          "Authorization cancelled; unapproved request will expire",
        );
      throw e;
    }
    throw failure(
      "AUTH_EXPIRED",
      "Authorization expired. Run auth login again.",
    );
  }
  publicSession(value = this.credential) {
    return {
      connected: !!value,
      sessionId: value?.sessionId,
      scopes: value?.scopes,
      draftIds: value?.draftIds,
      expiresAt: value?.expiresAt,
      dataRoot: this.descriptor?.dataRoot,
      client: this.identity,
    };
  }
  async authorize({ refresh = false } = {}) {
    await this.connect();
    if (
      !refresh &&
      this.credential &&
      this.authenticatedTransport === this.transport
    )
      return this.publicSession();
    const c =
      this.credential ||
      (await loadCredential(this.home, this.descriptor.dataRoot));
    if (!c)
      throw failure(
        "AUTH_REQUIRED",
        "Run xiaoxiong auth login, then approve the connection in 小熊AI.",
      );
    try {
      const resumed = await this.transport.request("auth.resume", {
        sessionId: c.sessionId,
        token: c.token,
      });
      const credential = { ...c };
      for (const key of ["scopes", "draftIds", "expiresAt"])
        if (Object.hasOwn(resumed || {}, key)) credential[key] = resumed[key];
      if (JSON.stringify(credential) !== JSON.stringify(c))
        await saveCredential(this.home, this.descriptor.dataRoot, credential);
      this.transport.token = c.token;
      this.credential = credential;
      this.authenticatedTransport = this.transport;
      return this.publicSession();
    } catch (e) {
      if (/AUTH|GRANT|EXPIRED|REVOKED/.test(e.code || "")) {
        await deleteCredential(this.home, this.descriptor.dataRoot);
        this.credential = null;
        this.transport.token = undefined;
        this.authenticatedTransport = null;
      }
      throw e;
    }
  }
  async status() {
    await this.connect();
    try {
      return {
        ...(await this.authorize({ refresh: true })),
        application: this.hello,
      };
    } catch (e) {
      if (/AUTH|GRANT|EXPIRED|REVOKED/.test(e.code || ""))
        return {
          connected: false,
          application: this.hello,
          reason: e.code,
          message: "Run auth login to approve this client in the application.",
        };
      throw e;
    }
  }
  async logout() {
    await this.authorize();
    const result = await this.transport.request("auth.logout", {});
    await deleteCredential(this.home, this.descriptor.dataRoot);
    this.credential = null;
    this.transport.token = undefined;
    return result;
  }
  async call(method, params = {}, options = {}) {
    assert(
      typeof method === "string" &&
        method &&
        !method.startsWith("auth.") &&
        method !== "hello",
      "Use dedicated authentication commands",
    );
    await this.authorize();
    const write = options.write ?? !READ_METHODS.has(method);
    const secrets = [this.credential?.token, ...secretValues(params)];
    const requestId =
      params.commandId ||
      params.requestId ||
      (write ? randomUUID() : undefined);
    let record;
    if (write) {
      record = {
        requestId,
        method,
        hash: createHash("sha256").update(JSON.stringify(params)).digest("hex"),
        status: "sent",
        createdAt: Date.now(),
      };
      await writeReceipt(this.home, requestId, record);
    }
    let result;
    try {
      result = await this.transport.request(method, params, { write });
    } catch (e) {
      if (record) {
        try {
          await writeReceipt(this.home, requestId, {
            ...record,
            status:
              e.code === "AGENT_WRITE_UNCONFIRMED" ? "unconfirmed" : "failed",
            error: { code: e.code },
            finishedAt: Date.now(),
          });
        } catch {
          e.details = { ...e.details, receiptPersisted: false };
        }
        e.details = { ...e.details, localReceiptId: requestId };
      }
      e.details = redact(e.details, secrets);
      e.message = redact(e.message, secrets);
      throw e;
    }
    const publicResult = redact(result, secrets);
    if (record) {
      try {
        await writeReceipt(this.home, requestId, {
          ...record,
          status: "confirmed",
          finishedAt: Date.now(),
          result: publicResult,
        });
      } catch {
        this.warnings.push({
          code: "RECEIPT_SAVE_FAILED",
          message:
            "Application confirmed success, but the local receipt could not be saved. Do not repeat the operation.",
          localReceiptId: requestId,
        });
      }
    }
    return publicResult;
  }
  async waitTask(
    taskId,
    {
      intervalMs = 2000,
      timeoutMs = 30 * 60_000,
      onProgress = () => {},
      signal,
    } = {},
  ) {
    const start = Date.now();
    let last = "";
    while (Date.now() - start < timeoutMs) {
      if (signal?.aborted)
        throw failure(
          "WAIT_CANCELLED",
          "Stopped waiting. The application task continues.",
        );
      const task = await this.call("task.get", { taskId });
      if (!task) throw failure("TASK_NOT_FOUND", "Task was not found");
      const summary = JSON.stringify([
        task.status,
        task.progress,
        task.updatedAt,
      ]);
      if (summary !== last) {
        onProgress(task);
        last = summary;
      }
      if (
        [
          "completed",
          "succeeded",
          "success",
          "failed",
          "cancelled",
          "canceled",
        ].includes(task.status)
      )
        return task;
      await sleep(intervalMs, undefined, { signal });
    }
    throw failure(
      "WAIT_TIMEOUT",
      "Stopped waiting at the timeout. The application task continues.",
      { taskId },
    );
  }
  close() {
    this.closed = true;
    this.transport?.close();
    this.credential = null;
  }
}
