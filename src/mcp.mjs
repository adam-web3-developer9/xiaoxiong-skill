import readline from "node:readline";
import { METHODS, toolsCatalog, toolName } from "./catalog.mjs";
import { publicError } from "./errors.mjs";
/** MCP stdio JSON-RPC, no HTTP port; shares exact CLI/application permission boundary. */
export async function serveMcp(
  client,
  { input = process.stdin, output = process.stdout } = {},
) {
  const catalog = toolsCatalog(),
    byName = new Map(
      Object.values(METHODS).map((v) => [toolName(v.method), v]),
    );
  const send = (value) => output.write(JSON.stringify(value) + "\n");
  const handler = async (m) => {
    if (m.id === undefined) return;
    const result = (value) => send({ jsonrpc: "2.0", id: m.id, result: value });
    if (m.method === "initialize")
      return result({
        protocolVersion: ["2024-11-05", "2025-03-26", "2025-06-18"].includes(
          m.params?.protocolVersion,
        )
          ? m.params.protocolVersion
          : "2024-11-05",
        capabilities: { tools: { listChanged: false } },
        serverInfo: { name: "xiaoxiong-agent", version: "0.1.0" },
        instructions:
          "Run xiaoxiong auth login once and approve in 小熊AI. Tools reuse the app account. Skills guide work; the app owns data, credentials and authorization. Never read secret values or resubmit unconfirmed mutations.",
      });
    if (m.method === "ping") return result({});
    if (m.method === "tools/list") return result({ tools: catalog });
    if (m.method === "tools/call") {
      const spec = byName.get(m.params?.name);
      if (!spec)
        return send({
          jsonrpc: "2.0",
          id: m.id,
          error: { code: -32602, message: "Unknown tool" },
        });
      try {
        const value = await client.call(spec.method, m.params.arguments || {}, {
          write: !spec.read,
        });
        const content = [{ type: "text", text: JSON.stringify(value) }];
        if (client.warnings?.length)
          content.push({
            type: "text",
            text: JSON.stringify({ warnings: client.warnings.splice(0) }),
          });
        return result({ content, isError: false });
      } catch (e) {
        return result({
          content: [{ type: "text", text: JSON.stringify(publicError(e)) }],
          isError: true,
        });
      }
    }
    return send({
      jsonrpc: "2.0",
      id: m.id,
      error: { code: -32601, message: "Method not found" },
    });
  };
  // Sequential dispatch keeps mutation ordering and avoids reordered confirmations.
  const lines = readline.createInterface({ input, crlfDelay: Infinity });
  for await (const line of lines) {
    if (!line.trim()) continue;
    let m;
    try {
      m = JSON.parse(line);
    } catch {
      send({
        jsonrpc: "2.0",
        id: null,
        error: { code: -32700, message: "Parse error" },
      });
      continue;
    }
    await handler(m);
  }
  client.close();
}
