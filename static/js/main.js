let lastBlockCtx = null;

async function getJSON(url) {
  try {
    const r = await fetch(url, { cache: "no-store" });
    if (!r.ok) throw new Error(await r.text());
    return await r.json();
  } catch (e) {
    console.warn("API error:", e);
    return null;
  }
}

function fmtBytes(n) {
  if (n == null) return "—";
  const units = ["B","KB","MB","GB","TB"];
  let i = 0, v = n;
  while (v >= 1024 && i < units.length - 1) { v /= 1024; i++; }
  return `${v.toFixed(v >= 100 ? 0 : v >= 10 ? 1 : 2)} ${units[i]}`;
}
function fmtBool(b) { return b ? "yes" : "no"; }

// -------------------- Mempool --------------------
async function refreshMempool() {
  const m = await getJSON("/api/mempoolinfo");
  if (!m) return;

  setText("mp-size",   m.size ?? "—");
  setText("mp-bytes",  fmtBytes(m.bytes ?? 0));
  setText("mp-usage",  fmtBytes(m.usage ?? 0));
  setText("mp-minfee", (m.mempoolminfee ?? 0).toFixed(2));
  setText("mp-rbf",    fmtBool(m.fullrbf ?? false));
  setText("mp-unb",    m.unbroadcastcount ?? 0);
}

// ----------------- NETWORK SUMMARY -----------------
function fmtNumber(n, digits = 2) {
  if (n == null || !isFinite(n)) return "—";
  return n.toLocaleString(undefined, { maximumFractionDigits: digits });
}
function fmtSecs(s) {
  if (!isFinite(s)) return "—";
  const m = s / 60;
  return m >= 1 ? `${fmtNumber(m, 1)} min` : `${fmtNumber(s, 0)} s`;
}
function fmtCompact(n, digits = 2) {
  if (n == null || !isFinite(n)) return "—";
  const units = ["", "K", "M", "B", "T", "Qa", "Qi"];
  let v = Math.abs(n);
  let u = 0;
  while (v >= 1000 && u < units.length - 1) { v /= 1000; u++; }
  const sign = n < 0 ? "-" : "";
  return `${sign}${v.toFixed(digits)} ${units[u]}`.trim();
}
function fmtHashrateHps(hps) {
  if (hps == null || !isFinite(hps) || hps <= 0) return "—";
  const units = ["H/s", "kH/s", "MH/s", "GH/s", "TH/s", "PH/s", "EH/s", "ZH/s"];
  let v = hps, i = 0;
  while (v >= 1000 && i < units.length - 1) { v /= 1000; i++; }
  const decimals = v >= 100 ? 0 : v >= 10 ? 1 : 2;
  return `${v.toFixed(decimals)} ${units[i]}`;
}

async function refreshNetwork() {
  const d = await getJSON("/api/network");
  if (!d) return;

  document.getElementById("net-height").textContent     = d.height?.toLocaleString() ?? "—";
  document.getElementById("net-difficulty").textContent = fmtCompact(d.difficulty, 2);

  // Hashrate with automatic unit scaling (kH/s → EH/s)
  const ghps = d.hashrate_ghps ?? 0;   // API gives GH/s
  const hps  = ghps * 1e9;             // convert to H/s for unit scaling
  document.getElementById("net-hashrate").textContent = fmtHashrateHps(hps);

  document.getElementById("net-interval").textContent  = fmtSecs(d.avg_block_interval_sec);

  // Adjustment estimate
  document.getElementById("net-nextadj").textContent   = d.blocks_to_next_adjust?.toLocaleString() ?? "—";
  const chg = d.est_diff_change_pct ?? 0;
  const chgEl = document.getElementById("net-diffchg");
  chgEl.textContent = `${chg >= 0 ? "▲" : "▼"} ${Math.abs(chg).toFixed(2)}%`;
  chgEl.style.color = chg >= 0 ? "#2ecc71" : "#ff6b6b";

  // Supply & issuance
  document.getElementById("net-subsidy").textContent = fmtNumber(d.current_subsidy_btc, 8);
  document.getElementById("net-newday").textContent  = fmtNumber(d.est_new_btc_per_day, 2);
  document.getElementById("net-supply").textContent  = fmtNumber(d.est_circulating_btc, 3);
}

