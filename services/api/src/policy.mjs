// Server policy knobs. Anything that could change the meaning of a number is
// configurable here and nowhere else.

export function loadPolicy(overrides = {}) {
  const env = process.env;
  const num = (key, fallback) => {
    const v = env[key];
    return v === undefined ? fallback : Number(v);
  };

  return {
    // One-time invitation lifetime. Short by design: a QR on screen is a bearer
    // token until it expires.
    invitationTtlMs: num("FL_INVITATION_TTL_MS", 8 * 60 * 1000),
    // How long a RECEIVED session waits for both confirmations.
    confirmTtlMs: num("FL_CONFIRM_TTL_MS", 10 * 60 * 1000),
    // A transmission may only be activated by usage that happened after it.
    activationWindowMs: num("FL_ACTIVATION_WINDOW_MS", 30 * 24 * 3600 * 1000),

    // Anti-abuse limits (F3 §7).
    maxInvitationsPerDay: num("FL_MAX_INVITATIONS_PER_DAY", 20),
    maxAttestPerHour: num("FL_MAX_ATTEST_PER_HOUR", 30),
    activationCooldownMs: num("FL_ACTIVATION_COOLDOWN_MS", 0),

    // Title policy. RESONANCE never counts; whether a NEW_DISCOVERY counts as a
    // transmission is a product decision, defaulting to "yes" because a genuinely
    // new capability really was transferred. The same receiver node can never be
    // counted twice for one helper thanks to unique(helper_id, helped_id).
    countFirstLightAsTransmission: true,
    countNewDiscoveryAsTransmission:
      (env.FL_COUNT_NEW_DISCOVERY ?? "true") !== "false",
    countResonanceAsTransmission: false,

    // SSE.
    sseHeartbeatMs: num("FL_SSE_HEARTBEAT_MS", 15_000),
    sseReplayWindowMs: num("FL_SSE_REPLAY_WINDOW_MS", 300_000),
    sseMaxEventsPerMinute: num("FL_SSE_MAX_EVENTS_PER_MINUTE", 30),

    // Clock skew tolerated on signed requests.
    clockSkewMs: num("FL_CLOCK_SKEW_MS", 120_000),

    publicBaseUrl: env.FL_PUBLIC_BASE_URL ?? "http://127.0.0.1:8787",

    ...overrides,
  };
}
