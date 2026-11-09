import { clearToken, getToken, requestJson, requestText, setToken } from "./client";
import { apiUrl, auditUrl, encodeQueryComponent, mailSearchUrl, messagesUrl, parseQuota, jmapRequestBody } from "./pure";
import type {
  Alias,
  CreateAliasReq,
  CreateDomainReq,
  CreatedToken,
  CreateTokenReq,
  CreateUserReq,
  DkimGenReq,
  DkimGenResp,
  DkimKey,
  DmarcRow,
  Domain,
  DraftsResp,
  FoldersResp,
  LoginResp,
  MeResp,
  MessageFull,
  MessagesResp,
  PushSub,
  PushSubReq,
  QuotaResp,
  SearchResp,
  SendMailReq,
  SendResp,
  TokenRow,
  TotpEnrollResp,
  UpdateAliasReq,
  UpdateDomainReq,
  UpdateUserReq,
  User
} from "./models";

// ----- Auth -----
export async function login(email: string, password: string, totp_code?: string): Promise<LoginResp> {
  const body: Record<string, unknown> = { email, password };
  if (totp_code && totp_code.trim()) body.totp_code = totp_code.trim();
  const resp = await requestJson<LoginResp>("POST", apiUrl("/auth/login"), body);
  if (resp.token) setToken(resp.token);
  return resp;
}

export async function logout(): Promise<unknown> {
  const resp = await requestJson<unknown>("POST", apiUrl("/auth/logout"));
  clearToken();
  return resp;
}

export async function me(): Promise<MeResp> {
  return requestJson<MeResp>("GET", apiUrl("/auth/me"));
}

export async function changePassword(current_password: string, new_password: string): Promise<unknown> {
  return requestJson("POST", apiUrl("/auth/password/change"), { current_password, new_password });
}

// ----- Domains -----
export async function listDomains(): Promise<Domain[]> {
  return requestJson<Domain[]>("GET", apiUrl("/domains"));
}

export async function getDomain(id: string): Promise<Domain> {
  return requestJson<Domain>("GET", apiUrl(`/domains/${id}`));
}

export async function createDomain(req: CreateDomainReq): Promise<Domain> {
  return requestJson<Domain>("POST", apiUrl("/domains"), req);
}

export async function updateDomain(id: string, req: UpdateDomainReq): Promise<Domain> {
  return requestJson<Domain>("PUT", apiUrl(`/domains/${id}`), req);
}

export async function deleteDomain(id: string): Promise<unknown> {
  return requestJson("DELETE", apiUrl(`/domains/${id}`));
}

export async function domainDns(id: string): Promise<unknown> {
  return requestJson("GET", apiUrl(`/domains/${id}/dns`));
}

export async function domainVerify(id: string): Promise<unknown> {
  return requestJson("GET", apiUrl(`/domains/${id}/verify`));
}

// ----- Users -----
export async function listUsers(domain?: string): Promise<User[]> {
  let url = apiUrl("/users");
  if (domain) url += `?domain=${encodeQueryComponent(domain)}`;
  return requestJson<User[]>("GET", url);
}

export async function getUser(id: string): Promise<User> {
  return requestJson<User>("GET", apiUrl(`/users/${id}`));
}

export async function createUser(req: CreateUserReq): Promise<User> {
  return requestJson<User>("POST", apiUrl("/users"), req);
}

export async function updateUser(id: string, req: UpdateUserReq): Promise<User> {
  return requestJson<User>("PUT", apiUrl(`/users/${id}`), req);
}

export async function deleteUser(id: string): Promise<unknown> {
  return requestJson("DELETE", apiUrl(`/users/${id}`));
}

export async function resetPassword(id: string, password: string): Promise<unknown> {
  return requestJson("POST", apiUrl(`/users/${id}/password`), { password });
}

// ----- Aliases -----
export async function listAliases(): Promise<Alias[]> {
  return requestJson<Alias[]>("GET", apiUrl("/aliases"));
}

export async function getAlias(id: string): Promise<Alias> {
  return requestJson<Alias>("GET", apiUrl(`/aliases/${id}`));
}

export async function createAlias(req: CreateAliasReq): Promise<Alias> {
  return requestJson<Alias>("POST", apiUrl("/aliases"), req);
}

