// SPDX-License-Identifier: GPL-3.0-or-later
import { mount } from "svelte";
import App from "./App.svelte";
import "./app.css";

const target = document.getElementById("app");
if (!target) throw new Error("#app is missing");

export default mount(App, { target });
