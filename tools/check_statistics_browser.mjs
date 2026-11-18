// Production-build chart smoke test. Uses isolated HTTP fixtures, never a database.
// CHROME_EXECUTABLE=/path/to/chrome WEKEEPER_CHART_ORIGIN=http://127.0.0.1:4173 node tools/check_statistics_browser.mjs
import { spawn } from "node:child_process";
import { mkdtemp, readFile, writeFile, mkdir, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { once } from "node:events";
import assert from "node:assert/strict";
const executable = process.env.CHROME_EXECUTABLE;
assert(executable, "Set CHROME_EXECUTABLE to an installed Chromium binary");
const origin = process.env.WEKEEPER_CHART_ORIGIN ?? "http://127.0.0.1:4173";
const output =
  process.env.WEKEEPER_CHART_OUTPUT ?? "/tmp/wekeeper-echarts-browser";
await mkdir(output, { recursive: true });
const profile = await mkdtemp(join(tmpdir(), "wekeeper-chart-browser-"));
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
const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
async function until(read, message) {
  const deadline = Date.now() + 30000;
  while (Date.now() < deadline) {
    const value = await read();
    if (value) return value;
    await delay(80);
  }
  throw new Error(message);
}
let socket;
try {
  let stderr = "";
  browser.stderr.on("data", (data) => {
    stderr += data;
  });
  const endpoint = await until(
    () => stderr.match(/DevTools listening on (ws:\/\/[^\s]+)/)?.[1],
    "Browser failed to start",
  );
  const tabs = await (
    await fetch(`http://${new URL(endpoint).host}/json/list`)
  ).json();
  socket = new WebSocket(
    tabs.find((tab) => tab.type === "page").webSocketDebuggerUrl,
  );
  await once(socket, "open");
  let sequence = 0;
  const pending = new Map();
  const listeners = new Map();
  const errors = [];
  socket.addEventListener("message", (event) => {
    const message = JSON.parse(event.data);
    if (message.id) {
      const task = pending.get(message.id);
      pending.delete(message.id);
      if (message.error) task.reject(new Error(JSON.stringify(message.error)));
      else task.resolve(message.result);
    } else {
      if (message.method === "Runtime.exceptionThrown")
        errors.push(message.params.exceptionDetails);
      listeners.get(message.method)?.(message.params);
    }
  });
  function command(method, params = {}) {
    return new Promise((resolve, reject) => {
      const id = ++sequence;
      pending.set(id, { resolve, reject });
      socket.send(JSON.stringify({ id, method, params }));
    });
  }
  async function evaluate(expression) {
    const reply = await command("Runtime.evaluate", {
      expression,
      returnByValue: true,
      awaitPromise: true,
      userGesture: true,
    });
    if (reply.exceptionDetails)
      throw new Error(JSON.stringify(reply.exceptionDetails));
    return reply.result.value;
  }
  await command("Runtime.enable");
  await command("Page.enable");
  await command("Fetch.enable", {
    patterns: [{ urlPattern: `${origin}/api/*` }],
  });
  const fixture = JSON.parse(
    await readFile(
      new URL(
        "../api-contract/fixtures/message-statistics.json",
        import.meta.url,
      ),
      "utf8",
    ),
  );
  const counts = {
    total: "100",
    sent: "63",
    failed: "12",
    unknown: "5",
    pending: "20",
  };
  const snapshots = new Map();
  const details = [];
  let snapshotSequence = 0;
  let bucketCount = 31;
  const group = (id, name, i = 0) => ({
    id,
    name,
    platform_name: "测试来源平台",
    received: String(100 + i),
    targets: { ...counts, total: String(100 + i), pending: String(20 + i) },
    responsibilities: {
      total: "300",
      sent: "189",
      failed: "36",
      unknown: "15",
      pending: "60",
    },
  });
  listeners.set("Fetch.requestPaused", async ({ requestId, request }) => {
    try {
      const url = new URL(request.url);
      let type = "message-statistics-queries",
        attributes,
        collection;
      if (url.pathname.endsWith("/auth/session")) {
        type = "sessions";
        attributes = {
          csrf: "chart-test",
          user: {
            id: "chart-admin",
            username: "chart-admin",
            display_name: "图表测试管理员",
            platform_admin: true,
            wecom_authenticated: false,
          },
        };
      } else if (url.pathname.includes("/business-platforms")) {
        type = "business-platforms";
        attributes = { items: [], has_more: false };
      } else if (url.pathname.endsWith("/wecom-assets/status")) {
        type = "wecom-asset-status";
        attributes = {
          server_configured: true,
          configuration_error: null,
          datasets: [],
        };
      } else if (
        url.pathname.includes("/wecom-directory/members") ||
        url.pathname.endsWith("/wecom-assets/groups")
      ) {
        collection = {
          data: [],
          meta: { page: { number: 1, size: 30, has_more: false } },
        };
      } else if (url.pathname.endsWith("/wecom-assets/member-statistics")) {
        const page = Number(url.searchParams.get("page[number]") ?? 1);
        collection = {
          data: [
            {
              type: "member-statistics",
              id: `member-page-${page}`,
              attributes: {
                name: `第${page}页成员`,
                userid: `member-${page}`,
                owned_group_count: page * 2,
                participating_group_count: page * 3,
              },
            },
          ],
          meta: { page: { number: page, size: 30, has_more: page === 1 } },
        };
      } else if (url.pathname.endsWith("/details")) {
        const id = url.pathname.split("/").at(-2);
        assert(
          snapshots.has(id),
          "Drilldown must use an existing query version",
        );
        details.push({
          snapshot: id,
          filter: Object.fromEntries(url.searchParams),
          query: snapshots.get(id).filter,
        });
        type = "message-statistics-details";
        attributes = { snapshot_id: id, items: [], has_more: false };
      } else if (url.pathname.endsWith("/queries")) {
        const filter = JSON.parse(request.postData).data.attributes;
        const channels = [
          group("wecom:wecom", "wecom:wecom"),
          group("mobile:wecom", "mobile:wecom"),
          group("mobile:wechat", "mobile:wechat"),
          group("mobile:unclassified", "mobile:unclassified"),
        ];
        attributes = {
          ...fixture,
          id: `chart-snapshot-${++snapshotSequence}`,
          generated_at: new Date().toISOString(),
          expires_at: new Date(Date.now() + 1800000).toISOString(),
          filter: { ...fixture.filter, ...filter },
          cards: ["day", "week", "month", "year"].map((key, i) => ({
            key,
            count: String(100 * (i + 1)),
            previous: "50",
            start: fixture.filter.start + "T00:00:00+08:00",
            end: fixture.generated_at,
            comparison_start: "2026-10-01T00:00:00+08:00",
            comparison_end: "2026-10-18T10:00:00+08:00",
          })),
          summary: {
            ...fixture.summary,
            received: "300",
            dispatched: "200",
            not_dispatched: "20",
            processing: "50",
            targets: counts,
            responsibilities: {
              total: "300",
              sent: "189",
              failed: "36",
              unknown: "15",
              pending: "60",
            },
            attention: {
              total: "25",
              sent: "0",
              failed: "0",
              unknown: "5",
              pending: "20",
            },
          },
          trend: Array.from({ length: bucketCount }, (_, i) => ({
            start: new Date(Date.UTC(2026, 10, 1) + i * 86400000).toISOString(),
            received: String(i % 7 === 0 ? 0 : i * 2),
            targets: counts,
          })),
          channels: filter.channel
            ? channels.filter(
                (c) => c.id === `${filter.channel}:${filter.client_kind}`,
              )
            : channels,
          units: Array.from({ length: 35 }, (_, i) =>
            group(`unit-${i}`, "很长的同名业务单位用于检查图表标签与布局", i),
          ),
          failures: [
            { object: "message", reason_code: "no_route", count: "20" },
            {
              object: "target",
              reason_code: "device_unavailable",
              count: "17",
            },
          ],
        };
        snapshots.set(attributes.id, attributes);
      } else {
        throw new Error(`Unexpected API request: ${request.url}`);
      }
      await command("Fetch.fulfillRequest", {
        requestId,
        responseCode: 200,
        responseHeaders: [
          { name: "content-type", value: "application/vnd.api+json" },
        ],
        body: Buffer.from(
          JSON.stringify(
            collection ?? {
              data: {
                type,
                id: attributes.id ?? "current",
                attributes: Object.fromEntries(
                  Object.entries(attributes).filter(([key]) => key !== "id"),
                ),
              },
            },
          ),
        ).toString("base64"),
      });
    } catch (error) {
      errors.push(String(error));
      await command("Fetch.failRequest", { requestId, errorReason: "Failed" });
    }
  });
  async function navigate(tab = "overview") {
    await command("Page.navigate", {
      url: `${origin}/admin?view=statistics-overview&stats_tab=${tab}`,
    });
    await until(
      () =>
        evaluate(`document.querySelectorAll('[data-chart] svg').length > 0`),
      "Charts did not render",
    ).catch(async (error) => {
      await writeFile(
        join(output, "failure.json"),
        JSON.stringify(
          { body: await evaluate("document.body.innerText"), errors },
          null,
          2,
        ),
      );
      throw error;
    });
    await until(
      () => evaluate(`!document.body.textContent.includes('正在统计消息')`),
      "Query did not finish",
    );
  }
  const measurements = [];
  for (const width of [375, 768, 1440]) {
    await command("Emulation.setDeviceMetricsOverride", {
      width,
      height: 1000,
      deviceScaleFactor: 1,
      mobile: width < 600,
    });
    await navigate();
    await delay(150);
    assert.equal(
      await evaluate("document.documentElement.scrollWidth <= innerWidth"),
      true,
      `Page overflow at ${width}px`,
    );
    assert.equal(
      await evaluate("document.body.textContent.includes('图表暂不可用')"),
      false,
    );
    const screenshot = await command("Page.captureScreenshot", {
      format: "png",
      captureBeyondViewport: true,
    });
    await writeFile(
      join(output, `overview-${width}.png`),
      Buffer.from(screenshot.data, "base64"),
    );
  }
  const chartWidth=await evaluate(`document.querySelector('[data-chart] svg').clientWidth`);
  await evaluate(`document.querySelector('button[aria-label="收起侧栏"]').click()`);
  await until(()=>evaluate(`document.querySelector('[data-chart] svg').clientWidth > ${chartWidth} && Math.abs(document.querySelector('[data-chart] svg').clientWidth-document.querySelector('[data-chart]').clientWidth)<2`),'Chart did not resize after sidebar collapse');
  await navigate("sending");
  await evaluate(
    `Array.from(document.querySelectorAll('details')).find(d=>d.querySelector('summary')?.textContent.startsWith('查看数据表')).open = true`,
  );
  await until(
    () => evaluate(`document.querySelector('details table button') !== null`),
    "Data table did not expand",
  );
  await evaluate(
    `Array.from(document.querySelectorAll('details table button')).find(b=>b.textContent==='63').click()`,
  );
  await until(() => details.length > 0, "Channel result did not drill down");
  assert.equal(details.at(-1).query.channel, "wecom");
  assert.equal(details.at(-1).filter.outcome, "sent");
  await navigate("units");
  assert.equal(
    await evaluate("document.body.textContent.includes('前 10 / 共 35')"),
    true,
  );
  await navigate("failures");
  assert.equal(
    await evaluate(
      "document.body.textContent.includes('未下发消息原因（条）') && document.body.textContent.includes('失败与未知目标原因（项）')",
    ),
    true,
  );
  await navigate("receiving");
  const lifecycle = await evaluate(`(async()=>{
    const {getInstanceByDom}=await import(Array.from(performance.getEntriesByType('resource')).find(r=>/\\/echarts-[^/]+\\.js/.test(r.name)).name);
    let disposed=0;
    const live=()=>Array.from(document.querySelectorAll('[data-chart]')).map(getInstanceByDom).filter(Boolean);
    const wait=async(check)=>{ const deadline=performance.now()+5000; while(!check()){ if(performance.now()>deadline)throw new Error('Chart transition did not settle'); await new Promise(resolve=>setTimeout(resolve,20)); } };
    for(let i=0;i<20;i++){
      await wait(()=>live().length>0);
      const old=live();
      const title=i%2===0?'失败与核对':'接收趋势';
      Array.from(document.querySelectorAll('[role="tab"]')).find(t=>t.textContent===title).click();
      await wait(()=>old.every(chart=>chart.isDisposed()));
      await wait(()=>live().length>0);
      disposed+=old.length;
    }
    return {cycles:20,disposed};
  })()`);
  // Measure chart updates with the largest allowed daily bucket count. Network/SQL are excluded.
  bucketCount = 3660;
  await navigate("receiving");
  await until(
    () =>
      evaluate(
        `document.querySelector('[data-chart]')?.getAttribute('_echarts_instance_')`,
      ),
    "Missing chart instance",
  );
  const performanceResult = await evaluate(`(async()=>{
    const {getInstanceByDom}=await import(Array.from(performance.getEntriesByType('resource')).find(r=>/\\/echarts-[^/]+\\.js/.test(r.name)).name);
    const host=document.querySelector('[data-chart]');
    const id=host.getAttribute('_echarts_instance_');
    // Read the existing renderer instance without creating another one.
    const chart=getInstanceByDom(host); const option=chart.getOption(); const times=[];
    for(let i=0;i<20;i++){ const begin=performance.now(); chart.setOption(option,{replaceMerge:['series']}); await new Promise(requestAnimationFrame); times.push(performance.now()-begin); }
    times.sort((a,b)=>a-b); return {buckets:3660,runs:20,p95:times[18],instanceId:id};
  })()`);
  measurements.push(performanceResult);
  await command("Page.navigate", { url: `${origin}/admin?view=wecom-assets` });
  await until(
    () =>
      evaluate(
        `Array.from(document.querySelectorAll('[role="tab"]')).some(t=>t.textContent==='运营成员统计')`,
      ),
    "Assets did not load",
  );
  await evaluate(
    `Array.from(document.querySelectorAll('[role="tab"]')).find(t=>t.textContent==='运营成员统计').click()`,
  );
  await until(
    () =>
      evaluate(
        `document.querySelector('[data-chart] svg') && document.body.textContent.includes('第1页成员')`,
      ),
    "Member chart did not render",
  );
  await evaluate(
    `document.querySelector('button[aria-label="下一页"]').click()`,
  );
  await until(
    () =>
      evaluate(
        `document.querySelector('[data-chart] svg')?.textContent.includes('第2页成员') && document.body.textContent.includes('第2页成员') && !document.body.textContent.includes('第1页成员')`,
      ),
    "Member chart retained the previous page",
  );
  assert.equal(errors.length, 0, JSON.stringify(errors));
  await writeFile(
    join(output, "results.json"),
    JSON.stringify(
      {
        browser: await command("Browser.getVersion"),
        widths: [375, 768, 1440],
        measurements,
        lifecycle,
        details,
        errors,
      },
      null,
      2,
    ),
  );
  console.log(
    JSON.stringify({
      output,
      measurements,
      lifecycle,
      drilldowns: details.length,
      errors: errors.length,
    }),
  );
} finally {
  socket?.close();
  const closed = once(browser, "exit");
  browser.kill("SIGTERM");
  await closed;
  await rm(profile, { recursive: true, force: true });
}
