#!/usr/bin/env node
/**
 * Build docs/Chakra-Technical-Architecture.docx from the architecture
 * prose and rendered PNGs. Run from anywhere:
 *   node docs/diagrams/export-docx.mjs
 */
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import {
  AlignmentType,
  BorderStyle,
  Document,
  Footer,
  Header,
  HeadingLevel,
  ImageRun,
  LevelFormat,
  Packer,
  PageNumber,
  Paragraph,
  ShadingType,
  TabStopPosition,
  TabStopType,
  Table,
  TableCell,
  TableRow,
  TextRun,
  WidthType,
} from "docx";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const docsDir = path.resolve(__dirname, "..");
const diagramsDir = __dirname;
const outPath = path.join(docsDir, "Chakra-Technical-Architecture.docx");

const PAGE_W = 12240;
const PAGE_H = 15840;
const MARGIN = 1440;
const CONTENT_W = PAGE_W - MARGIN * 2; // 9360 DXA
const IMG_MAX_PX = 624; // 6.5 in at 96 dpi

const border = { style: BorderStyle.SINGLE, size: 1, color: "CCCCCC" };
const borders = { top: border, bottom: border, left: border, right: border };
const headerFill = "F7EDE4";
const cellMargins = { top: 80, bottom: 80, left: 120, right: 120 };

function pngSize(buf) {
  return { width: buf.readUInt32BE(16), height: buf.readUInt32BE(20) };
}

function imageParagraph(file, name, description) {
  const data = fs.readFileSync(path.join(diagramsDir, file));
  const { width, height } = pngSize(data);
  const w = Math.min(IMG_MAX_PX, width);
  const h = Math.round(w * (height / width));
  return new Paragraph({
    alignment: AlignmentType.CENTER,
    spacing: { before: 120, after: 120 },
    children: [
      new ImageRun({
        type: "png",
        data,
        transformation: { width: w, height: h },
        altText: { name, description, title: name },
      }),
    ],
  });
}

function caption(text) {
  return new Paragraph({
    spacing: { after: 240 },
    children: [new TextRun({ text, italics: true, size: 20, color: "5D5148" })],
  });
}

function para(text, after = 160) {
  return new Paragraph({
    spacing: { after },
    children: [new TextRun(text)],
  });
}

function runsPara(children, after = 160) {
  return new Paragraph({ spacing: { after }, children });
}

function code(text) {
  return new TextRun({ text, font: "Courier New" });
}

function h1(text) {
  return new Paragraph({
    heading: HeadingLevel.HEADING_1,
    children: [new TextRun(text)],
  });
}

function h2(text) {
  return new Paragraph({
    heading: HeadingLevel.HEADING_2,
    children: [new TextRun(text)],
  });
}

function h3(text) {
  return new Paragraph({
    heading: HeadingLevel.HEADING_3,
    children: [new TextRun(text)],
  });
}

function bullet(text, extraRuns = []) {
  return new Paragraph({
    numbering: { reference: "bullets", level: 0 },
    spacing: { after: 80 },
    children: [new TextRun(text), ...extraRuns],
  });
}

function bulletMixed(children) {
  return new Paragraph({
    numbering: { reference: "bullets", level: 0 },
    spacing: { after: 80 },
    children,
  });
}

function numbered(ref, text, extraRuns = []) {
  return new Paragraph({
    numbering: { reference: ref, level: 0 },
    spacing: { after: 80 },
    children: [new TextRun(text), ...extraRuns],
  });
}

function numberedMixed(ref, children) {
  return new Paragraph({
    numbering: { reference: ref, level: 0 },
    spacing: { after: 80 },
    children,
  });
}

function cell(text, width, header = false) {
  return new TableCell({
    borders,
    width: { size: width, type: WidthType.DXA },
    shading: header ? { fill: headerFill, type: ShadingType.CLEAR } : undefined,
    margins: cellMargins,
    children: [
      new Paragraph({
        children: [new TextRun({ text, bold: header, size: header ? 20 : 22 })],
      }),
    ],
  });
}

