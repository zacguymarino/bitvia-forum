// ============================================================
// Bitvia Explorer Frontend Script
// - Fetches data from /api/* endpoints
// - Renders mempool + network summary
// - Provides search, block/tx views, and address tools
// - Streams BTC price via Coinbase Advanced Trade WebSocket
// ============================================================

// Remember the last block view so we can "back" from tx -> block
let lastBlockCtx = null;

// ============================================================
// Generic helpers
// ============================================================

/**
 * Fetch JSON from the given URL with no cache.
 * Logs a warning and returns null on error.
 */
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

/**
 * Human-readable bytes (B, KB, MB, GB, TB).
 */
function fmtBytes(n) {
  if (n == null) return "—";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let i = 0;
  let v = n;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  const decimals = v >= 100 ? 0 : v >= 10 ? 1 : 2;
  return `${v.toFixed(decimals)} ${units[i]}`;
}

/**
 * Simple yes/no text for booleans.
 */
function fmtBool(b) {
  return b ? "yes" : "no";
}

/**
 * Number with localized thousands separator and limited decimals.
 */
function fmtNumber(n, digits = 2) {
  if (n == null || !isFinite(n)) return "—";
  return n.toLocaleString(undefined, { maximumFractionDigits: digits });
}

/**
 * Format seconds as either "Xs" or "Ym".
 */
function fmtSecs(s) {
  if (!isFinite(s)) return "—";
  const m = s / 60;
  return m >= 1 ? `${fmtNumber(m, 1)} min` : `${fmtNumber(s, 0)} s`;
}

/**
 * Compact number formatting:
 *   1_000 → 1.00 K
 *   1_000_000 → 1.00 M
 */
function fmtCompact(n, digits = 2) {
  if (n == null || !isFinite(n)) return "—";
  const units = ["", "K", "M", "B", "T", "Qa", "Qi"];
  let v = Math.abs(n);
  let u = 0;
  while (v >= 1000 && u < units.length - 1) {
    v /= 1000;
    u++;
  }
  const sign = n < 0 ? "-" : "";
  return `${sign}${v.toFixed(digits)} ${units[u]}`.trim();
}

/**
 * Format hashrate in H/s with proper units (kH/s → ZH/s).
 * Input should be in H/s.
 */
function fmtHashrateHps(hps) {
  if (hps == null || !isFinite(hps) || hps <= 0) return "—";
  const units = ["H/s", "kH/s", "MH/s", "GH/s", "TH/s", "PH/s", "EH/s", "ZH/s"];
  let v = hps;
  let i = 0;
  while (v >= 1000 && i < units.length - 1) {
    v /= 1000;
    i++;
  }
  const decimals = v >= 100 ? 0 : v >= 10 ? 1 : 2;
  return `${v.toFixed(decimals)} ${units[i]}`;
}

/**
 * Time ago (in seconds since epoch) → "3 min ago", "2 h ago", etc.
 */
function timeAgo(tsSec) {
  if (!tsSec) return "—";
  const diff = Date.now() / 1000 - tsSec;
  if (diff < 60) return `${Math.floor(diff)} s ago`;
  if (diff < 3600) return `${Math.floor(diff / 60)} min ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)} h ago`;
  return `${Math.floor(diff / 86400)} d ago`;
}

/**
 * Convenience helper: set element's textContent by id.
 */
function setText(id, val) {
  const el = document.getElementById(id);
  if (el) {
    el.textContent = val;
  } else {
    console.warn(`[setText] Missing element #${id}`);
  }
}

/**
 * Simple validators for search:
 * - isHex64: 64-char hex string (block/tx ids)
 * - isDigits: integer height
 */
function isHex64(s) {
  return typeof s === "string" && /^[0-9a-fA-F]{64}$/.test(s);
}
function isDigits(s) {
  return typeof s === "string" && /^[0-9]+$/.test(s);
}

// ============================================================
// Mempool widget
// ============================================================

/**
 * Fetch and render mempool summary (/api/mempoolinfo).
 */
async function refreshMempool() {
  const m = await getJSON("/api/mempoolinfo");
  if (!m) return;

  setText("mp-size", m.size ?? "—");
  setText("mp-bytes", fmtBytes(m.bytes ?? 0));
  setText("mp-usage", fmtBytes(m.usage ?? 0));
  setText("mp-minfee", (m.mempoolminfee ?? 0).toFixed(2));
  setText("mp-rbf", fmtBool(m.fullrbf ?? false));
  setText("mp-unb", m.unbroadcastcount ?? 0);
}

