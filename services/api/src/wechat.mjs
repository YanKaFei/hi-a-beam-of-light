// WeChat login (M3-B).
//
// The AppSecret lives only here. Nothing from code2Session is returned to the client:
// no session_key, no openid, no code. The client gets a random business session token.

import { randomBytes, createHash } from "node:crypto";

export const SESSION_TTL_MS = 7 * 24 * 3600 * 1000;

export function productionMode() {
  return process.env.FL_ENV === "production" || process.env.NODE_ENV === "production";
}

/**
 * Guard rails. A production deployment must not be able to fall back to the
 * development adapter or to a fixture authority, even if the environment is wrong.
 */
export function assertProductionIsolation(env = process.env) {
  const problems = [];
  const prod = env.FL_ENV === "production" || env.NODE_ENV === "production";
  if (prod && env.FL_DEV_ADAPTER === "on") problems.push("FL_DEV_ADAPTER must not be enabled in production");
  if (prod && env.FL_TRUSTED_FIXTURE_ADAPTER) problems.push("FL_TRUSTED_FIXTURE_ADAPTER must not be set in production");
  if (prod && !env.FL_WECHAT_APP_ID) problems.push("FL_WECHAT_APP_ID is required in production");
  if (prod && !env.FL_WECHAT_APP_SECRET) problems.push("FL_WECHAT_APP_SECRET is required in production");
  if (problems.length) {
    const err = new Error(`production isolation failed: ${problems.join("; ")}`);
    err.code = "PRODUCTION_ISOLATION";
    throw err;
  }
}

export const tokenHash = (token) => createHash("sha256").update(token).digest("hex");
export const newToken = () => randomBytes(32).toString("base64url");

/** Exchange a wx.login code. Real AppID/AppSecret required; never returns secrets. */
export async function code2Session(code, { appId, appSecret, fetchImpl }) {
  const url = new URL("https://api.weixin.qq.com/sns/jscode2session");
  url.searchParams.set("appid", appId);
  url.searchParams.set("secret", appSecret);
  url.searchParams.set("js_code", code);
  url.searchParams.set("grant_type", "authorization_code");
  const r = await fetchImpl(url.toString());
  const body = await r.json().catch(() => ({}));
  if (body.errcode) {
    const err = new Error(`wechat error ${body.errcode}`);
    err.code = "WECHAT_ERROR";
    err.wechatCode = body.errcode;
    throw err;
  }
  return { openId: body.openid, sessionKey: body.session_key };
}