export async function updateAlias(id: string, req: UpdateAliasReq): Promise<Alias> {
  return requestJson<Alias>("PUT", apiUrl(`/aliases/${id}`), req);
}

export async function deleteAlias(id: string): Promise<unknown> {
  return requestJson("DELETE", apiUrl(`/aliases/${id}`));
}

// ----- Quota / Queues -----
export async function quota(): Promise<QuotaResp> {
  const text = await requestText("GET", apiUrl("/quota"));
  return parseQuota(text);
}

export async function quotaUsers(): Promise<QuotaResp["users"]> {
  return (await quota()).users;
}

export async function queues(): Promise<unknown> {
  return requestJson("GET", apiUrl("/queues"));
}

// ----- DKIM -----
export async function dkimGenerate(domain_id: string, req: DkimGenReq): Promise<DkimGenResp> {
  return requestJson<DkimGenResp>("POST", apiUrl(`/dkim/${domain_id}/generate`), req);
}

export async function dkimList(domain_id: string): Promise<DkimKey[]> {
  return requestJson<DkimKey[]>("GET", apiUrl(`/dkim/${domain_id}`));
}

// ----- Filters / Vacation -----
export async function getFilters(user_id: string): Promise<unknown> {
  return requestJson("GET", apiUrl(`/filters/${user_id}`));
}

export async function saveFilters(user_id: string, body: unknown): Promise<unknown> {
  return requestJson("PUT", apiUrl(`/filters/${user_id}`), body);
}

export async function getVacation(user_id: string): Promise<unknown> {
  return requestJson("GET", apiUrl(`/vacation/${user_id}`));
}

export async function saveVacation(user_id: string, body: unknown): Promise<unknown> {
  return requestJson("PUT", apiUrl(`/vacation/${user_id}`), body);
}

// ----- Admin -----
export async function serverStats(): Promise<unknown> {
  return requestJson("GET", apiUrl("/admin/server"));
}

export async function listAudit(limit?: number): Promise<unknown[]> {
  const url = auditUrl(limit ?? null);
  return requestJson<unknown[]>("GET", url);
}

export async function listBans(): Promise<unknown[]> {
  return requestJson<unknown[]>("GET", apiUrl("/admin/bans"));
}

export async function exportConfigValue(): Promise<unknown> {
  return requestJson("GET", apiUrl("/config/export"));
}

export async function exportConfig(): Promise<string> {
  const v = await exportConfigValue();
  return JSON.stringify(v, null, 2);
}

export async function importConfig(body: unknown): Promise<unknown> {
  return requestJson("POST", apiUrl("/config/import"), body);
}

// ----- DMARC -----
export async function listDmarc(domain?: string): Promise<DmarcRow[]> {
  let url = apiUrl("/dmarc/reports");
  if (domain) url += `?domain=${encodeQueryComponent(domain)}`;
  return requestJson<DmarcRow[]>("GET", url);
}

export async function createDmarcReport(body: unknown): Promise<unknown> {
  return requestJson("POST", apiUrl("/dmarc/reports"), body);
}

// ----- Push -----
export async function listPush(): Promise<PushSub[]> {
  return requestJson<PushSub[]>("GET", apiUrl("/push/subscribe"));
}

export async function createPush(req: PushSubReq): Promise<unknown> {
  return requestJson("POST", apiUrl("/push/subscribe"), req);
}

export async function deletePush(id: string): Promise<unknown> {
  return requestJson("DELETE", apiUrl(`/push/subscribe/${id}`));
}

// ----- TOTP -----
export async function totpEnroll(password: string, code?: string): Promise<TotpEnrollResp> {
  const body: Record<string, unknown> = { password };
  if (code && code.trim()) body.code = code.trim();
  return requestJson<TotpEnrollResp>("POST", apiUrl("/auth/totp/enroll"), body);
}

export async function totpVerify(totp_code: string): Promise<unknown> {
  return requestJson("POST", apiUrl("/auth/totp/verify"), { totp_code });
}

export async function totpDisable(password: string): Promise<unknown> {
  return requestJson("POST", apiUrl("/auth/totp/disable"), { password });
}

// ----- Tokens -----
export async function listApiTokens(): Promise<TokenRow[]> {
  return requestJson<TokenRow[]>("GET", apiUrl("/api-tokens"));
}