// ----------------- Explorer: search + latest blocks -----------------
function isHex64(s) { return typeof s === "string" && /^[0-9a-fA-F]{64}$/.test(s); }
function isDigits(s) { return typeof s === "string" && /^[0-9]+$/.test(s); }

function setText(id, val) {
  const el = document.getElementById(id);
  if (el) el.textContent = val;
  else console.warn(`[mempool] Missing element #${id}`);
}

async function loadLatestBlocks(n = 10) {
  // Use /api/network for tip height, then walk back n heights using /api/blockhash/{height}
  const net = await getJSON("/api/network");
  const list = document.getElementById("blocks-list");
  if (!net || !list) return;

  const tip = net.height ?? 0;
  const items = [];
  const count = Math.max(1, Math.min(n, 20));

  for (let i = 0; i < count; i++) {
    const h = tip - i;
    if (h < 0) break;
    const bh = await getJSON(`/api/blockhash/${h}`);
    if (bh && bh.hash) {
      items.push({ height: h, hash: bh.hash });
    }
  }

  list.innerHTML = items.map(it => `
    <li class="list__item">
      <div class="mono ellip">${it.hash}</div>
      <div class="muted">Height ${it.height.toLocaleString()}</div>
      <button class="btn btn--sm" data-hash="${it.hash}" data-height="${it.height}">Open</button>
    </li>
  `).join("");

  // Hook buttons (block detail to be implemented server-side later)
  list.querySelectorAll("button[data-hash]").forEach(btn => {
    btn.addEventListener("click", () => {
        const hash = btn.getAttribute("data-hash");
        showBlock(hash);
    });
  });
}

function hookSearch() {
  const form = document.getElementById("search-form");
  const input = document.getElementById("search-input");
  const result = document.getElementById("result");
  if (!form || !input || !result) return;

  form.addEventListener("submit", async (e) => {
    e.preventDefault();
    const q = input.value.trim();
    if (!q) return;

    // Height → get hash → show block
    if (isDigits(q)) {
        const h = Number(q);
        const bh = await getJSON(`/api/blockhash/${h}`);
        if (bh && bh.hash) return showBlock(bh.hash);
        document.getElementById("result").textContent = "Block not found.";
        return;
    }

    // 64-hex: try as block first, then tx
    if (isHex64(q)) {
        const tryBlock = await getJSON(`/api/block/${q}`);
        if (tryBlock) return showBlock(q);
        const tryTx = await getJSON(`/api/tx/${q}`);
        if (tryTx) return showTx(q);
        document.getElementById("result").textContent = "No block or tx found for that hash.";
        return;
    }

    document.getElementById("result").textContent = "Please enter a height (digits) or a 64-char hex.";
  });
}

function timeAgo(tsSec) {
  if (!tsSec) return "—";
  const diff = (Date.now()/1000) - tsSec;
  if (diff < 60) return `${Math.floor(diff)} s ago`;
  if (diff < 3600) return `${Math.floor(diff/60)} min ago`;
  if (diff < 86400) return `${Math.floor(diff/3600)} h ago`;
  return `${Math.floor(diff/86400)} d ago`;
}

