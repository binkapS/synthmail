// Token storage

let memToken: string | null = null;

function getLocalStorage(): Storage | null {
  if (typeof window === "undefined") return null;
  try {
    return window.localStorage;
  } catch {
    return null;
  }
}

export function getToken(): string | null {
  if (memToken) return memToken;
  const storage = getLocalStorage();
  if (!storage) return null;
  try {
    const t = storage.getItem("synthmail_token");
    if (t) {
      memToken = t;
      return t;
    }
  } catch {
    /* noop */
  }
  return null;
}

export function setToken(token: string): void {
  memToken = token;
  const storage = getLocalStorage();
  if (storage) {
    try {
      storage.setItem("synthmail_token", token);
    } catch {
      /* noop */
    }
  }
}

export function clearToken(): void {
  memToken = null;
  const storage = getLocalStorage();
  if (storage) {
    try {
      storage.removeItem("synthmail_token");
    } catch {
      /* noop */
    }
  }
}

export function hasToken(): boolean {
  return !!getToken();
}

export const API_BASE = "/api/v1";
export const TOKEN_KEY = "synthmail_token";

import { apiUrl, httpErrorMessage } from "./pure";

export async function requestText(method: string, url: string, body?: unknown): Promise<string> {
  const init: RequestInit = {
    method,
    headers: {
      Accept: "application/json"
    }
  };
  const token = getToken();
  if (token) (init.headers as Record<string, string>)["Authorization"] = `Bearer ${token}`;
  if (body !== undefined) {
    (init.headers as Record<string, string>)["Content-Type"] = "application/json";
    init.body = JSON.stringify(body);
  }
  const resp = await fetch(url, init);
  const text = await resp.text();
  if (resp.ok) return text;
  throw new Error(httpErrorMessage(resp.status, text));
}

export async function requestJson<T>(method: string, url: string, body?: unknown): Promise<T> {
  const text = await requestText(method, url, body);
  const trimmed = text.trim();
  if (!trimmed) throw new Error("Empty response body");
  return JSON.parse(trimmed) as T;
}

export { apiUrl, httpErrorMessage };

// re-export pure helpers for convenience
export * from "./pure";
