// Entry point of the Svelte build: the same job as `plan.js` has in the
// vanilla page, done by a component tree.
import { mount } from "svelte";

import "./app.css";
import App from "./App.svelte";

const app = mount(App, { target: document.getElementById("app") });

export default app;