async function showBlock(hash, offset = 0, limit = 20) {
  const res = await getJSON(`/api/block/${hash}?offset=${offset}&limit=${limit}`);
  const el = document.getElementById("result");
  if (!el) return;
  if (!res) { el.textContent = "Block not found."; return; }

  lastBlockCtx = { hash: res.hash, height: res.height, limit }; // remember page size

  const when = timeAgo(res.time);
  const prevHtml = res.prev
    ? `Prev: <button class="btn btn--sm" data-goto-block="${res.prev}">Open</button> <span class="mono">${res.prev}</span>`
    : `Prev: —`;
  const nextHtml = res.next
    ? `Next: <button class="btn btn--sm" data-goto-block="${res.next}">Open</button> <span class="mono">${res.next}</span>`
    : ``;

  // tx list
  const txListHtml = res.txids.map(t => `
    <li class="list__item">
      <div class="mono ellip">${t}</div>
      <button class="btn btn--sm" data-tx="${t}">Open TX</button>
    </li>
  `).join("");

  // pager
  const from = res.total_tx === 0 ? 0 : (res.offset + 1);
  const to   = Math.min(res.offset + res.limit, res.total_tx);
  const canPrev = res.offset > 0;
  const canNext = (res.offset + res.limit) < res.total_tx;

  const pagerHtml = `
    <div class="sub" style="display:flex;gap:8px;align-items:center;justify-content:space-between;margin-top:8px;">
      <div>Showing ${from.toLocaleString()}–${to.toLocaleString()} of ${res.total_tx.toLocaleString()}</div>
      <div style="display:flex;gap:6px;">
        <button class="btn btn--sm" id="pg-first" ${canPrev ? "" : "disabled"}>« First</button>
        <button class="btn btn--sm" id="pg-prev"  ${canPrev ? "" : "disabled"}>‹ Prev</button>
        <button class="btn btn--sm" id="pg-next"  ${canNext ? "" : "disabled"}>Next ›</button>
        <button class="btn btn--sm" id="pg-last"  ${canNext ? "" : "disabled"}>Last »</button>
      </div>
    </div>
  `;

  // render
  el.innerHTML = `
    <div class="callout">
      <div><strong>Block</strong> <span class="muted">height</span> ${res.height.toLocaleString()}</div>
      <div class="mono">${res.hash}</div>
      <div class="sub" style="margin-top:6px;">${when} • ${fmtNumber(res.size,0)} B • ${fmtNumber(res.weight??0,0)} WU • tx: ${fmtNumber(res.n_tx,0)}</div>
      <div class="sub" style="margin:6px 0;">${prevHtml}</div>
      ${nextHtml ? `<div class="sub">${nextHtml}</div>` : ``}

      <h4 style="margin:10px 0 6px;">Transactions</h4>
      <ul class="list">${txListHtml || `<li class="list__item">(no transactions)</li>`}</ul>
      ${res.total_tx > res.limit ? pagerHtml : ``}
    </div>
  `;
  el.scrollIntoView({ behavior: "smooth", block: "start" });

  // wire tx buttons
  el.querySelectorAll("button[data-tx]").forEach(btn => {
    btn.addEventListener("click", () => showTx(btn.getAttribute("data-tx")));
  });

  // wire block prev/next
  el.querySelectorAll("button[data-goto-block]").forEach(btn => {
    btn.addEventListener("click", () => showBlock(btn.getAttribute("data-goto-block"), 0, lastBlockCtx?.limit ?? 20));
  });

  // wire pager
  const pageSize = res.limit;
  const total = res.total_tx;

  const first = document.getElementById("pg-first");
  const prev  = document.getElementById("pg-prev");
  const next  = document.getElementById("pg-next");
  const last  = document.getElementById("pg-last");

  if (first) first.addEventListener("click", () => showBlock(hash, 0, pageSize));
  if (prev)  prev.addEventListener("click",  () => showBlock(hash, Math.max(0, res.offset - pageSize), pageSize));
  if (next)  next.addEventListener("click",  () => showBlock(hash, res.offset + pageSize, pageSize));
  if (last)  last.addEventListener("click",  () => {
    const remainder = total % pageSize;
    const lastOffset = remainder === 0 ? Math.max(0, total - pageSize) : total - remainder;
    showBlock(hash, lastOffset, pageSize);
  });
}

