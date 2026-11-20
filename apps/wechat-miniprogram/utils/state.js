// Trust levels, kept as five separate facts. There is deliberately no boolean "verified".
const LEVELS = {
  UNPAIRED: "UNPAIRED",
  PAIRED_IDLE: "PAIRED_IDLE",
  LOCAL_OBSERVED: "LOCAL_OBSERVED",
  SIGNATURE_VALID: "SIGNATURE_VALID",
  SOURCE_APPROVED: "SOURCE_APPROVED",
  ACTIVATION_ELIGIBLE: "ACTIVATION_ELIGIBLE",
  ACTIVATED: "ACTIVATED",
};

const COPY = {
  UNPAIRED:            { title: "还没有连接电脑", next: "在电脑上运行插件并完成配对" },
  PAIRED_IDLE:         { title: "已连接，尚未观察到使用", next: "在电脑上通过插件调用一次模型，记录才会出现" },
  LOCAL_OBSERVED:      { title: "本机观察到使用", next: "等待服务器确认签名" },
  SIGNATURE_VALID:     { title: "记录已签名", next: "该记录尚未获得独立来源核准" },
  SOURCE_APPROVED:     { title: "来源符合政策", next: "仍需与本次传光绑定才能点亮" },
  ACTIVATION_ELIGIBLE: { title: "具备激活资格", next: "等待服务端完成激活" },
  ACTIVATED:           { title: "已被点亮", next: "这束光已在远方亮起" },
};

/** Derive the level from server facts, never from local timers or animations. */
function levelOf({ device, credential, transmission }) {
  if (transmission && transmission.state === "ACTIVATED") return LEVELS.ACTIVATED;
  if (!device || !device.paired) return LEVELS.UNPAIRED;
  if (!credential) return LEVELS.PAIRED_IDLE;
  if (credential.signature_status !== "VALID") return LEVELS.LOCAL_OBSERVED;
  if (credential.eligible && credential.eligible.activate) return LEVELS.ACTIVATION_ELIGIBLE;
  if (credential.source_class === "PLUGIN_OBSERVED") return LEVELS.SIGNATURE_VALID;
  return LEVELS.SOURCE_APPROVED;
}

function describe(level) { return COPY[level] ?? COPY.UNPAIRED; }

module.exports = { LEVELS, COPY, levelOf, describe };
