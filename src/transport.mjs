import net from "node:net";
import { EventEmitter } from "node:events";
import { randomUUID } from "node:crypto";
import { failure } from "./errors.mjs";
/** No mutation is retried automatically, even when a timeout leaves an uncertain outcome. */
export class Transport extends EventEmitter {
  constructor(
    endpoint,
    {
      timeoutMs = 30_000,
      maxFrameBytes = 32 * 1024 * 1024,
      socketFactory = (endpoint) => net.createConnection(endpoint),
    } = {},
  ) {
    super();
    this.endpoint = endpoint;
    this.socketFactory = socketFactory;
    this.timeoutMs = timeoutMs;
    this.maxFrameBytes = maxFrameBytes;
    this.pending = new Map();
    this.socket = null;
    this.token = undefined;
  }
  async connect() {
    if (this.socket && !this.socket.destroyed) return;
    const socket = this.socketFactory(this.endpoint);
    this.socket = socket;
    socket.setEncoding("utf8");
    let buffer = "";
    const interrupt = (code, message) => {
      for (const [id, p] of this.pending) {
        clearTimeout(p.timer);
        this.pending.delete(id);
        p.reject(
          p.write && p.sent
            ? failure(
                "AGENT_WRITE_UNCONFIRMED",
                "The write was sent but its receipt was not received. Query its outcome before retrying.",
                {
                  method: p.method,
                  commandId: p.params.commandId,
                  requestId: p.params.requestId,
                  taskId: p.params.taskId,
                },
              )
            : failure(code, message),
        );
      }
    };
    socket.on("data", (chunk) => {
      buffer += chunk;
      if (
        Buffer.byteLength(buffer) > this.maxFrameBytes &&
        !buffer.includes("\n")
      ) {
        interrupt(
          "PROTOCOL_ERROR",
          "Application response exceeds transport limit",
        );
        socket.destroy();
        return;
      }
      let n;
      while ((n = buffer.indexOf("\n")) >= 0) {
        const line = buffer.slice(0, n);
        buffer = buffer.slice(n + 1);
        let value;
        try {
          value = JSON.parse(line);
        } catch {
          interrupt("PROTOCOL_ERROR", "Invalid application response");
          socket.destroy();
          return;
        }
        if (value.event) {
          this.emit("event", value);
          continue;
        }
        const p = this.pending.get(value.id);
        if (!p) continue;
        clearTimeout(p.timer);
        this.pending.delete(value.id);
        if (value.ok === true) p.resolve(value.result);
        else if (value.ok === false)
          p.reject(
            failure(
              value.error?.code || "REMOTE_ERROR",
              value.error?.message || "Application operation failed",
              value.error?.details || {},
            ),
          );
        else
          p.reject(
            failure("PROTOCOL_ERROR", "Invalid application response envelope"),
          );
      }
    });
    socket.on("error", () =>
      interrupt("APP_DISCONNECTED", "Application connection failed"),
    );
    socket.on("close", () => {
      interrupt("APP_DISCONNECTED", "Application connection closed");
      this.emit("disconnect");
    });
    await new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        socket.destroy();
        reject(failure("APP_TIMEOUT", "Application connection timed out"));
      }, this.timeoutMs);
      const clear = () => {
        clearTimeout(timer);
        socket.off("connect", ok);
        socket.off("error", err);
      };
      const ok = () => {
        clear();
        resolve();
      };
      const err = () => {
        clear();
        reject(
          failure(
            "APP_UNAVAILABLE",
            "Unable to connect. Restart the application or select the correct data directory.",
          ),
        );
      };
      socket.once("connect", ok);
      socket.once("error", err);
    });
  }
  request(
    method,
    params = {},
    { write = false, token = this.token, timeoutMs = this.timeoutMs } = {},
  ) {
    if (!this.socket || this.socket.destroyed)
      return Promise.reject(
        failure("APP_DISCONNECTED", "Not connected to the application"),
      );
    const id = randomUUID();
    let encoded;
    try {
      encoded =
        JSON.stringify({ id, method, params, ...(token ? { token } : {}) }) +
        "\n";
    } catch {
      return Promise.reject(
        failure("INVALID_ARGUMENT", "Request must be JSON serializable"),
      );
    }
    return new Promise((resolve, reject) => {
      const p = { resolve, reject, write, sent: false, method, params };
      p.timer = setTimeout(() => {
        this.pending.delete(id);
        reject(
          write && p.sent
            ? failure(
                "AGENT_WRITE_UNCONFIRMED",
                "Write timed out without a receipt. Do not submit a new request.",
                {
                  method,
                  commandId: params.commandId,
                  requestId: params.requestId,
                  taskId: params.taskId,
                },
              )
            : failure("APP_TIMEOUT", "Application response timed out"),
        );
        this.socket.destroy();
      }, timeoutMs);
      this.pending.set(id, p);
      try {
        this.socket.write(encoded, (error) => {
          if (error) this.socket.destroy();
        });
        p.sent = true;
      } catch {
        clearTimeout(p.timer);
        this.pending.delete(id);
        reject(failure("APP_DISCONNECTED", "Request could not be sent"));
      }
    });
  }
  close() {
    this.socket?.destroy();
    this.socket = null;
    this.token = undefined;
  }
}