function satoshi(n) { return `${n.toLocaleString(undefined, { maximumFractionDigits: 8 })} BTC`; }

function hookAddrLookup() {
  const form = document.getElementById("addr-form");
  const input = document.getElementById("addr-input");
  const totalEl = document.getElementById("addr-total");
  const countEl = document.getElementById("addr-count");
  const listEl  = document.getElementById("addr-utxos");
  if (!form || !input) return;

  form.addEventListener("submit", async (e) => {
    e.preventDefault();
    const addr = input.value.trim();
    if (!addr) return;

    totalEl.textContent = "…";
    countEl.textContent = "…";
    listEl.innerHTML = "";

    const j = await getJSON(`/api/addr/${encodeURIComponent(addr)}?details=true`);
    if (!j) { totalEl.textContent = "—"; countEl.textContent = "—"; listEl.textContent = "Lookup failed"; return; }

    totalEl.textContent = j.total_btc != null ? j.total_btc.toLocaleString(undefined, { maximumFractionDigits: 8 }) + " BTC" : "0 BTC";
    countEl.textContent = j.utxo_count ?? 0;

    if (Array.isArray(j.utxos) && j.utxos.length) {
      const rows = j.utxos.slice(0, 25).map(u => `
        <div class="mono mono-wrap" style="margin:2px 0;">
          ${u.txid} : ${u.vout} — ${u.amount_btc.toFixed(8)} BTC${u.height ? ` • h${u.height}` : ``}
        </div>
      `).join("");
      listEl.innerHTML = rows + (j.utxos.length > 25 ? `<div class="sub">(+ more…)</div>` : ``);

      document.getElementById("addr-clear-btn")?.classList.remove("hidden");
      const histBtn = document.getElementById("addr-history-btn");
      if (histBtn) {
        histBtn.onclick = async () => {
          await loadAddrHistory(addr, 0, 25);
          document.getElementById("addr-clear-btn")?.classList.remove("hidden");
        };
      }
    } else {
      listEl.textContent = "(no UTXOs found)";
      if (!j) {
        totalEl.textContent = "—";
        countEl.textContent = "—";
        listEl.textContent = "Lookup failed";
        document.getElementById("addr-clear-btn")?.classList.add("hidden");
        return;
      }
    }
  });
}

async function loadAddrHistory(addr, offset = 0, limit = 25) {
  const j = await getJSON(`/api/addr/${encodeURIComponent(addr)}/history?offset=${offset}&limit=${limit}`);
  const histEl = document.getElementById("addr-history");
  if (!histEl) return;
  if (!j) { histEl.textContent = "History lookup failed."; return; }

  if (!Array.isArray(j.items) || j.items.length === 0) {
    histEl.textContent = "(no history)";
    return;
  }
  const rows = j.items.map(it => `
    <div class="mono mono-wrap" style="margin:2px 0;">
      ${it.txid} ${it.height > 0 ? `• h${it.height}` : `• mempool`}
      <button class="btn btn--sm" data-open-tx="${it.txid}" style="margin-left:6px;">Open TX</button>
    </div>
  `).join("");
  histEl.innerHTML = rows;

  histEl.querySelectorAll("button[data-open-tx]").forEach(btn => {
    btn.addEventListener("click", () => showTx(btn.getAttribute("data-open-tx")));
  });
}

function clearAddrWidget() {
  const input   = document.getElementById("addr-input");
  const totalEl = document.getElementById("addr-total");
  const countEl = document.getElementById("addr-count");
  const listEl  = document.getElementById("addr-utxos");
  const histEl  = document.getElementById("addr-history");
  const clearBtn = document.getElementById("addr-clear-btn");

  if (input)   input.value = "";
  if (totalEl) totalEl.textContent = "—";
  if (countEl) countEl.textContent = "—";
  if (listEl)  listEl.innerHTML = "";
  if (histEl)  histEl.innerHTML = "";
  if (clearBtn) clearBtn.classList.add("hidden");
}


