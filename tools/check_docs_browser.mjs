// Run against the deployed Caddy origin, using an installed Chromium.
// CHROME_EXECUTABLE=/path/to/chrome WEKEEPER_SMOKE_ORIGIN=http://127.0.0.1:8088 node tools/check_docs_browser.mjs
import { spawn } from "node:child_process";
import { mkdtemp, rm, mkdir, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import assert from "node:assert/strict";
import { once } from "node:events";

const origin = process.env.WEKEEPER_SMOKE_ORIGIN || "http://127.0.0.1:8088";
const executable = process.env.CHROME_EXECUTABLE;
if (!executable)
  throw new Error("Set CHROME_EXECUTABLE to an installed Chromium binary");
const profile = await mkdtemp(join(tmpdir(), "wekeeper-docs-browser-"));
const browser = spawn(
  executable,
  [
    "--headless",
    "--no-sandbox",
    "--disable-gpu",
    "--remote-debugging-port=0",
    `--user-data-dir=${profile}`,
    "about:blank",
  ],
  { stdio: ["ignore", "ignore", "pipe"] },
);
let socket;
const errors = [];
const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
async function until(read, message) {
  const end = Date.now() + 20000;
  while (Date.now() < end) {
    const value = await read();
    if (value) return value;
    await delay(100);
  }
  throw new Error(message);
}
try {
  let stderr = "";
  browser.stderr.on("data", (chunk) => {
    stderr += chunk;
  });
  const endpoint = await until(
    () => stderr.match(/DevTools listening on (ws:\/\/[^\s]+)/)?.[1],
    "Chromium did not start",
  );
  const address = new URL(endpoint);
  const tabs = await (await fetch(`http://${address.host}/json/list`)).json();
  socket = new WebSocket(
    tabs.find((tab) => tab.type === "page").webSocketDebuggerUrl,
  );
  await once(socket, "open");
  let id = 0;
  const pending = new Map();
  const listeners = new Map();
  socket.addEventListener("message", (event) => {
    const value = JSON.parse(event.data);
    if (value.id) {
      const promise = pending.get(value.id);
      pending.delete(value.id);
      if (value.error) promise?.reject(new Error(JSON.stringify(value.error)));
      else promise?.resolve(value.result);
    } else {
      if (value.method === "Runtime.exceptionThrown")
        errors.push(JSON.stringify(value.params.exceptionDetails));
      if (
        value.method === "Log.entryAdded" &&
        value.params.entry.level === "error"
      )
        errors.push(value.params.entry.text);
      listeners.get(value.method)?.(value.params);
    }
  });
  function command(method, params = {}) {
    return new Promise((resolve, reject) => {
      const requestId = ++id;
      pending.set(requestId, { resolve, reject });
      socket.send(JSON.stringify({ id: requestId, method, params }));
    });
  }
  async function evaluate(expression) {
    const r = await command("Runtime.evaluate", {
      expression,
      returnByValue: true,
      awaitPromise: true,
      userGesture: true,
    });
    if (r.exceptionDetails) throw new Error(JSON.stringify(r.exceptionDetails));
    return r.result.value;
  }
  async function navigate(path) {
    await command("Page.navigate", { url: origin + path });
    await until(
      () =>
        evaluate(
          `location.pathname === ${JSON.stringify(path)} && document.readyState === 'complete'`,
        ),
      `Navigation failed: ${path}`,
    );
  }
  async function screenshot(name) {
    const folder = process.env.WEKEEPER_GUIDE_SCREENSHOTS;
    if (!folder) return;
    await mkdir(folder, { recursive: true });
    const r = await command("Page.captureScreenshot", {
      format: "png",
      captureBeyondViewport: false,
    });
    await writeFile(join(folder, name), Buffer.from(r.data, "base64"));
  }
  await command("Page.enable");
  await command("Page.bringToFront");
  await command("Runtime.enable");
  await command("Log.enable");
  await command("Emulation.setDeviceMetricsOverride", {
    width: 1440,
    height: 1000,
    deviceScaleFactor: 1,
    mobile: false,
  });
  await navigate("/docs/");
  assert.equal(
    await evaluate(`document.querySelector('a[href="/admin"]')?.textContent`),
    "返回工作台",
  );
  await until(
    () => evaluate(`!!document.querySelector('.VPNavBarSearch button')`),
    "Search button missing",
  );
  // A click on a hydrated control catches CSP and client initialization regressions.
  await evaluate(`document.querySelector('.VPNavBarSearch button').click()`);
  await until(
    () => evaluate(`!!document.querySelector('input[type="search"]')`),
    "Local search did not open",
  );
  await evaluate(`document.querySelector('input[type="search"]').focus()`);
  await command("Input.insertText", { text: "客户群" });
  await until(
    () =>
      evaluate(
        `document.querySelectorAll('.VPLocalSearchBox a[href^="/docs/"]').length > 0`,
      ),
    "Local search has no /docs/ results",
  );
  await command("Input.dispatchKeyEvent", {
    type: "keyDown",
    key: "Escape",
    code: "Escape",
    windowsVirtualKeyCode: 27,
  });
  await navigate("/docs/development/docs-publishing");
  await until(
    () =>
      evaluate(
        `!!document.querySelector('.vp-code-group, div[class*="language-"]')`,
      ),
    "Code block missing",
  );
  await command("Browser.grantPermissions", {
    origin,
    permissions: ["clipboardReadWrite", "clipboardSanitizedWrite"],
  });
  await evaluate(`document.querySelector('button.copy').click()`);
  await until(
    () =>
      evaluate(
        `document.querySelector('button.copy')?.classList.contains('copied')`,
      ),
    "Code copy did not work",
  );
  assert.match(await evaluate("navigator.clipboard.readText()"), /make docs/);
  const dark = await evaluate(
    `document.documentElement.classList.contains('dark')`,
  );
  await evaluate(`document.querySelector('.VPSwitchAppearance').click()`);
  await until(
    () =>
      evaluate(
        `document.documentElement.classList.contains('dark') !== ${dark}`,
      ),
    "Theme switch did not work",
  );
  await command("Emulation.setDeviceMetricsOverride", {
    width: 390,
    height: 844,
    deviceScaleFactor: 1,
    mobile: true,
  });
  await evaluate(`document.querySelector('.VPNavBarHamburger').click()`);
  await until(
    () => evaluate(`!!document.querySelector('.VPNavScreen')`),
    "Mobile navigation did not open",
  );
  await command("Emulation.setDeviceMetricsOverride", {
    width: 1440,
    height: 1000,
    deviceScaleFactor: 1,
    mobile: false,
  });
  await navigate("/docs/guide/operator/mobile-tasks");
  assert.deepEqual(errors, [], `Browser errors: ${errors.join("\n")}`);
  console.log(
    "Docs browser checks passed: local search, /docs/ deep links, code copy, theme, mobile navigation and CSP.",
  );

  // Optional guide illustrations use the actual Web build with explicit fake data.
  // No requests reach a real business API, and no real credentials are used.
  if (process.env.WEKEEPER_GUIDE_SCREENSHOTS) {
    let authenticated = false;
    listeners.set("Fetch.requestPaused", async ({ requestId, request }) => {
      const path = new URL(request.url).pathname;
      let body = {
        data: [],
        meta: { page: { number: 1, size: 30, has_more: false } },
      };
      let responseCode = 200;
      if (path === "/api/v1/auth/session") {
        responseCode = authenticated ? 200 : 401;
        body = authenticated
          ? {
              data: {
                type: "sessions",
                id: "example",
                attributes: {
                  csrf: "example-only",
                  user: {
                    id: "example",
                    username: "example-admin",
                    display_name: "示例管理员",
                    platform_admin: true,
                    bound_school_id: null,
                    grants: [{ school_id: "example-school", role: "admin" }],
                    wecom_authenticated: false,
                  },
                },
              },
            }
          : {
              errors: [
                { status: "401", code: "unauthorized", title: "请登录" },
              ],
            };
      } else if (path === "/api/v1/schools") {
        body = {
          data: [
            {
              type: "schools",
              id: "example-school",
              attributes: {
                name: "示例学校（教学数据）",
                active: true,
                revision: 1,
                config: {},
              },
            },
          ],
        };
      }
      await command("Fetch.fulfillRequest", {
        requestId,
        responseCode,
        responseHeaders: [
          { name: "Content-Type", value: "application/vnd.api+json" },
        ],
        body: Buffer.from(JSON.stringify(body)).toString("base64"),
      });
    });
    await command("Fetch.enable", { patterns: [{ urlPattern: "*/api/*" }] });
    await navigate("/admin");
    await until(
      () => evaluate(`document.body.innerText.includes('登录工作空间')`),
      "Login did not render",
    );
    await screenshot("login.png");
    authenticated = true;
    await command("Page.navigate", {
      url: origin + "/admin?school=example-school&view=posts",
    });
    await until(
      () =>
        evaluate(
          `document.body.innerText.includes('示例管理员') && document.body.innerText.includes('示例学校（教学数据）')`,
        ),
      "Example school did not render",
    );
    await until(
      () => evaluate(`!!document.querySelector('.ant-table-empty')`),
      "Example list did not render",
    );
    await screenshot("workspace.png");
    console.log(
      "Guide screenshots captured from the real Web build with isolated example data.",
    );
  }
} catch (error) {
  if (errors.length) console.error("Browser errors:", errors);
  throw error;
} finally {
  socket?.close();
  const closed = once(browser, "exit");
  browser.kill("SIGTERM");
  await closed;
  await rm(profile, { recursive: true, force: true });
}