// ============================================================
// Network summary widget
// ============================================================

/**
 * Fetch and render network / chain summary (/api/network).
 */
async function refreshNetwork() {
  const d = await getJSON("/api/network");
  if (!d) return;

  // Height & difficulty
  setText("net-height", d.height?.toLocaleString() ?? "—");
  setText("net-difficulty", fmtCompact(d.difficulty, 2));

  // Hashrate: API gives GH/s, convert to H/s for fmtHashrateHps
  const ghps = d.hashrate_ghps ?? 0;
  const hps = ghps * 1e9;
  setText("net-hashrate", fmtHashrateHps(hps));

  // Average block interval
  setText("net-interval", fmtSecs(d.avg_block_interval_sec));

  // Difficulty adjustment estimate
  setText("net-nextadj", d.blocks_to_next_adjust?.toLocaleString() ?? "—");
  const chg = d.est_diff_change_pct ?? 0;
  const chgEl = document.getElementById("net-diffchg");
  if (chgEl) {
    chgEl.textContent = `${chg >= 0 ? "▲" : "▼"} ${Math.abs(chg).toFixed(2)}%`;
    chgEl.style.color = chg >= 0 ? "#2ecc71" : "#ff6b6b";
  }

  // Subsidy + issuance + supply
  setText("net-subsidy", fmtNumber(d.current_subsidy_btc, 8));
  setText("net-newday", fmtNumber(d.est_new_btc_per_day, 2));
  setText("net-supply", fmtNumber(d.est_circulating_btc, 3));
}

// ============================================================
// Latest blocks list + search
// ============================================================

/**
 * Load the latest N blocks starting from the tip, using:
 *   /api/network       for current height
 *   /api/blockhash/h   for each block hash
 */
async function loadLatestBlocks(n = 10) {
  const net = await getJSON("/api/network");
  const list = document.getElementById("blocks-list");
  if (!net || !list) return;

  const tip = net.height ?? 0;
  const items = [];
  const count = Math.max(1, Math.min(n, 20)); // hard cap at 20 for now

  for (let i = 0; i < count; i++) {
    const h = tip - i;
    if (h < 0) break;

    const bh = await getJSON(`/api/blockhash/${h}`);
    if (bh && bh.hash) {
      items.push({ height: h, hash: bh.hash });
    }
  }

  list.innerHTML = items
    .map(
      (it) => `
      <li class="list__item">
        <div class="mono-wrap">${it.hash}</div>
        <div class="muted">Height ${it.height.toLocaleString()}</div>
        <button class="btn btn--sm" data-hash="${it.hash}" data-height="${it.height}">Open</button>
      </li>
    `
    )
    .join("");

  // Wire "Open" buttons to block view
  list.querySelectorAll("button[data-hash]").forEach((btn) => {
    btn.addEventListener("click", () => {
      const hash = btn.getAttribute("data-hash");
      showBlock(hash);
    });
  });
}

/**
 * Hook search form:
 * - Pure digits: treat as block height
 * - 64-hex: try block first, then tx
 */
function hookSearch() {
  const form = document.getElementById("search-form");
  const input = document.getElementById("search-input");
  const result = document.getElementById("result");

  if (!form || !input || !result) return;

  form.addEventListener("submit", async (e) => {
    e.preventDefault();
    const q = input.value.trim();
    if (!q) return;

    // Case 1: height
    if (isDigits(q)) {
      const h = Number(q);
      const bh = await getJSON(`/api/blockhash/${h}`);
      if (bh && bh.hash) {
        return showBlock(bh.hash);
      }
      result.textContent = "Block not found.";
      return;
    }

    // Case 2: 64-char hex → try block, then tx
    if (isHex64(q)) {
      const tryBlock = await getJSON(`/api/block/${q}`);
      if (tryBlock) return showBlock(q);

      const tryTx = await getJSON(`/api/tx/${q}`);
      if (tryTx) return showTx(q);

      result.textContent = "No block or tx found for that hash.";
      return;
    }

    // Fallback: invalid input
    result.textContent = "Please enter a height (digits) or a 64-char hex.";
  });
}