async function showTx(txid, resolveN = 20) {
  const res = await getJSON(`/api/tx/${txid}?resolve=${resolveN}`);
  const el = document.getElementById("result");
  if (!el) return;
  if (!res) { el.textContent = "Transaction not found."; return; }

  const conf = res.confirmations ?? 0;
  const vsize = res.vsize ?? 0;

  // Outputs list
  const vout = Array.isArray(res.vout) ? res.vout : [];
  const voutList = vout.map(o => {
    const val = (o.value ?? 0);
    const spk = o.scriptPubKey || {};
    const addr = (spk.addresses && spk.addresses[0]) || spk.address || "(no address)";
    return `<li class="list__item"><div>${fmtNumber(val, 8)} BTC → <span class="mono">${addr}</span></div></li>`;
  }).join("");

  // Inputs (resolved)
  const ins = Array.isArray(res.inputs_resolved) ? res.inputs_resolved : [];
  const insList = ins.map(i =>
    `<li class="list__item"><div>${fmtNumber(i.value_btc, 8)} BTC ← <span class="mono">${i.address}</span>
     <span class="sub">(prev ${i.txid.slice(0,10)}…:${i.vout})</span></div></li>`
  ).join("");

  // Fee/feerate
  const feeLine = (res.fee_btc != null && res.feerate_sat_vb != null)
    ? ` • fee ${fmtNumber(res.fee_btc, 8)} BTC (${fmtNumber(res.feerate_sat_vb, 2)} sat/vB)`
    : ``;

  // Buttons
  const backBtn = lastBlockCtx
    ? `<button class="btn btn--sm" id="back-to-block">← Back to block ${lastBlockCtx.height?.toLocaleString?.() ?? ""}</button>`
    : ``;

  const moreBtn = (res.more_inputs && !res.is_coinbase && ins.length)
    ? `<button class="btn btn--sm" id="load-more-inputs">Load more inputs</button>`
    : ``;

  // Inputs section (coinbase-aware)
  const inputsSection = res.is_coinbase
    ? `<div class="sub">(coinbase transaction — no inputs)</div>`
    : `<ul class="list">${ins.length ? insList : `<li class="list__item">(no inputs resolved yet)</li>`}</ul>${moreBtn}`;

  // Render once
  el.innerHTML = `
    <div class="callout">
      <div style="display:flex;gap:8px;align-items:center;justify-content:space-between;">
        <div><strong>Transaction</strong> ${conf ? `<span class="sub">(${conf} conf)</span>` : `<span class="sub">(unconfirmed)</span>`}</div>
        ${backBtn}
      </div>
      <div class="mono" style="margin-top:6px;">${res.txid}</div>
      <div class="sub" style="margin-top:6px;">vsize ${fmtNumber(vsize,0)} vB${feeLine}</div>

      <h4 style="margin:10px 0 6px;">Outputs</h4>
      <ul class="list">${voutList || `<li class="list__item">(no outputs)</li>`}</ul>

      <h4 style="margin:10px 0 6px;">Inputs</h4>
      ${inputsSection}
    </div>
  `;
  el.scrollIntoView({ behavior: "smooth", block: "start" });

  // Wire buttons
  const back = document.getElementById("back-to-block");
  if (back && lastBlockCtx?.hash) {
    back.addEventListener("click", () => showBlock(lastBlockCtx.hash));
  }
  const more = document.getElementById("load-more-inputs");
  if (more) {
    more.addEventListener("click", () => {
      const next = Math.min((resolveN || 20) + 40, 100);
      showTx(txid, next);
    });
  }
}

// -------------------- Price (unchanged) --------------------
const CB_WS = "wss://advanced-trade-ws.coinbase.com";
let ws, wsRetry = 0, wsTimer;
let currentFiat = "USD";
let currentProduct = () => `BTC-${currentFiat}`;
let latestPrice = null;
let latestOpen = null;
let lastDraw = 0;

