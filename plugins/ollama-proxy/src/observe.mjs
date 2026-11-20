// Turns an Ollama response stream into a completion verdict.
//
// The proxy necessarily sees response bytes while forwarding them. This module parses
// them with a hard rule: nothing it returns may contain user content. Only the model
// name, the done flag, and counters leave this boundary.

/** Non-streaming JSON body: one object with `.done === true`. */
export function observeJson(text) {
  let obj;
  try { obj = JSON.parse(text); } catch { return { completed: false, reason: "unparseable" }; }
  if (!obj || typeof obj !== "object") return { completed: false, reason: "not_object" };
  if (obj.error) return { completed: false, reason: "upstream_error" };
  if (obj.done !== true) return { completed: false, reason: "no_done_flag" };
  return {
    completed: true,
    model: typeof obj.model === "string" ? obj.model : null,
    evalCount: Number.isFinite(obj.eval_count) ? obj.eval_count : null,
    promptEvalCount: Number.isFinite(obj.prompt_eval_count) ? obj.prompt_eval_count : null,
  };
}

/**
 * Incremental NDJSON observer. Feed chunks, then call finish().
 * A stream counts only if it produced a chunk with done:true AND reached the end
 * normally. Errors and aborted streams return completed:false.
 */
export function createStreamObserver() {
  let buffer = "";
  let sawDone = false;
  let model = null;
  let evalCount = null;
  let promptEvalCount = null;
  let upstreamError = false;
  let sawAny = false;

  function consumeChunk(text) {
    buffer += text;
    let idx;
    while ((idx = buffer.indexOf("\n")) >= 0) {
      const line = buffer.slice(0, idx).trim();
      buffer = buffer.slice(idx + 1);
      if (!line) continue;
      sawAny = true;
      let obj;
      try { obj = JSON.parse(line); } catch { continue; }   // partial/split line: ignore
      if (obj?.error) upstreamError = true;
      if (typeof obj?.model === "string" && !model) model = obj.model;
      if (obj?.done === true) {
        sawDone = true;
        if (Number.isFinite(obj.eval_count)) evalCount = obj.eval_count;
        if (Number.isFinite(obj.prompt_eval_count)) promptEvalCount = obj.prompt_eval_count;
      }
    }
  }

  function finish({ aborted = false } = {}) {
    // a trailing line without a newline still counts if it is the done chunk
    if (buffer.trim()) {
      try {
        const obj = JSON.parse(buffer.trim());
        if (obj?.error) upstreamError = true;
        if (obj?.done === true) sawDone = true;
        if (typeof obj?.model === "string" && !model) model = obj.model;
      } catch { /* ignore */ }
      buffer = "";
    }
    if (aborted) return { completed: false, reason: "aborted", model };
    if (upstreamError) return { completed: false, reason: "upstream_error", model };
    if (!sawDone) return { completed: false, reason: sawAny ? "no_done_chunk" : "empty_stream", model };
    return { completed: true, model, evalCount, promptEvalCount };
  }

  return { consumeChunk, finish };
}
