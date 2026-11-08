import "../../chunks/server.js";
// src/routes/+layout.svelte
function _layout($$renderer, $$props) {
	let { children } = $$props;
	$$renderer.push(`<div class="bg-appBg text-text-primary min-h-screen">`);
	children($$renderer);
	$$renderer.push(`<!----></div>`);
}
export { _layout as default };

//# sourceMappingURL=_layout.svelte.js.map