function table(colWidths, rows) {
  const width = colWidths.reduce((a, b) => a + b, 0);
  return new Table({
    width: { size: width, type: WidthType.DXA },
    columnWidths: colWidths,
    rows: rows.map((row, i) =>
      new TableRow({
        children: row.map((text, j) => cell(text, colWidths[j], i === 0)),
      })
    ),
  });
}

const numbering = {
  config: [
    {
      reference: "bullets",
      levels: [
        {
          level: 0,
          format: LevelFormat.BULLET,
          text: "•",
          alignment: AlignmentType.LEFT,
          style: { paragraph: { indent: { left: 720, hanging: 360 } } },
        },
      ],
    },
    ...["quoteSteps", "swapSteps", "poolSteps"].map((reference) => ({
      reference,
      levels: [
        {
          level: 0,
          format: LevelFormat.DECIMAL,
          text: "%1.",
          alignment: AlignmentType.LEFT,
          style: { paragraph: { indent: { left: 720, hanging: 360 } } },
        },
      ],
    })),
  ],
};

const children = [
  h1("Architecture"),
  para(
    "Chakra quotes across allowlisted Arc Testnet venues (Xylo, Presto, UnitFlow) and executes atomically through a non-custodial Solidity aggregator (splitSwap). The quote plane uses local AMM, stable-swap, and concentrated liquidity (CLMM) math over a Redis copy of pool state. Quotes do not call RPC. The API never holds keys and never submits. Output returns to msg.sender in the same transaction."
  ),
  para(
    "The token catalog is ERC-20 USDC, EURC, and cirBTC. Native USDC gas is a separate 18-decimal field and is never summed with swap USDC."
  ),
  para(
    "The HTTP boundary is chakra-api-server. Path finding, quoting, split optimization, and DEX adapters ship as in-process libraries, not as separate network services."
  ),
  para(
    "Hosted API: https://chakra-api-0a5i.onrender.com. Swap UI: https://chakra-ag.vercel.app. Explorer: https://testnet.arcscan.app."
  ),

  h2("Components"),
  para("The implementation is these pieces."),
  bulletMixed([
    new TextRun({ text: "Aggregator contract. ", bold: true }),
    new TextRun(
      "The public swap entrypoint. splitSwap pulls tokenIn from the user through Permit2, executes one or more SubRoutes (a one-path swap is one entry; a split is several), checks minAmountOut, transfers tokenOut to msg.sender, and sweeps leftover catalog tokens to the sender. Factory allowlist gates every hop. An empty Permit2 signature skips permit(). Splits cannot share a pool. The contract does not retain funds after the call."
    ),
  ]),
  bulletMixed([
    new TextRun({ text: "chakra-market-data-worker. ", bold: true }),
    new TextRun(
      "The single writer of market state. Discovers pools, watches Arc logs, and publishes the routing graph and per-pool reserves or ticks under the chakra: Redis prefix."
    ),
  ]),
  bulletMixed([
    new TextRun({ text: "chakra-api-server. ", bold: true }),
    new TextRun(
      "Stateless HTTP API. Quotes, encodes unsigned splitSwap calldata, and serves tokens, balances, stats, health, and readiness. Horizontal replicas read the same Redis. The API does not submit transactions."
    ),
  ]),
  bulletMixed([
    new TextRun({ text: "router-engine. ", bold: true }),
    new TextRun(
      "In-process PathFinder (BFS), QuoteEngine (local pool math), and SplitOptimizer (Brent on two paths, pairwise merge on more)."
    ),
  ]),
  bulletMixed([
    new TextRun({ text: "dex-adapters. ", bold: true }),
    new TextRun(
      "Per-venue discovery, log matching, and quote math for Xylo, Presto, UnitFlow V2.5 (30 bps xy=k), and generic xy=k / stable / CLMM factories on the seed list."
    ),
  ]),
  bulletMixed([
    new TextRun({ text: "Redis. ", bold: true }),
    new TextRun(
      "Routing topology (chakra:snapshot:*), live pool state (chakra:pool:xyk|stable|clmm:*), factory allowlist (chakra:factories), and additive swap analytics (chakra:analytics:*)."
    ),
  ]),
  bulletMixed([
    new TextRun({ text: "Swap UI and @chakra-ag/sdk. ", bold: true }),
    new TextRun(
      "Browser UI and TypeScript client for quote, build, sign, and submit. A public dashboard is at https://chakra-ag.vercel.app/stats."
    ),
  ]),
  bulletMixed([
    new TextRun({ text: "Circle user-controlled SCA and Gas Station. ", bold: true }),
    new TextRun(
      "Grant targets. They are not on the live path. External wallets sign and submit today."
    ),
  ]),
  bulletMixed([
    new TextRun({ text: "Arb-only vault and operator bot. ", bold: true }),
    new TextRun(
      "Grant targets. They are not on the live path. A future vault is a separate contract that would call splitSwap twice in one transaction. The aggregator stays non-upgradeable. Not retail yield."
    ),
  ]),
  imageParagraph(
    "system-components.png",
    "Chakra system components",
    "Clients talk to the API. The worker is the only Redis writer. Hops execute in the aggregator. Circle Wallets and Gas Station are grant targets."
  ),
  caption(
    "System components. Clients talk to the API. The worker is the only Redis writer. Hops execute in the aggregator. Circle Wallets and Gas Station are drawn dotted: grant target, not a live path."
  ),

  h2("Terminology"),
  bulletMixed([
    new TextRun({ text: "Quote. ", bold: true }),
    new TextRun(
      "Off-chain computation of expected output and sub_routes for a pair and amount_in. It is not a reservation and not an on-chain lock."
    ),
  ]),
  bulletMixed([
    new TextRun({ text: "Sub-route. ", bold: true }),
    new TextRun(
      "One executable leg: an amount_in and an ordered list of hops (Hop: pool, DexType, tokens, fee). Several sub-routes with the same tokenOut are a split."
    ),
  ]),
  bulletMixed([
    new TextRun({ text: "Split. ", bold: true }),
    new TextRun(
      "One swap that sends input down two or more paths in a single splitSwap call. The optimizer splits only when price impact or competing paths warrant it. Sub-routes cannot share a pool."
    ),
  ]),
  bulletMixed([
    new TextRun({ text: "Routing graph. ", bold: true }),
    new TextRun(
      "Token pairs, pool addresses, fee tiers, factories, and CLMM references. Topology only. No reserves or ticks."
    ),
  ]),
  bulletMixed([
    new TextRun({ text: "Pool state. ", bold: true }),
    new TextRun(
      "Live xy=k reserves, stable balances, or CLMM slot0 plus ticks. Stored under chakra:pool:*."
    ),
  ]),
  bulletMixed([
    new TextRun({ text: "Hydrate. ", bold: true }),
    new TextRun(
      "Per-quote Redis read of pool keys on candidate paths. The API does not keep a long-lived in-process pool cache. /quote never calls RPC."
    ),
  ]),
  bulletMixed([
    new TextRun({ text: "Route identity. ", bold: true }),
    new TextRun(
      "Token path, pool addresses, venue types, fees, and factories. /build_tx revalidates this against the current snapshot. It does not re-quote."
    ),
  ]),
  bulletMixed([
    new TextRun({ text: "Permit2. ", bold: true }),
    new TextRun(
      "Uniswap AllowanceTransfer at 0x000000000022D473030F116dDEE9F6B43aC78BA3. The aggregator pulls exact amountIn. Empty signature means allowance is already set."
    ),
  ]),
  bulletMixed([
    new TextRun({ text: "splitSwap. ", bold: true }),
    new TextRun(
      "The aggregator's public swap. value is always 0. protocol_fee_bps is always 0."
    ),
  ]),
  bulletMixed([
    new TextRun({ text: "Readiness. ", bold: true }),
    new TextRun(
      "/ready is true when chakra:snapshot:current exists and at least one chakra:pool:* key is present. Optional CHAKRA_STRICT_READINESS also requires a usable direct or multihop quote on each of the six directed catalog pairs."
    ),
  ]),

  h2("Roles"),
  h3("End user"),
  bullet("Quotes and swaps from the UI or a wallet integration."),
  bullet("Signs every approval, Permit2 message, and swap. Chakra never holds the user's key."),
  bullet("Receives tokenOut directly. Must hold the ERC-20 catalog tokens being swapped."),
  bullet("Pays Arc gas today. Circle Gas Station is a grant target."),
  h3("Integrator"),
  bullet("Calls GET /api/v1/quote and POST /api/v1/build_tx from a dApp or backend."),
  bullet("Maps quote.sub_routes into build_tx. Signs with the user's wallet."),
  bullet("May use @chakra-ag/sdk or the OpenAPI spec."),
  bullet("Submits the signed transaction from the wallet. The API has no submit endpoint."),
  h3("Chakra operator"),
  bullet("Runs the worker, Redis, API, and frontend. Chooses RPC from the Arc Testnet manifest."),
  bullet("Manages seed factories and the aggregator allowlist."),
  bullet("Cannot sign a user transaction. Does not write Redis from the API process."),
  h3("Venue operator"),
  bullet(
    "Deploys and maintains pools. Inclusion depends on a supported adapter, an allowlisted factory or hub, complete market state, and usable liquidity."
  ),
  bullet("Does not sit in the quote or swap path."),

  h2("Operating model"),
  bullet(
    "User swaps are non-custodial. The aggregator pulls input, executes hops, and returns output in one Arc transaction. msg.value must be 0. Direct ETH to the contract reverts."
  ),
  bullet(
    "chakra-market-data-worker is the only Redis writer. API replicas are stateless readers. Pool-key TTL is eviction, not a freshness SLA. Untouched pools keep the last written value."
  ),
  bullet(
    "CLMM ticks live in pool contract storage. The worker writes a CLMM pool to Redis only when tick coverage is complete. Incomplete windows are not quoted."
  ),
  bullet(
    "/quote hydrates from Redis (or in-memory store) and never calls RPC. /build_tx encodes the client's sub_routes. It checks paused state and allowances on RPC. It does not compute a new route."
  ),
  bullet(
    "A paused aggregator fails /build_tx. Protocol fee is 0 bps on every quote, including empty, single-path, and split routes."
  ),
  bullet(
    "Catalog tokens only: ERC-20 USDC (0x3600…0000, 6 decimals), EURC (0x89B5…D72a, 6 decimals), cirBTC (0xf0C4…32BF, 8 decimals). Native USDC gas encoding is rejected as token_in / token_out."
  ),
  bullet(
    "The user (or their wallet adapter) signs and submits. Circle SCA and Gas Station remain grant targets."
  ),

  h2("Quote"),
  para(
    "A quote is an off-chain computation of expected output and sub_routes. PathFinder walks the in-memory graph. The API overlays Redis pool state and runs venue math on each hop. SplitOptimizer may split input across paths. Native USDC gas is rejected before quoting."
  ),
  imageParagraph(
    "quote-flow.png",
    "Quote flow",
    "GET /api/v1/quote: load engine, PathFinder, Redis MGET, QuoteEngine, SplitOptimizer, envelope. Never RPC."
  ),
  caption("Quote flow. GET /api/v1/quote. Local math over Redis. Never RPC. Numbers match the steps below."),
  numberedMixed("quoteSteps", [
    new TextRun("The client calls "),
    code("GET /api/v1/quote"),
    new TextRun(" with token_in, token_out, amount_in, and optional slippage_bps, max_hops, and max_splits."),
  ]),
  numberedMixed("quoteSteps", [
    new TextRun("The API loads the engine for the current snapshot version ("),
    code("chakra:snapshot:current"),
    new TextRun("). If the pointer moved, it reloads; if reload fails, it may use a stale engine."),
  ]),
  numbered(
    "quoteSteps",
    "PathFinder returns all candidate paths on the current graph. It does not prune by liquidity."
  ),
  numbered(
    "quoteSteps",
    "The API loads unique pool keys for those paths from Redis (xy=k, stable, CLMM) plus chakra:factories. This step never calls RPC."
  ),
  numbered(
    "quoteSteps",
    "QuoteEngine quotes each path at full amount_in with local math. CLMM hops with incomplete coverage are skipped. Native USDC encoding returns no route."
  ),
  numbered(
    "quoteSteps",
    "SplitOptimizer runs on quoted paths: Brent's method for two paths, pairwise merge for more, and only when impact or competitive delta warrants it."
  ),
  numbered(
    "quoteSteps",
    "The API returns expected_output, minimum_output, is_split, protocol_fee_bps (0), and sub_routes for /build_tx. Every response uses the {success, data, error} envelope."
  ),
  para(
    "A quote is not a reservation. Reserves can move between /quote and inclusion. minAmountOut is the on-chain guard."
  ),

  h2("Swap"),
  para(
    "A swap is quote, unsigned calldata, wallet signature, then wallet submit. /build_tx validates the client's sub_routes against the current snapshot, checks that the aggregator is not paused, reports ERC-20 approvals to Permit2, and optionally returns Permit2 typed data. It encodes splitSwap. It does not re-quote. value is always 0."
  ),
  imageParagraph(
    "swap-execution.png",
    "Swap execution",
    "Quote to aggregator.splitSwap. Wallet signs and submits. Output returns to msg.sender with catalog sweep."
  ),
  caption(
    "Swap execution. Quote to aggregator.splitSwap. Wallet submits. Output returns to msg.sender. Numbers match the steps below."
  ),
  numbered("swapSteps", "The user (or integrator) requests a quote."),
  numbered("swapSteps", "The API returns expected output and sub_routes."),
  numberedMixed("swapSteps", [
    new TextRun("The client posts user, amounts, min_amount_out, and those sub_routes to "),
    code("POST /api/v1/build_tx"),
    new TextRun("."),
  ]),
  numbered(
    "swapSteps",
    "The API revalidates route identity, rejects a paused aggregator, reads ERC-20 and Permit2 allowances on RPC, and encodes splitSwap. If Permit2 allowance is already sufficient and unexpired, typedData is omitted and the encoded signature is empty."
  ),
  numbered(
    "swapSteps",
    "The user signs required ERC-20 approvals, optional Permit2 typed data, and the swap transaction in a wallet."
  ),
  numbered("swapSteps", "The wallet submits to Arc RPC. The API does not submit."),
  numbered(
    "swapSteps",
    "The aggregator pulls tokenIn via Permit2, executes every hop on allowlisted venues, requires amountOut >= minAmountOut, transfers tokenOut to msg.sender, and sweeps leftover USDC, EURC, and cirBTC to the sender."
  ),
  para(
    "If /build_tx fails (paused, invalid route identity, bad catalog token, shared pool), it returns an error and no calldata. Refresh the quote; do not retry the same payload."
  ),
  para(
    "Live split execution on current liquidity remains unclaimed until a confirmed split receipt exists. The optimizer may still return is_split: true in quotes."
  ),

  h2("Pool-state refresh"),
  para(
    "Routing graph and live pool state are separate. Freshness is event-driven: update Redis when the chain moves, or on bootstrap and discovery. Operator env vars and Redis key tables live in the Pool-state architecture page."
  ),
  imageParagraph(
    "pool-state-channels.png",
    "Pool-state update channels",
    "Worker bootstrap, WebSocket logs, HTTP getLogs fallback, fetch pipeline, Redis writes, discovery, API hydrate."
  ),
  caption(
    "Pool-state update channels. Worker is the only Redis writer. Numbers match the steps below."
  ),
  numbered(
    "poolSteps",
    "On worker start, bootstrap seeds topology from the last snapshot, runs discovery, and publishes MarketSnapshot plus full pool state to Redis (chakra:snapshot:*, chakra:pool:*, chakra:factories)."
  ),
  numbered(
    "poolSteps",
    "The log watcher prefers eth_subscribe logs on seed and discovery factories plus known pools. Inclusion is final: a log touch enqueues an immediate fetch and Redis write."
  ),
  numbered(
    "poolSteps",
    "When WebSocket is disabled or dead, the watcher falls back to HTTP eth_getLogs over recent blocks every 500 ms, with topic0 batches of at most 10 (Arc filter limit) and a catch-up cap."
  ),
  numbered("poolSteps", "Touched pools go to the fetch pipeline (RPC workers, then Redis)."),
  numbered("poolSteps", "xy=k and stable always write. CLMM writes only when coverage is complete."),
  numbered(
    "poolSteps",
    "Discovery (about 600 s) republishes the graph and full pool state: new pools, missed events, reconciliation."
  ),
  numbered(
    "poolSteps",
    "Ledger ticks do not publish a new snapshot. The API reloads the graph from the last snapshot and overlays pool state on each /quote."
  ),
  para(
    "Active pools refresh within about 500 ms to 2 s after an on-chain swap or liquidity change. Cold pools stay on their last Redis value until the next write."
  ),

  h2("Functional scope"),
  h3("In scope"),
  bullet(
    "Quotes and atomic swaps on Arc Testnet (chain ID 5042002) across Xylo, Presto, and UnitFlow V2.5, plus generic xy=k / stable / CLMM factories on the seed list."
  ),
  bullet("Catalog ERC-20 USDC, EURC, and cirBTC. Permit2 pull. Factory allowlist."),
  bullet("Multi-hop paths and split optimization when they improve output."),
  bullet("Unsigned transaction building with route-identity checks, wallet signature, and wallet submit."),
  bullet("Public stats from chakra:analytics:* and the /stats dashboard."),
  bullet("Self-host: worker + Redis + API, or equivalent memory-backed topology."),
  h3("Out of scope"),
  bullet("Custody, private-key handling, or API-side submit."),
  bullet(
    "Artificial liquidity or guaranteed quote execution. A quote can go stale between /quote and inclusion; minAmountOut is the on-chain guard."
  ),
  bullet("Order books, lending, limit orders, or DCA."),
  bullet("Vaults or operator arbitrage (grant targets, not live)."),
  bullet("Cross-chain routing, CCTP, Gateway, or non-Arc venues."),
  bullet("Circle user-controlled SCA and Gas Station (grant targets, not live)."),
  bullet("Arc mainnet. This document does not set a mainnet date."),
  bullet("Claiming split execution on live liquidity until a confirmed split receipt exists."),

  h2("Arc and Circle integration"),
  para(
    "Integrators talk to Chakra over REST (OpenAPI) or @chakra-ag/sdk. Wallets sign EIP-155 transactions and optional Permit2 typed data. The aggregator is a non-upgradeable Solidity contract on Arc Testnet at 0xeb12351602c56d47c4ee955193335848952b29d8."
  ),
  para(
    "RPC, explorer, catalog, and venue coordinates are in docs/arc-testnet-manifest.json: HTTP https://rpc.testnet.arc.io, WebSocket wss://rpc.testnet.arc.io, with Blockdaemon, dRPC, and QuickNode failovers. Do not invent Alchemy or Canteen RPC URLs."
  ),
  para(
    "On-chain types are DexType (Xyk, Stable, Clmm, Xylo, Presto), Hop, SubRoute, and Permit2Pull in Aggregator.sol. Off-chain market state is Redis under chakra:."
  ),
  para(
    "Circle Wallets (user-controlled smart contract account on ARC-TESTNET) and Gas Station are grant work. They consume the same to, data, chain_id, value, and authorization fields /build_tx already returns. They do not change the router."
  ),
  para(
    "An arb-only vault and operator bot are also grant work. The vault would be a separate contract; the aggregator stays non-upgradeable. Two splitSwap calls in one transaction keep isolated USDC/EURC venues aligned. That is not retail yield and not Circle StableFX."
  ),

  h2("Current vs grant"),
  para(
    "The live system is an Arc Testnet proof of concept: public API, UI, SDK, worker, Redis, aggregator, and external-wallet settlement. Circle SCA, Gas Station, and an arb-only vault/operator bot are grant targets, not live. Proven split execution on live liquidity is not delivered. Arc mainnet is an external gate, not a date this document sets."
  ),
  para(
    "A short companion for the Circle grant is docs/grants/circle-developer-grant-proposal.md. That document is not the architecture source of truth."
  ),

  h2("Where to go next"),
  table(
    [3600, 5760],
    [
      ["Need", "Doc"],
      ["Quote, build, sign, submit", "Integrator guide"],
      ["Redis keys, env, fetch pipeline", "Pool-state architecture"],
      ["HTTP contract", "API reference and OpenAPI"],
      ["Which binary to run", "Deployment overview"],
      ["Chain, catalog, venues", "Arc Testnet manifest"],
      ["Grant companion", "Circle developer grant proposal"],
    ]
  ),
  para(
    "PlantUML sources for the diagrams in this document are in docs/diagrams/. Regenerate SVG and PNG with docs/diagrams/render.sh. This Word file is the shareable export with embedded diagrams. The markdown page remains the source of truth in git."
  ),
];

