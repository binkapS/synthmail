import type { QuotaResp, QuotaUser } from "./models";

export const API_BASE = "/api/v1";
export const TOKEN_KEY = "synthmail_token";

export function apiUrl(path: string): string {
  const p = path.startsWith("/") ? path : `/${path}`;
  return `${API_BASE}${p}`;
}

export function encodeQueryComponent(s: string): string {
  let out = "";
  for (let i = 0; i < s.length; i++) {
    const code = s.charCodeAt(i);
    const c = s[i];
    if (
      (code >= 65 && code <= 90) ||
      (code >= 97 && code <= 122) ||
      (code >= 48 && code <= 57) ||
      c === "-" ||
      c === "_" ||
      c === "." ||
      c === "~"
    ) {
      out += c;
    } else {
      // encode full UTF-8
      const bytes = new TextEncoder().encode(c);
      for (const b of bytes) out += `%${b.toString(16).toUpperCase().padStart(2, "0")}`;
    }
  }
  return out;
}

// NOTE: Rust reference encodes byte-by-byte for multi-byte; TextEncoder approach matches.
export function messagesUrl(folder?: string | null, limit?: number | null): string {
  let url = apiUrl("/mail/messages");
  const qs: string[] = [];
  if (folder) qs.push(`folder=${encodeQueryComponent(folder)}`);
  if (limit !== undefined && limit !== null) qs.push(`limit=${limit}`);
  if (qs.length) url += `?${qs.join("&")}`;
  return url;
}

export function mailSearchUrl(q: string): string {
  return `${apiUrl("/mail/search")}?q=${encodeQueryComponent(q)}`;
}

export function auditUrl(limit?: number | null): string {
  let url = apiUrl("/admin/audit");
  if (limit !== undefined && limit !== null) url += `?limit=${limit}`;
  return url;
}

export function errDetailFromBody(body: string): string | null {
  try {
    const v = JSON.parse(body.trim());
    for (const key of ["detail", "message", "error", "title"]) {
      const s = v?.[key];
      if (typeof s === "string" && s.trim()) return s.trim();
    }
  } catch {
    /* noop */
  }
  return null;
}

export function httpErrorMessage(status: number, body: string): string {
  const detail = errDetailFromBody(body);
  if (detail) return `${detail} (HTTP ${status})`;
  const text = body.trim();
  if (!text) return `HTTP ${status}`;
  const chars = [...text];
  if (chars.length > 300) {
    return `HTTP ${status}: ${chars.slice(0, 300).join("")}…`;
  }
  return `HTTP ${status}: ${text}`;
}

export function jmapRequestBody(methodCalls: unknown): unknown {
  if (
    methodCalls &&
    typeof methodCalls === "object" &&
    !Array.isArray(methodCalls) &&
    "method_calls" in (methodCalls as Record<string, unknown>)
  ) {
    return methodCalls;
  }
  return {
    using: ["urn:ietf:params:jmap:core", "urn:ietf:params:jmap:mail"],
    method_calls: methodCalls
  };
}

export interface QuotaRespParsed extends QuotaResp {
  // alias
}



export function parseQuota(body: string): QuotaResp {
  const v = JSON.parse(body.trim());
  if (Array.isArray(v)) {
    const users: QuotaUser[] = v.map((u) => ({
      email: u.email ?? "",
      quota_mb: Number(u.quota_mb ?? 0),
      used_mb: Number(u.used_mb ?? 0),
      pct: Number(u.pct ?? 0)
    }));
    const quota_mb = users.reduce((a, u) => a + (u.quota_mb || 0), 0);
    const used_mb = users.reduce((a, u) => a + (u.used_mb || 0), 0);
    return { quota_mb, used_mb, users };
  }
  if (v && typeof v === "object") {
    const resp: QuotaResp = {
      quota_mb: Number(v.quota_mb ?? 0),
      used_mb: Number(v.used_mb ?? 0),
      users: Array.isArray(v.users)
        ? v.users.map((u: any) => ({
            email: u.email ?? "",
            quota_mb: Number(u.quota_mb ?? 0),
            used_mb: Number(u.used_mb ?? 0),
            pct: Number(u.pct ?? 0)
          }))
        : []
    };
    if (resp.users.length) {
      if (resp.quota_mb === 0) resp.quota_mb = resp.users.reduce((a, u) => a + (u.quota_mb || 0), 0);
      if (resp.used_mb === 0) resp.used_mb = resp.users.reduce((a, u) => a + (u.used_mb || 0), 0);
    }
    return resp;
  }
  throw new Error("Unexpected quota response shape");
}


