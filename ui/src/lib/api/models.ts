export interface LoginResp {
  token?: string;
  email?: string;
  kind?: string;
}

export interface MeResp {
  id?: string;
  email?: string;
  kind?: string;
  quota_mb?: number;
  force_password_change?: boolean;
}

export interface User {
  id?: string;
  email?: string;
  kind?: string;
  quota_mb?: number;
  used_mb?: number;
  active?: boolean;
  force_password_change?: boolean;
  created_at?: string;
  last_login_at?: string | null;
}

export interface Domain {
  id?: string;
  domain?: string;
  description?: string | null;
  catch_all?: boolean;
  dkim_selector?: string | null;
  dkim_enabled?: boolean;
  spf_value?: string | null;
  dmarc_policy?: string;
  active?: boolean;
  created_at?: string;
}

export interface Alias {
  id?: string;
  address?: string;
  goto?: string;
  active?: boolean;
  created_at?: string;
}

export interface QuotaUser {
  email: string;
  quota_mb: number;
  used_mb: number;
  pct?: number;
}

export interface QuotaResp {
  quota_mb: number;
  used_mb: number;
  users: QuotaUser[];
}

export interface QueueCounts {
  deferred?: number;
  active?: number;
  hold?: number;
}

export interface QueueResp {
  queue?: QueueCounts;
  activity_1h?: number;
  postfix_queue_size?: number;
}

export interface DkimGenResp {
  domain?: string;
  selector?: string;
  key_type?: string;
  public_key?: string;
  dns_record?: string;
  full_dns?: string;
  keytable_entry?: string;
  synced_keys?: unknown[];
  opendkim_note?: string;
}

export interface DkimKey {
  id?: string;
  selector?: string;
  public_key?: string;
  dns_value?: string;
  revoked?: boolean;
}

export interface DmarcRow {
  id?: string;
  domain?: string;
  report_id?: string;
  report_type?: string;
  created_at?: string;
}

export interface TokenRow {
  id?: string;
  name?: string | null;
  prefix?: string | null;
  scopes?: string[];
  expires_at?: string | null;
  revoked?: boolean;
  created_at?: string;
}

export interface CreatedToken {
  id?: string;
  token?: string;
  prefix?: string;
  scopes?: string[];
  expires_at?: string | null;
}

export interface PushSub {
  id?: string;
  endpoint?: string;
}

export interface TotpEnrollResp {
  secret?: string;
  otpauth_url?: string;
}

export interface MessageHeader {
  uid: number;
  subject?: string;
  from?: string;
  date?: string;
  preview?: string;
  seen?: boolean;
  message_id?: string | null;
}

export interface FoldersResp {
  folders: string[];
}

export interface MessagesResp {
  messages: MessageHeader[];
}

export interface MessageFull {
  uid?: number;
  subject?: string;
  from?: string;
  date?: string;
  seen?: boolean;
  message_id?: string | null;
  raw?: string;
}

export interface SearchResult {
  subject?: string;
  folder?: string;
}

export interface SearchResp {
  query?: string;
  results: SearchResult[];
}

export interface DraftsResp {
  drafts: MessageHeader[];
}

export interface SendResp {
  status?: string;
}

export interface CreateDomainReq {
  domain: string;
  description?: string | null;
  catch_all?: boolean | null;
}

export interface UpdateDomainReq {
  description?: string | null;
  catch_all?: boolean | null;
  dkim_enabled?: boolean | null;
  spf_value?: string | null;
  dmarc_policy?: string | null;
  active?: boolean | null;
}

export interface CreateUserReq {
  email: string;
  password: string;
  kind?: string | null;
  quota_mb?: number | null;
}

export interface UpdateUserReq {
  kind?: string | null;
  quota_mb?: number | null;
  active?: boolean | null;
}

export interface CreateAliasReq {
  address: string;
  goto: string;
}

export interface UpdateAliasReq {
  goto?: string | null;
  active?: boolean | null;
}

export interface DkimGenReq {
  key_type?: string | null;
  selector?: string | null;
}

export interface PushSubReq {
  endpoint: string;
  p256dh: string;
  auth: string;
}

export interface CreateTokenReq {
  name?: string | null;
  scopes?: string[] | null;
  expires_in_days?: number | null;
}

export interface SendMailReq {
  from: string;
  to: string;
  subject: string;
  body: string;
}

