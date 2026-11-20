// ObservationController — the tap-to-observe flow.
//
// Every line it emits is tied to a request that actually ran. There is no timer that invents
// discoveries, and a stale session can never overwrite a newer one.

const MIN_VISIBLE_MS = 900;      // keep the sequence readable when the server answers instantly
const TIMEOUT_MS = 12000;

function makeObserve({ api }) {
  let seq = 0;

  function session() {
    const id = ++seq;
    return {
      id,
      stale: () => id !== seq,
      cancel() { if (id === seq) seq += 1; },
    };
  }

  /** Steps run in order; each step is a real call, and its narrative is emitted only after it returns. */
  async function run({ onStep, timeoutMs = TIMEOUT_MS }) {
    const s = session();
    const started = Date.now();
    const emit = (stage, text, tone) => { if (!s.stale()) onStep({ stage, text, tone: tone || "calm" }); };
    const guard = (p) => Promise.race([
      p,
      new Promise((_, rej) => setTimeout(() => rej(new Error("TIMEOUT")), timeoutMs)),
    ]);

    try {
      emit("STARTING", "正在开始观察");

      const [me, devs] = await guard(Promise.all([api.me(), api.myDevices()]));
      if (s.stale()) return { cancelled: true };
      if (me.status !== 200 || devs.status !== 200) throw new Error("REQUEST_FAILED");
      const devices = devs.status === 200 ? (devs.json.devices || []) : [];
      emit("DISCOVERING_CONNECTIONS",
        devices.length ? `发现 ${devices.length} 台已连接设备` : "尚未连接可观察的设备",
        devices.length ? "found" : "calm");

      const obs = await guard(api.myObservations());
      if (s.stale()) return { cancelled: true };
      if (obs.status !== 200) throw new Error("REQUEST_FAILED");
      const observations = obs.status === 200 ? (obs.json.observations || []) : [];
      emit("READING_OBSERVATIONS",
        observations.length ? `发现 ${observations.length} 条观察记录` : "暂时没有新的模型使用记录",
        observations.length ? "found" : "calm");

      const latest = observations[0] || null;
      if (latest) {
        emit("EVALUATING_EVIDENCE",
          latest.signature_status === "VALID" ? "记录签名有效" : "记录签名尚未通过校验",
          "calm");
        if (latest.signature_status === "VALID") {
          emit("EVALUATING_EVIDENCE", "来源尚未获得独立核验", "calm");
        }
      }

      const elapsed = Date.now() - started;
      if (elapsed < MIN_VISIBLE_MS) await new Promise((r) => setTimeout(r, MIN_VISIBLE_MS - elapsed));
      if (s.stale()) return { cancelled: true };

      return {
        cancelled: false,
        me: me.status === 200 ? me.json : null,
        devices,
        observations,
        latest,
      };
    } catch (e) {
      if (s.stale()) return { cancelled: true };
      const timedOut = String(e && e.message) === "TIMEOUT";
      emit("ERROR", timedOut ? "观察超时，请重试" : "暂时无法连接观察服务", "error");
      return { error: timedOut ? "TIMEOUT" : "ERROR" };
    }
  }

  return { run, cancel: () => { seq += 1; } };
}

module.exports = { makeObserve, MIN_VISIBLE_MS, TIMEOUT_MS };
