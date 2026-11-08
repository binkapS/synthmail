import { i as escape_html, r as getContext } from "../../chunks/server.js";
import "../../chunks/server-errors.js";
// node_modules/@sveltejs/kit/src/runtime/app/state/server.js
function context() {
	return getContext("__request__");
}
var page = {
	get data() {
		return context().page.data;
	},
	get error() {
		return context().page.error;
	},
	get form() {
		return context().page.form;
	},
	get params() {
		return context().page.params;
	},
	get route() {
		return context().page.route;
	},
	get shallow() {
		return context().page.shallow;
	},
	get state() {
		return context().page.state;
	},
	get status() {
		return context().page.status;
	},
	get url() {
		return context().page.url;
	}
};
// src/routes/+error.svelte
function _error($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		$$renderer.push(`<div class="grid place-items-center min-h-screen bg-appBg text-text-primary p-6"><div class="bg-surface border border-divider rounded-2xl p-6 shadow-xl max-w-md w-full"><h1 class="text-xl font-semibold">${escape_html(page.status)}</h1> <p class="text-text-muted mt-2">${escape_html(page.error?.message ?? "Page not found")}</p> <a href="/" class="bg-brand hover:bg-indigo-500 text-text-primary font-medium px-5 py-2.5 rounded-xl shadow-lg shadow-brand/20 active:scale-[0.98] transition-all inline-block mt-4">Home</a></div></div>`);
	});
}
export { _error as default };

//# sourceMappingURL=_error.svelte.js.map