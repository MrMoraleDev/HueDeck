// SPDX-License-Identifier: GPL-3.0-or-later
// Test Action: what the policy refuses, logged by class.
export function Action() {
  this.Name = "Policy";
  this.Description = "Tries refused calls";
  this.Options = [];
  this.onExecute = function () {
    const tryIt = (what, f) => {
      try {
        f();
        console.log(`${what}: ok`);
      } catch (e) {
        console.log(`${what}: ${e.name}`);
      }
    };
    tryIt("launch", () => system.openApplication("notepad.exe", "a b"));
    tryIt("file", () => system.openUrl("file:///C:/Windows/System32/cmd.exe"));
    tryIt("web", () => system.openUrl("https://example.com"));
    tryIt("badkey", () => keys.press("Ctrl + Nope"));
    tryIt("globals", () => {
      if (typeof setTimeout !== "undefined" || typeof window !== "undefined") throw new Error("present");
    });
    system.action("signalrgb://layout/apply/Desk");
    system.prompt({ title: "Name?", message: "Type it" }, (text) => {
      if (text) system.copyToClipboard(`got ${text}`);
      else console.log("prompt cancelled");
    });
  };
}