// ============================================================
// Block view (with tx pagination)
// ============================================================

/**
 * Render a block detail page, including a paged tx list.
 *
 * - hash:   block hash to show
 * - offset: starting tx index for pagination
 * - limit:  page size
 *
 * Uses /api/block/{hash}?offset=&limit=
 */
async function showBlock(hash, offset = 0, limit = 20) {
  const res = await getJSON(`/api/block/${hash}?offset=${offset}&limit=${limit}`);
  const el = document.getElementById("result");
  if (!el) return;
  if (!res) {
    el.textContent = "Block not found.";
    return;
  }

  // Remember context for "back to block" from tx view
  lastBlockCtx = { hash: res.hash, height: res.height, limit };

  const when = timeAgo(res.time);
  const prevHtml = res.prev
    ? `Prev: <button class="btn btn--sm" data-goto-block="${res.prev}">Open</button> <span class="mono-wrap">${res.prev}</span>`
    : `Prev: —`;
  const nextHtml = res.next
    ? `Next: <button class="btn btn--sm" data-goto-block="${res.next}">Open</button> <span class="mono-wrap">${res.next}</span>`
    : ``;

  // Transaction list
  const txListHtml = res.txids
    .map(
      (t) => `
      <li class="list__item">
        <div class="mono-wrap">${t}</div>
        <button class="btn btn--sm" data-tx="${t}">Open TX</button>
      </li>
    `
    )
    .join("");

  // Pagination calculations
  const from = res.total_tx === 0 ? 0 : res.offset + 1;
  const to = Math.min(res.offset + res.limit, res.total_tx);
  const canPrev = res.offset > 0;
  const canNext = res.offset + res.limit < res.total_tx;

  const pagerHtml =
    res.total_tx > res.limit
      ? `
        <div class="sub" style="display:flex;gap:8px;align-items:center;justify-content:space-between;margin-top:8px;">
          <div>Showing ${from.toLocaleString()}–${to.toLocaleString()} of ${res.total_tx.toLocaleString()}</div>
          <div style="display:flex;gap:6px;">
            <button class="btn btn--sm" id="pg-first" ${canPrev ? "" : "disabled"}>« First</button>
            <button class="btn btn--sm" id="pg-prev"  ${canPrev ? "" : "disabled"}>‹ Prev</button>
            <button class="btn btn--sm" id="pg-next"  ${canNext ? "" : "disabled"}>Next ›</button>
            <button class="btn btn--sm" id="pg-last"  ${canNext ? "" : "disabled"}>Last »</button>
          </div>
        </div>
      `
      : ``;

  // Main block view
  el.innerHTML = `
    <div class="callout">
      <div><strong>Block</strong> <span class="muted">height</span> ${res.height.toLocaleString()}</div>
      <div class="mono-wrap">${res.hash}</div>
      <div class="sub" style="margin-top:6px;">
        ${when} • ${fmtNumber(res.size, 0)} B • ${fmtNumber(res.weight ?? 0, 0)} WU • tx: ${fmtNumber(res.n_tx, 0)}
      </div>

      <div class="sub" style="margin:6px 0;">${prevHtml}</div>
      ${nextHtml ? `<div class="sub">${nextHtml}</div>` : ``}

      <h4 style="margin:10px 0 6px;">Transactions</h4>
      <ul class="list">
        ${txListHtml || `<li class="list__item">(no transactions)</li>`}
      </ul>
      ${pagerHtml}
    </div>
  `;
  el.scrollIntoView({ behavior: "smooth", block: "start" });

  // Wire tx buttons
  el.querySelectorAll("button[data-tx]").forEach((btn) => {
    btn.addEventListener("click", () => showTx(btn.getAttribute("data-tx")));
  });

  // Wire block prev/next
  el.querySelectorAll("button[data-goto-block]").forEach((btn) => {
    btn.addEventListener("click", () =>
      showBlock(btn.getAttribute("data-goto-block"), 0, lastBlockCtx?.limit ?? 20)
    );
  });

  // Wire pager buttons
  const pageSize = res.limit;
  const total = res.total_tx;

  const first = document.getElementById("pg-first");
  const prev = document.getElementById("pg-prev");
  const next = document.getElementById("pg-next");
  const last = document.getElementById("pg-last");

  if (first) first.addEventListener("click", () => showBlock(hash, 0, pageSize));
  if (prev)
    prev.addEventListener("click", () => showBlock(hash, Math.max(0, res.offset - pageSize), pageSize));
  if (next) next.addEventListener("click", () => showBlock(hash, res.offset + pageSize, pageSize));
  if (last) {
    last.addEventListener("click", () => {
      const remainder = total % pageSize;
      const lastOffset = remainder === 0 ? Math.max(0, total - pageSize) : total - remainder;
      showBlock(hash, lastOffset, pageSize);
    });
  }
}