export async function createApiToken(req: CreateTokenReq): Promise<CreatedToken> {
  return requestJson<CreatedToken>("POST", apiUrl("/api-tokens"), req);
}

export async function revokeApiToken(id: string): Promise<unknown> {
  return requestJson("DELETE", apiUrl(`/api-tokens/${id}`));
}

// ----- Webmail -----
export async function mailFolders(): Promise<FoldersResp> {
  return requestJson<FoldersResp>("GET", apiUrl("/mail/folders"));
}

export async function mailListMessages(folder?: string, limit?: number): Promise<MessagesResp> {
  return requestJson<MessagesResp>("GET", messagesUrl(folder ?? undefined, limit ?? undefined));
}

export async function mailGetMessage(uid: number): Promise<MessageFull> {
  return requestJson<MessageFull>("GET", apiUrl(`/mail/messages/${uid}`));
}

export async function mailSearch(q: string): Promise<SearchResp> {
  return requestJson<SearchResp>("GET", mailSearchUrl(q));
}

export async function mailSend(req: SendMailReq): Promise<SendResp> {
  return requestJson<SendResp>("POST", apiUrl("/mail/send"), req);
}

export async function mailListDrafts(): Promise<DraftsResp> {
  return requestJson<DraftsResp>("GET", apiUrl("/mail/drafts"));
}

export async function mailDeleteDraft(uid: number): Promise<unknown> {
  return requestJson("POST", apiUrl("/mail/drafts"), { uid });
}

// ----- JMAP -----
export async function jmapSession(): Promise<unknown> {
  return requestJson("GET", "/jmap/session");
}

export async function jmapRequest(methodCalls: unknown): Promise<unknown> {
  const body = jmapRequestBody(methodCalls);
  return requestJson("POST", "/jmap", body);
}

// ----- Compat shims -----
export async function tokensList(): Promise<TokenRow[]> {
  return listApiTokens();
}

export async function tokensCreate(
  name: string,
  scopes: string[],
  expires_days: number
): Promise<CreatedToken> {
  return createApiToken({
    name: name || null,
    scopes: scopes || null,
    expires_in_days: expires_days > 0 ? expires_days : null
  });
}

export async function tokensRevoke(id: string): Promise<unknown> {
  return revokeApiToken(id);
}

export async function pushList(): Promise<PushSub[]> {
  return listPush();
}

export async function pushCreate(endpoint: string, p256dh: string, auth: string): Promise<unknown> {
  return createPush({ endpoint, p256dh, auth });
}

export async function pushDeleteCompat(id: string): Promise<unknown> {
  return deletePush(id);
}

export async function audit(limit?: number): Promise<unknown[]> {
  return listAudit(limit);
}

export async function bans(): Promise<unknown[]> {
  return listBans();
}

export async function filtersGet(user_id: string): Promise<unknown> {
  return getFilters(user_id);
}

export async function filtersPut(user_id: string, sieve: string): Promise<unknown> {
  return saveFilters(user_id, { sieve });
}

export async function vacationGet(user_id: string): Promise<unknown> {
  return getVacation(user_id);
}

export async function vacationPut(user_id: string, body: unknown): Promise<unknown> {
  return saveVacation(user_id, body);
}

export async function configExport(): Promise<string> {
  return exportConfig();
}

export async function configImport(body: unknown): Promise<unknown> {
  return importConfig(body);
}

export async function dmarcList(): Promise<DmarcRow[]> {
  return listDmarc();
}

export async function mailMessages(folder: string, limit: number): Promise<MessagesResp> {
  return mailListMessages(folder, limit);
}

export async function mailMessage(uid: number): Promise<MessageFull> {
  return mailGetMessage(uid);
}

export async function mailDrafts(): Promise<DraftsResp> {
  return mailListDrafts();
}

export async function mailDraftDelete(uid: number): Promise<unknown> {
  return mailDeleteDraft(uid);
}

export { apiUrl, httpErrorMessage, getToken, setToken, clearToken, hasToken, API_BASE, TOKEN_KEY } from "./client";
export { parseQuota, encodeQueryComponent } from "./pure";
export type { QuotaResp } from "./models";
export * from "./models";

export { deletePush as pushDelete };