function price_connect() {
  clearTimeout(wsTimer);
  ws = new WebSocket(CB_WS);
  ws.onopen = () => {
    wsRetry = 0;
    const sub = { type: "subscribe", channel: "ticker", product_ids: [currentProduct()] };
    ws.send(JSON.stringify(sub));
  };
  ws.onmessage = (ev) => {
    try {
      const msg = JSON.parse(ev.data);
      if (msg.channel === "ticker" && msg.events) {
        for (const e of msg.events) {
          if (!e.tickers) continue;
          for (const t of e.tickers) {
            if (t.product_id !== currentProduct()) continue;
            const price = parseFloat(t.price ?? t.last_trade_price ?? t.best_bid ?? t.best_ask);
            const open24 = parseFloat(t.price_24h ?? t.open_24h);
            if (Number.isFinite(price)) updatePrice(price, open24);
          }
        }
      }
      if (msg.type === "ticker" && msg.product_id === currentProduct()) {
        const price = parseFloat(msg.price ?? msg.last_trade_price ?? msg.best_bid ?? msg.best_ask);
        const open24 = parseFloat(msg.open_24h);
        if (Number.isFinite(price)) updatePrice(price, open24);
      }
    } catch {}
  };
  ws.onclose = ws.onerror = () => {
    wsRetry = Math.min(wsRetry + 1, 6);
    wsTimer = setTimeout(price_connect, 500 * 2 ** wsRetry);
  };
}
function money(n, currency = currentFiat) {
  try {
    return n.toLocaleString(undefined, { style: "currency", currency, minimumFractionDigits: 2, maximumFractionDigits: 2 });
  } catch {
    return `${n.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 })} ${currency}`;
  }
}
function updatePrice(price, open24) { latestPrice = price; latestOpen = open24; }
function drawPrice() {
  if (!latestPrice) return;
  const now = Date.now();
  if (now - lastDraw < 2000) return;
  lastDraw = now;
  const el = document.getElementById("btc-price");
  const ch = document.getElementById("btc-change");
  const tm = document.getElementById("btc-time");
  if (!el) return;
  el.textContent = money(latestPrice);
  if (Number.isFinite(latestOpen) && latestOpen > 0) {
    const pct = ((latestPrice - latestOpen) / latestOpen) * 100;
    ch.textContent = `${pct >= 0 ? "▲" : "▼"} ${pct.toFixed(2)}%`;
    ch.style.color = pct >= 0 ? "#2ecc71" : "#ff6b6b";
  } else {
    ch.textContent = "—"; ch.style.color = "";
  }
  tm.textContent = new Date().toLocaleTimeString();
}
function price_start() {
  const sel = document.getElementById("fiat-select");
  if (sel) {
    sel.addEventListener("change", () => {
      currentFiat = sel.value;
      try { ws && ws.close(); } catch {}
      price_connect();
    });
  }
  document.addEventListener("visibilitychange", () => {
    if (document.hidden) { try { ws && ws.close(); } catch {} } else { price_connect(); }
  });
  price_connect();
}
setInterval(drawPrice, 500);

// -------------------- START --------------------
function start() {
  document.getElementById("year").textContent = new Date().getFullYear();

  // initial load
  refreshNetwork();
  refreshMempool();
  loadLatestBlocks(10);
  hookSearch();
  hookAddrLookup();

  // gentle polling for Pi performance
  setInterval(refreshNetwork, 15000);
  setInterval(refreshMempool, 10000);
  setInterval(() => loadLatestBlocks(10), 30000);

  const clearBtn = document.getElementById("addr-clear-btn");
  if (clearBtn) clearBtn.addEventListener("click", clearAddrWidget);

  price_start();
}
document.addEventListener("DOMContentLoaded", start);