// ============================================================
// Address balance + UTXO + history widget
// ============================================================

/**
 * Wire up the address lookup form:
 * - Query /api/addr/{address}?details=true
 * - Render total balance + first 25 UTXOs
 * - Enable "history" and "clear" buttons
 */
function hookAddrLookup() {
  const form = document.getElementById("addr-form");
  const input = document.getElementById("addr-input");
  const totalEl = document.getElementById("addr-total");
  const countEl = document.getElementById("addr-count");
  const listEl = document.getElementById("addr-utxos");
  if (!form || !input) return;

  form.addEventListener("submit", async (e) => {
    e.preventDefault();
    const addr = input.value.trim();
    if (!addr) return;

    totalEl.textContent = "…";
    countEl.textContent = "…";
    listEl.innerHTML = "";

    const j = await getJSON(`/api/addr/${encodeURIComponent(addr)}?details=true`);
    if (!j) {
      totalEl.textContent = "—";
      countEl.textContent = "—";
      listEl.textContent = "Lookup failed";
      return;
    }

    // Summary
    totalEl.textContent =
      j.total_btc != null
        ? j.total_btc.toLocaleString(undefined, { maximumFractionDigits: 8 }) + " BTC"
        : "0 BTC";
    countEl.textContent = j.utxo_count ?? 0;

    // UTXO list (first 25)
    if (Array.isArray(j.utxos) && j.utxos.length) {
      const rows = j.utxos
        .slice(0, 25)
        .map(
          (u) => `
          <div class="mono-wrap" style="margin:2px 0;">
            ${u.txid} : ${u.vout} — ${u.amount_btc.toFixed(8)} BTC${u.height ? ` • h${u.height}` : ``}
          </div>
        `
        )
        .join("");
      listEl.innerHTML =
        rows + (j.utxos.length > 25 ? `<div class="sub">(+ more…)</div>` : ``);

      // Show "clear" button
      document.getElementById("addr-clear-btn")?.classList.remove("hidden");

      // History button
      const histBtn = document.getElementById("addr-history-btn");
      if (histBtn) {
        histBtn.onclick = async () => {
          await loadAddrHistory(addr, 0, 25);
          document.getElementById("addr-clear-btn")?.classList.remove("hidden");
        };
      }
    } else {
      listEl.textContent = "(no UTXOs found)";
    }
  });
}

async function loadAddrHistory(addr, offset = 0, limit = 25) {
  const j = await getJSON(`/api/addr/${encodeURIComponent(addr)}/history?offset=${offset}&limit=${limit}`);
  const histEl = document.getElementById("addr-history");
  if (!histEl) return;

  if (!j) {
    histEl.textContent = "History lookup failed.";
    return;
  }

  if (!Array.isArray(j.items) || j.items.length === 0) {
    histEl.textContent = "(no history)";
    return;
  }

  const rows = j.items.map((it) => {
    const amount = typeof it.delta_btc === "number" ? it.delta_btc : 0;
    const dir = it.direction || (amount > 0 ? "in" : amount < 0 ? "out" : "unknown");

    const sign = amount > 0 ? "+" : amount < 0 ? "−" : "";
    const absAmt = Math.abs(amount);
    const amtStr = absAmt
      ? `${sign}${absAmt.toFixed(8)} BTC`
      : "0 BTC";

    const statusStr = it.height > 0
      ? `confirmed • h${(it.height || 0).toLocaleString?.() ?? it.height}`
      : "mempool";

    const whenStr = it.timestamp
      ? timeAgo(it.timestamp)  // timestamp is seconds since epoch
      : "";

    const rightBits = [statusStr, whenStr].filter(Boolean).join(" • ");

    let amountClass = "";
    if (dir === "in") amountClass = "addr-amount--in";
    else if (dir === "out") amountClass = "addr-amount--out";
    else if (dir === "self") amountClass = "addr-amount--self";

    return `
      <div class="list__item">
        <div class="sub ${amountClass}">${amtStr}</div>
        <div class="mono-wrap">${it.txid}</div>
        <div class="sub">${rightBits}</div>
        <button class="btn btn--sm" data-open-tx="${it.txid}" style="margin-top:4px;">Open TX</button>
      </div>
    `;
  }).join("");

  histEl.innerHTML = rows;

  // Hook "Open TX" buttons
  histEl.querySelectorAll("button[data-open-tx]").forEach((btn) => {
    btn.addEventListener("click", () => showTx(btn.getAttribute("data-open-tx")));
  });
}


