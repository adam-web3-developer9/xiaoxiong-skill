export class AgentError extends Error {
  constructor(code, message, details = {}) {
    super(message);
    this.name = "AgentError";
    this.code = code;
    this.details = details;
  }
}
export const failure = (code, message, details) =>
  new AgentError(code, message, details);
export function assert(value, message, code = "INVALID_ARGUMENT") {
  if (!value) throw failure(code, message);
}
// Defense in depth only. The application's projection is the security boundary.
const secretKey =
  /(?:api.?key|private.?key|secret|password|authorization|cookie|access.?token|refresh.?token|secondary.?api.?key|pollSecret|^token$|value_base64|fingerprint|credentialRef)/i;
export function redact(value, secrets = []) {
  if (typeof value === "string") {
    let out = value;
    for (const secret of secrets)
      if (typeof secret === "string" && secret.length > 3)
        out = out.split(secret).join("[redacted]");
    return out;
  }
  if (Array.isArray(value)) return value.map((v) => redact(v, secrets));
  if (value && typeof value === "object")
    return Object.fromEntries(
      Object.entries(value).map(([k, v]) => [
        k,
        secretKey.test(k) && typeof v !== "boolean"
          ? "[redacted]"
          : redact(v, secrets),
      ]),
    );
  return value;
}
export function secretValues(value, result = [], insideSecret = false) {
  if (typeof value === "string" && insideSecret) result.push(value);
  else if (value && typeof value === "object")
    for (const [k, v] of Object.entries(value)) {
      const sensitive =
        insideSecret ||
        secretKey.test(k) ||
        /^(?:capability)?credentials$/i.test(k);
      if (sensitive && typeof v === "string") result.push(v);
      else secretValues(v, result, sensitive);
    }
  return result;
}
export function publicError(error, secrets = []) {
  return redact(
    {
      code: error.code || "AGENT_ERROR",
      message: error.code
        ? error.message
        : "Operation failed; inspect local application diagnostics",
      details: error.details || {},
    },
    secrets,
  );
}
