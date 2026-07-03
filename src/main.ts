import { mount } from "svelte";
import App from "./App.svelte";

// `?catalog` mounts the visual-language catalog instead of the app. It is
// lazy-imported so none of its CSS ever enters the app bundle — a static
// import injects the catalog's stylesheet globally even when it never mounts.
const isCatalog = new URLSearchParams(window.location.search).has("catalog");

const target = document.getElementById("app")!;

if (isCatalog) {
  import("./lib/ui/Catalog.svelte").then(({ default: Catalog }) => {
    mount(Catalog, { target });
  });
} else {
  mount(App, { target });
}