/**
 * Clear all address widget state (input, utxos, history, etc).
 */
function clearAddrWidget() {
  const input = document.getElementById("addr-input");
  const totalEl = document.getElementById("addr-total");
  const countEl = document.getElementById("addr-count");
  const listEl = document.getElementById("addr-utxos");
  const histEl = document.getElementById("addr-history");
  const clearBtn = document.getElementById("addr-clear-btn");

  if (input) input.value = "";
  if (totalEl) totalEl.textContent = "—";
  if (countEl) countEl.textContent = "—";
  if (listEl) listEl.innerHTML = "";
  if (histEl) histEl.innerHTML = "";
  if (clearBtn) clearBtn.classList.add("hidden");
}

// ============================================================
// Transaction view
// ============================================================

/**
 * Show transaction detail:
 *   /api/tx/{txid}?resolve=N
 *
 * - Resolved inputs (inputs_resolved)
 * - Outputs
 * - Fee + feerate
 * - Optional "load more inputs" button
 * - Optional "back to block" button
 */
async function showTx(txid, resolveN = 20) {
  const res = await getJSON(`/api/tx/${txid}?resolve=${resolveN}`);
  const el = document.getElementById("result");
  if (!el) return;
  if (!res) {
    el.textContent = "Transaction not found.";
    return;
  }

  const conf = res.confirmations ?? 0;
  const vsize = res.vsize ?? 0;

  // Outputs list
  const vout = Array.isArray(res.vout) ? res.vout : [];
  const voutList = vout
    .map((o) => {
      const val = o.value ?? 0;
      const spk = o.scriptPubKey || {};
      const addr =
        (spk.addresses && spk.addresses[0]) || spk.address || "(no address)";
      return `<li class="list__item"><div>${fmtNumber(
        val,
        8
      )} BTC → <span class="mono-wrap">${addr}</span></div></li>`;
    })
    .join("");

  // Inputs (resolved)
  const ins = Array.isArray(res.inputs_resolved) ? res.inputs_resolved : [];
  const insList = ins
    .map(
      (i) => `
      <li class="list__item">
        <div>
          ${fmtNumber(i.value_btc, 8)} BTC ← <span class="mono-wrap">${i.address}</span>
          <span class="sub">(prev ${i.txid.slice(0, 10)}…:${i.vout})</span>
        </div>
      </li>
    `
    )
    .join("");

  // Fee / feerate line
  const feeLine =
    res.fee_btc != null && res.feerate_sat_vb != null
      ? ` • fee ${fmtNumber(res.fee_btc, 8)} BTC (${fmtNumber(
          res.feerate_sat_vb,
          2
        )} sat/vB)`
      : ``;

  // Back-to-block button
  const backBtn = lastBlockCtx
    ? `<button class="btn btn--sm" id="back-to-block">← Back to block ${lastBlockCtx.height?.toLocaleString?.() ?? ""}</button>`
    : ``;

  // Load more inputs button
  const moreBtn =
    res.more_inputs && !res.is_coinbase && ins.length
      ? `<button class="btn btn--sm" id="load-more-inputs">Load more inputs</button>`
      : ``;

  // Inputs section respects coinbase
  const inputsSection = res.is_coinbase
    ? `<div class="sub">(coinbase transaction — no inputs)</div>`
    : `<ul class="list">${
        ins.length ? insList : `<li class="list__item">(no inputs resolved yet)</li>`
      }</ul>${moreBtn}`;

  // Render
  el.innerHTML = `
    <div class="callout">
      <div style="display:flex;gap:8px;align-items:center;justify-content:space-between;">
        <div>
          <strong>Transaction</strong>
          ${
            conf
              ? `<span class="sub">(${conf} confirmed)</span>`
              : `<span class="sub">(unconfirmed)</span>`
          }
        </div>
        ${backBtn}
      </div>

      <div class="mono-wrap" style="margin-top:6px;">${res.txid}</div>
      <div class="sub" style="margin-top:6px;">vsize ${fmtNumber(
        vsize,
        0
      )} vB${feeLine}</div>

      <h4 style="margin:10px 0 6px;">Outputs</h4>
      <ul class="list">${voutList || `<li class="list__item">(no outputs)</li>`}</ul>

      <h4 style="margin:10px 0 6px;">Inputs</h4>
      ${inputsSection}
    </div>
  `;
  el.scrollIntoView({ behavior: "smooth", block: "start" });

  // Wire back-to-block button
  const back = document.getElementById("back-to-block");
  if (back && lastBlockCtx?.hash) {
    back.addEventListener("click", () => showBlock(lastBlockCtx.hash));
  }

  // Wire load-more-inputs button
  const more = document.getElementById("load-more-inputs");
  if (more) {
    more.addEventListener("click", () => {
      const next = Math.min((resolveN || 20) + 40, 100);
      showTx(txid, next);
    });
  }
}

