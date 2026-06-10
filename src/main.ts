import { mount } from "svelte";
import App from "./App.svelte";
import Catalog from "./lib/ui/Catalog.svelte";

// `?catalog` mounts the visual-language catalog instead of the app.
// It's a standalone dev view (its global atoms.css never loads in app mode).
const isCatalog = new URLSearchParams(window.location.search).has("catalog");

const app = mount(isCatalog ? Catalog : App, {
  target: document.getElementById("app")!,
});

export default app;
