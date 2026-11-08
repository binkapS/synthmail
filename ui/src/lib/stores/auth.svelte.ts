import { getToken, setToken, clearToken } from "../api";

export const auth = $state<{ token: string | null }>({ token: getToken() });

export function setAuthToken(token: string | null): void {
  auth.token = token;
  if (token) setToken(token);
  else clearToken();
}

export function clearAuth(): void {
  auth.token = null;
  clearToken();
}

export function isAuthenticated(): boolean {
  return !!auth.token;
}