// ============================================================
// BTC price widget (Coinbase Advanced Trade WebSocket)
// ============================================================

const CB_WS = "wss://advanced-trade-ws.coinbase.com";
let ws;
let wsRetry = 0;
let wsTimer;
let currentFiat = "USD";
let currentProduct = () => `BTC-${currentFiat}`;
let latestPrice = null;
let latestOpen = null;
let lastDraw = 0;

/**
 * Open / re-open Coinbase WebSocket and subscribe to ticker.
 */
function price_connect() {
  clearTimeout(wsTimer);

  ws = new WebSocket(CB_WS);

  ws.onopen = () => {
    wsRetry = 0;
    const sub = {
      type: "subscribe",
      channel: "ticker",
      product_ids: [currentProduct()],
    };
    ws.send(JSON.stringify(sub));
  };

  ws.onmessage = (ev) => {
    try {
      const msg = JSON.parse(ev.data);

      // Newer style (channel + events)
      if (msg.channel === "ticker" && msg.events) {
        for (const e of msg.events) {
          if (!e.tickers) continue;
          for (const t of e.tickers) {
            if (t.product_id !== currentProduct()) continue;
            const price = parseFloat(
              t.price ?? t.last_trade_price ?? t.best_bid ?? t.best_ask
            );
            const open24 = parseFloat(t.price_24h ?? t.open_24h);
            if (Number.isFinite(price)) updatePrice(price, open24);
          }
        }
      }

      // Legacy / alternative ticker form
      if (msg.type === "ticker" && msg.product_id === currentProduct()) {
        const price = parseFloat(
          msg.price ?? msg.last_trade_price ?? msg.best_bid ?? msg.best_ask
        );
        const open24 = parseFloat(msg.open_24h);
        if (Number.isFinite(price)) updatePrice(price, open24);
      }
    } catch {
      // ignore parse errors
    }
  };

  ws.onclose = ws.onerror = () => {
    wsRetry = Math.min(wsRetry + 1, 6);
    wsTimer = setTimeout(price_connect, 500 * 2 ** wsRetry);
  };
}

/**
 * Format a number as localized fiat (USD, EUR, etc).
 */
function money(n, currency = currentFiat) {
  try {
    return n.toLocaleString(undefined, {
      style: "currency",
      currency,
      minimumFractionDigits: 2,
      maximumFractionDigits: 2,
    });
  } catch {
    // Fallback if locale/currency not supported
    return `${n.toLocaleString(undefined, {
      minimumFractionDigits: 2,
      maximumFractionDigits: 2,
    })} ${currency}`;
  }
}

/**
 * Update cached price/open, used by drawPrice().
 */
function updatePrice(price, open24) {
  latestPrice = price;
  latestOpen = open24;
}

/**
 * Draw current price + 24h change + update timestamp.
 * Throttled to avoid spamming the DOM.
 */
function drawPrice() {
  if (!latestPrice) return;

  const now = Date.now();
  if (now - lastDraw < 2000) return; // throttle to every 2s
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
    ch.textContent = "—";
    ch.style.color = "";
  }

  if (tm) {
    tm.textContent = new Date().toLocaleTimeString();
  }
}

