import { describe, it, expect } from "vitest";
import {
  apiUrl,
  encodeQueryComponent,
  messagesUrl,
  mailSearchUrl,
  auditUrl,
  httpErrorMessage,
  jmapRequestBody,
  parseQuota
} from "../src/lib/api/pure";

describe("pure helpers", () => {
  it("apiUrl joins base and path", () => {
    expect(apiUrl("/domains")).toBe("/api/v1/domains");
    expect(apiUrl("domains")).toBe("/api/v1/domains");
    expect(apiUrl("/mail/folders")).toBe("/api/v1/mail/folders");
  });

  it("encodeQueryComponent", () => {
    expect(encodeQueryComponent("abc-XY_09.~")).toBe("abc-XY_09.~");
    expect(encodeQueryComponent("a b")).toBe("a%20b");
    expect(encodeQueryComponent("a&b=c")).toBe("a%26b%3Dc");
    expect(encodeQueryComponent("caf\u00e9")).toBe("caf%C3%A9");
  });

  it("messagesUrl", () => {
    expect(messagesUrl()).toBe("/api/v1/mail/messages");
    expect(messagesUrl("INBOX")).toBe("/api/v1/mail/messages?folder=INBOX");
    expect(messagesUrl(undefined, 20)).toBe("/api/v1/mail/messages?limit=20");
    expect(messagesUrl("Sent Items", 5)).toBe("/api/v1/mail/messages?folder=Sent%20Items&limit=5");
  });

  it("mailSearchUrl", () => {
    expect(mailSearchUrl("hello & bye")).toBe("/api/v1/mail/search?q=hello%20%26%20bye");
  });

  it("auditUrl", () => {
    expect(auditUrl()).toBe("/api/v1/admin/audit");
    expect(auditUrl(10)).toBe("/api/v1/admin/audit?limit=10");
  });

  it("httpErrorMessage prefers detail", () => {
    const body = '{"type":"about:blank","title":"Unprocessable Entity","status":422,"detail":"validation error: bad domain"}';
    expect(httpErrorMessage(422, body)).toBe("validation error: bad domain (HTTP 422)");
  });

  it("httpErrorMessage fallback", () => {
    expect(httpErrorMessage(500, "")).toBe("HTTP 500");
    expect(httpErrorMessage(404, "not here")).toBe("HTTP 404: not here");
  });

  it("jmapRequestBody wraps arrays", () => {
    const calls = [["Mailbox/query", { accountId: "a" }, "t1"]];
    const body: any = jmapRequestBody(calls);
    expect(body.using[0]).toBe("urn:ietf:params:jmap:core");
    expect(body.method_calls).toEqual(calls);
  });

  it("jmapRequestBody passes envelopes", () => {
    const env = { using: ["x"], method_calls: [] };
    expect(jmapRequestBody(env)).toEqual(env);
  });

  it("parseQuota bare array", () => {
    const body = '[{"email":"a@example.com","quota_mb":1024,"used_mb":100,"pct":9.7},{"email":"b@example.com","quota_mb":2048,"used_mb":512,"pct":25.0}]';
    const q = parseQuota(body);
    expect(q.users.length).toBe(2);
    expect(q.quota_mb).toBe(3072);
    expect(q.used_mb).toBe(612);
    expect(q.users[0].email).toBe("a@example.com");
  });

  it("parseQuota wrapped object", () => {
    const body = '{"quota_mb":3072,"used_mb":612,"users":[{"email":"a@example.com","quota_mb":1024,"used_mb":100,"pct":9.7}]}';
    const q = parseQuota(body);
    expect(q.quota_mb).toBe(3072);
    expect(q.used_mb).toBe(612);
    expect(q.users.length).toBe(1);
  });

  it("parseQuota rejects garbage", () => {
    expect(() => parseQuota("not json")).toThrow();
    expect(() => parseQuota("42")).toThrow();
  });
});