const doc = new Document({
  creator: "Chakra",
  title: "Chakra Technical Architecture",
  description: "Technical architecture for the Chakra Arc Testnet DEX aggregator.",
  styles: {
    default: { document: { run: { font: "Arial", size: 22 } } },
    paragraphStyles: [
      {
        id: "Heading1",
        name: "Heading 1",
        basedOn: "Normal",
        next: "Normal",
        quickFormat: true,
        run: { size: 32, bold: true, font: "Arial", color: "15100D" },
        paragraph: { spacing: { before: 240, after: 240 }, outlineLevel: 0 },
      },
      {
        id: "Heading2",
        name: "Heading 2",
        basedOn: "Normal",
        next: "Normal",
        quickFormat: true,
        run: { size: 28, bold: true, font: "Arial", color: "15100D" },
        paragraph: { spacing: { before: 280, after: 160 }, outlineLevel: 1 },
      },
      {
        id: "Heading3",
        name: "Heading 3",
        basedOn: "Normal",
        next: "Normal",
        quickFormat: true,
        run: { size: 24, bold: true, font: "Arial", color: "15100D" },
        paragraph: { spacing: { before: 200, after: 120 }, outlineLevel: 2 },
      },
    ],
  },
  numbering,
  sections: [
    {
      properties: {
        page: {
          size: { width: PAGE_W, height: PAGE_H },
          margin: { top: MARGIN, right: MARGIN, bottom: MARGIN, left: MARGIN },
        },
      },
      headers: {
        default: new Header({
          children: [
            new Paragraph({
              tabStops: [{ type: TabStopType.RIGHT, position: TabStopPosition.MAX }],
              border: {
                bottom: { style: BorderStyle.SINGLE, size: 6, color: "EC7A3A", space: 1 },
              },
              children: [
                new TextRun({ text: "Chakra Technical Architecture", bold: true, size: 20 }),
                new TextRun({ text: "\tArc Testnet", size: 20, color: "5D5148" }),
              ],
            }),
          ],
        }),
      },
      footers: {
        default: new Footer({
          children: [
            new Paragraph({
              tabStops: [{ type: TabStopType.RIGHT, position: TabStopPosition.MAX }],
              children: [
                new TextRun({ text: "Chakra  ·  Arc Testnet  ·  chain 5042002", size: 18, color: "5D5148" }),
                new TextRun({ text: "\tPage ", size: 18, color: "5D5148" }),
                new TextRun({ children: [PageNumber.CURRENT], size: 18, color: "5D5148" }),
              ],
            }),
          ],
        }),
      },
      children,
    },
  ],
});

const buffer = await Packer.toBuffer(doc);
fs.writeFileSync(outPath, buffer);
console.log(`wrote ${outPath} (${buffer.length} bytes)`);