/**
 * Initialize price widget:
 * - hook fiat selector
 * - react to visibility changes
 * - open WebSocket
 */
function price_start() {
  const sel = document.getElementById("fiat-select");
  if (sel) {
    sel.addEventListener("change", () => {
      currentFiat = sel.value;
      try {
        ws && ws.close();
      } catch {}
      price_connect();
    });
  }

  document.addEventListener("visibilitychange", () => {
    if (document.hidden) {
      try {
        ws && ws.close();
      } catch {}
    } else {
      price_connect();
    }
  });

  price_connect();
}

// Periodically repaint the price (using cached websocket data)
setInterval(drawPrice, 500);


/**
 * Simple validation: txid must be 64-char hex.
 */
function isValidTxid(s) {
  return typeof s === "string" && /^[0-9a-fA-F]{64}$/.test(s.trim());
}

/**
 * Check transaction status via /api/tx/{txid}.
 *
 * - confirmed: confirmations > 0
 * - mempool:   confirmations == 0
 * - unseen:    API returns null / error
 */
async function checkTxStatus(txid) {
  const statusEl = document.getElementById("txstatus-output");
  const openBtn  = document.getElementById("txstatus-open-btn");

  if (!statusEl) return;

  // Reset UI
  statusEl.textContent = "Checking…";
  statusEl.className = "sub";
  if (openBtn) {
    openBtn.classList.add("hidden");
    openBtn.onclick = null;
  }

  // Basic validation
  if (!isValidTxid(txid)) {
    statusEl.textContent = "Please enter a valid 64-character hex transaction ID.";
    statusEl.classList.add("txstatus-status--error");
    return;
  }

  // Ask the backend for tx details
  const j = await getJSON(`/api/tx/${txid}?resolve=0`); // light lookup
  if (!j) {
    // Not found by this node (neither mempool nor confirmed)
    statusEl.textContent =
      "Unseen by this node: not in mempool and no confirmed transaction found. " +
      "It may be in another node's mempool or the ID may be invalid.";
    statusEl.classList.add("txstatus-status--unknown");
    return;
  }

  const conf = j.confirmations ?? 0;

  if (conf > 0) {
    // Confirmed
    statusEl.textContent =
      `✅ Confirmed on-chain with ${conf} confirmation` + (conf === 1 ? "" : "s") + ".";
    statusEl.classList.add("txstatus-status--confirmed");
  } else {
    // Known but unconfirmed
    statusEl.textContent =
      "⏳ Seen by this node and currently in the mempool (unconfirmed).";
    statusEl.classList.add("txstatus-status--mempool");
  }

  // Offer to open full tx details
  if (openBtn) {
    openBtn.classList.remove("hidden");
    openBtn.onclick = () => showTx(txid);
  }
}

/**
 * Attach the tx status form behavior.
 */
function hookTxStatusWidget() {
  const form  = document.getElementById("txstatus-form");
  const input = document.getElementById("txstatus-input");
  const statusEl = document.getElementById("txstatus-output");

  if (!form || !input || !statusEl) return;

  form.addEventListener("submit", (e) => {
    e.preventDefault();
    const txid = input.value.trim();
    if (!txid) return;
    checkTxStatus(txid);
  });
}


// ============================================================
// App startup
// ============================================================

/**
 * Main entrypoint: called once DOM is ready.
 * - Sets footer year
 * - Loads initial data
 * - Starts polling
 * - Hooks search + address widgets
 * - Initializes price stream
 */
function start() {
  // Footer year
  const yearEl = document.getElementById("year");
  if (yearEl) yearEl.textContent = new Date().getFullYear();

  // Initial data loads
  refreshNetwork();
  refreshMempool();
  loadLatestBlocks(10);
  hookSearch();
  hookAddrLookup();
  hookTxStatusWidget();

  // Gentle polling intervals (respecting Pi resources)
  setInterval(refreshNetwork, 15000);
  setInterval(refreshMempool, 10000);
  setInterval(() => loadLatestBlocks(10), 30000);

  // Address clear button
  const clearBtn = document.getElementById("addr-clear-btn");
  if (clearBtn) clearBtn.addEventListener("click", clearAddrWidget);

  // Start BTC price stream
  price_start();
}

// Run once the DOM is ready
document.addEventListener("DOMContentLoaded", start);
