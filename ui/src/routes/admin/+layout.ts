import { redirect } from "@sveltejs/kit";
import * as Api from "$lib/api";

export async function load() {
  const isBrowser = typeof window !== "undefined";
  if (isBrowser) {
    if (!Api.getToken()) throw redirect(302, "/login");
    try {
      const me = await Api.me();
      if (me.kind !== "admin" && me.kind !== "superadmin") {
        throw redirect(302, "/mail");
      }
    } catch (e) {
      throw redirect(302, "/login");
    }
  }
  return {};
}
