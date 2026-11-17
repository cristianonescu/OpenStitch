// A fake model endpoint for the browser Assistant's page test (#766): no network, no real key.
// It speaks the Claude Messages API (POST /v1/messages) and OpenAI-compatible chat completions
// (POST /v1/chat/completions), both streamed, and answers the page's CORS preflight as the real
// services do for a page. Each reply depends only on the request, so it needs no state:
//   - the last message holds a tool result: a short text answer, "Done: <the result's start>";
//   - else the user's text names what to do: "export" asks for design.export to the page's save
//     folder (a file command, so the panel's approval card comes up first); anything else draws
//     a rectangle with execute shape.rectangle.
// Every request is recorded (method, path, headers, body) for the test to check.

import { createServer } from "node:http";

export async function startFakeModel() {
  const requests = [];
  const server = createServer((req, res) => {
    const cors = {
      "access-control-allow-origin": req.headers.origin ?? "*",
      "access-control-allow-headers": "content-type, x-api-key, authorization, anthropic-version, anthropic-beta, anthropic-dangerous-direct-browser-access",
      "access-control-allow-methods": "POST, OPTIONS",
    };
    if (req.method === "OPTIONS") {
      res.writeHead(204, cors).end();
      return;
    }
    let raw = "";
    req.on("data", (c) => (raw += c));
    req.on("end", () => {
      let body = null;
      try {
        body = JSON.parse(raw);
      } catch {}
      requests.push({ method: req.method, path: req.url, headers: req.headers, body });
      const sse = (text) => {
        res.writeHead(200, { ...cors, "content-type": "text/event-stream", "cache-control": "no-cache" });
        res.end(text);
      };
      if (req.url === "/v1/messages") return sse(anthropic(body));
      if (req.url === "/v1/chat/completions") return sse(openai(body));
      res.writeHead(404, cors).end(JSON.stringify({ error: { message: "not found" } }));
    });
  });
  await new Promise((ok) => server.listen(0, "127.0.0.1", ok));
  return { address: `http://127.0.0.1:${server.address().port}`, requests, close: () => server.close() };
}

/// What the request asks for: { result } after a tool call, else { tool, input }.
function plan(lastText, toolResult) {
  if (toolResult != null) return { result: toolResult };
  if (/export/i.test(lastText)) {
    return { tool: "execute", input: { command: "design.export", params: { path: "/web/save/assistant.dst", format: "dst" } } };
  }
  return { tool: "execute", input: { command: "shape.rectangle", params: { x: 10, y: 10, width: 20, height: 10 } } };
}

const textOf = (content) =>
  typeof content === "string" ? content : (content ?? []).map((p) => p.text ?? p.content?.[0]?.text ?? "").join(" ");

function anthropic(body) {
  const last = body?.messages?.at(-1) ?? {};
  const result = (Array.isArray(last.content) ? last.content : []).find((b) => b.type === "tool_result");
  const p = plan(textOf(last.content), result ? textOf(result.content) || "(empty)" : null);
  const ev = (e) => `event: ${e.type}\ndata: ${JSON.stringify(e)}\n\n`;
  const out = [ev({ type: "message_start", message: { id: "msg_fake", type: "message", role: "assistant", content: [], usage: { input_tokens: 12, output_tokens: 1 } } })];
  if (p.result != null) {
    out.push(ev({ type: "content_block_start", index: 0, content_block: { type: "text", text: "" } }));
    out.push(ev({ type: "content_block_delta", index: 0, delta: { type: "text_delta", text: `Done: ${p.result.slice(0, 400)}` } }));
    out.push(ev({ type: "content_block_stop", index: 0 }));
    out.push(ev({ type: "message_delta", delta: { stop_reason: "end_turn" }, usage: { output_tokens: 7 } }));
  } else {
    out.push(ev({ type: "content_block_start", index: 0, content_block: { type: "tool_use", id: "toolu_fake", name: p.tool, input: {} } }));
    out.push(ev({ type: "content_block_delta", index: 0, delta: { type: "input_json_delta", partial_json: JSON.stringify(p.input) } }));
    out.push(ev({ type: "content_block_stop", index: 0 }));
    out.push(ev({ type: "message_delta", delta: { stop_reason: "tool_use" }, usage: { output_tokens: 9 } }));
  }
  out.push(ev({ type: "message_stop" }));
  return out.join("");
}

function openai(body) {
  const last = body?.messages?.at(-1) ?? {};
  const p = plan(textOf(last.content), last.role === "tool" ? textOf(last.content) || "(empty)" : null);
  const chunk = (delta, finish) => `data: ${JSON.stringify({ choices: [{ index: 0, delta, finish_reason: finish }] })}\n\n`;
  const out = [];
  if (p.result != null) {
    out.push(chunk({ role: "assistant", content: `Done: ${p.result.slice(0, 400)}` }, null));
    out.push(chunk({}, "stop"));
  } else {
    out.push(chunk({ role: "assistant", tool_calls: [{ index: 0, id: "call_fake", type: "function", function: { name: p.tool, arguments: JSON.stringify(p.input) } }] }, null));
    out.push(chunk({}, "tool_calls"));
  }
  out.push(`data: ${JSON.stringify({ choices: [], usage: { prompt_tokens: 12, completion_tokens: 5 } })}\n\n`);
  out.push("data: [DONE]\n\n");
  return out.join("");
}
