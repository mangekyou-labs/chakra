'use client';

import { useState, type ReactNode } from 'react';
import { BuildTxCodeSample } from '@/components/BuildTxCodeSample';
import { GITHUB_REPO_URL } from '@/lib/site';

const API_URL = process.env.NEXT_PUBLIC_CHAKRA_API_URL || 'http://localhost:8080';

const TOKENS: Record<string, string> = {
  USDC: '0x3600000000000000000000000000000000000000',
  EURC: '0x89b50855aa3be2f677cd6303cec089b5f319d72a',
};

const DEMO_USER = '0x0000000000000000000000000000000000000001';

type QuoteSubRoute = {
  path: string[];
  pool_addresses: string[];
  amount_in: string;
  source: string;
};

type QuotePayload = {
  amount_in: string;
  minimum_output: string;
  sub_routes: QuoteSubRoute[];
};

/** `source.split(" → ")` → dex_type; path[i]/path[i+1] → token_in/token_out. */
function quoteToBuildTxBody(user: string, tokenIn: string, tokenOut: string, quote: QuotePayload) {
  const sourceToDexType = (source: string) => {
    if (source.includes('stable')) return 'stable';
    if (source.includes('clmm')) return 'clmm';
    return 'xyk';
  };
  return {
    user,
    token_in: tokenIn,
    token_out: tokenOut,
    amount_in: quote.amount_in,
    min_amount_out: quote.minimum_output,
    sub_routes: quote.sub_routes.map((sr) => ({
      amount_in: sr.amount_in,
      steps: sr.pool_addresses.map((pool, i) => ({
        dex_type: sourceToDexType(sr.source.split(' → ')[i] ?? sr.source),
        pool_address: pool,
        token_in: sr.path[i],
        token_out: sr.path[i + 1],
      })),
    })),
  };
}

type Param = { name: string; type: string; required: boolean; desc: string };

export function ApiReference() {
  return (
    <article className="docs-page">
      <header className="docs-intro">
        <h1 className="docs-title">API Documentation</h1>
        <p className="docs-lead">
          Chakra — swap routing across xy=k, stable, and CLMM venues on Arc Testnet.
        </p>
        <p className="docs-meta">
          <a href={GITHUB_REPO_URL} target="_blank" rel="noopener noreferrer">
            github.com/mangekyou-labs/chakra
          </a>
          {' · '}
          <a
            href={`${GITHUB_REPO_URL}/blob/main/docs/openapi.yaml`}
            target="_blank"
            rel="noopener noreferrer"
          >
            OpenAPI
          </a>
        </p>
        <dl className="docs-ref">
          <div className="docs-ref-row">
            <dt>Base URL</dt>
            <dd>
              <code>{API_URL}</code>
            </dd>
          </div>
          <div className="docs-ref-row">
            <dt>Chain</dt>
            <dd>
              <code>Arc Testnet · 5042002 (0x4CEF52)</code>
            </dd>
          </div>
        </dl>
      </header>

      <div className="docs-list">
        <Endpoint
          method="GET"
          path="/api/v1/health"
          description="Health check (rate-limit exempt)."
          tryIt={<PingTryIt path="/api/v1/health" />}
        />

        <Endpoint
          method="GET"
          path="/api/v1/ready"
          description="Snapshot current AND at least one pool key in Redis."
          tryIt={<PingTryIt path="/api/v1/ready" />}
        />

        <Endpoint
          method="GET"
          path="/api/v1/tokens"
          description="Frozen catalog: ERC-20 USDC (6 dp), EURC (6 dp), cirBTC (8 dp). Native USDC is never listed."
          tryIt={<PingTryIt path="/api/v1/tokens" />}
        />

        <Endpoint
          method="GET"
          path="/api/v1/balances"
          description="Catalog ERC-20 balances plus a separate native_usdc (18 dp gas) — never summed."
          params={[{ name: 'account', type: 'string', required: true, desc: '0x EVM address' }]}
          tryIt={<BalancesTryIt />}
        />

        <Endpoint
          method="GET"
          path="/api/v1/quote"
          description="Best route. Integer price_impact_bps; protocol_fee_bps is 0."
          params={[
            { name: 'token_in', type: 'string', required: true, desc: 'ERC-20 address (6/8 dp)' },
            { name: 'token_out', type: 'string', required: true, desc: 'ERC-20 address' },
            { name: 'amount_in', type: 'string', required: true, desc: 'Atomic amount (integer)' },
            {
              name: 'slippage_bps',
              type: 'number',
              required: false,
              desc: 'Integer bps; default 50 (= 0.5%)',
            },
            { name: 'max_hops', type: 'number', required: false, desc: 'Server default 3' },
            { name: 'max_splits', type: 'number', required: false, desc: 'Clamped to server max' },
          ]}
          tryIt={<QuoteTryIt />}
        />

        <Endpoint
          method="POST"
          path="/api/v1/build_tx"
          description="splitSwap calldata + optional Permit2 typed data. Never a re-quoter."
          params={[
            { name: 'user', type: 'string', required: true, desc: '0x EOA address' },
            { name: 'token_in', type: 'string', required: true, desc: 'Input token' },
            { name: 'token_out', type: 'string', required: true, desc: 'Output token' },
            { name: 'amount_in', type: 'string', required: true, desc: 'Atomic amount in' },
            { name: 'min_amount_out', type: 'string', required: true, desc: 'Atomic min out' },
            { name: 'sub_routes', type: 'array', required: true, desc: 'From GET /quote' },
          ]}
          extra={<BuildTxCodeSample />}
          tryIt={<BuildTxTryIt />}
        />
      </div>

      <footer className="docs-card docs-dexes">
        <h2 className="docs-section-label">Supported venues</h2>
        <ul className="docs-dex-list">
          {[
            ['xy=k', 'Uniswap V2 style · 30 bps'],
            ['Stable', 'A=100 · 4 bps'],
            ['CLMM', 'Uniswap V3 style · 30 bps'],
          ].map(([name, type]) => (
            <li key={name}>
              <strong>{name}</strong>
              <span>{type}</span>
            </li>
          ))}
        </ul>
      </footer>
    </article>
  );
}

function Endpoint({
  method,
  path,
  description,
  params = [],
  extra,
  tryIt,
}: {
  method: string;
  path: string;
  description: string;
  params?: Param[];
  extra?: ReactNode;
  tryIt?: ReactNode;
}) {
  const isGet = method === 'GET';

  return (
    <section className="docs-card">
      <div className="docs-endpoint-head">
        <span className={isGet ? 'docs-method docs-method--get' : 'docs-method docs-method--post'}>
          {method}
        </span>
        <code className="docs-path">{path}</code>
      </div>
      <p className="docs-desc">{description}</p>

      {params.length > 0 && (
        <div className="docs-params">
          <p className="docs-section-label">Query / body</p>
          <ul>
            {params.map((p) => (
              <li key={p.name} className="docs-param">
                <code>{p.name}</code>
                <span className="docs-param-type">
                  {p.type}
                  {p.required ? '' : '?'}
                </span>
                <span className="docs-param-desc">{p.desc}</span>
              </li>
            ))}
          </ul>
        </div>
      )}

      {extra && <div className="docs-extra">{extra}</div>}

      {tryIt && (
        <div className="docs-try">
          <p className="docs-section-label">Try it</p>
          {tryIt}
        </div>
      )}
    </section>
  );
}

function PingTryIt({ path }: { path: string }) {
  const [result, setResult] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const run = async () => {
    setLoading(true);
    try {
      const resp = await fetch(`${API_URL}${path}`);
      setResult(JSON.stringify(await resp.json(), null, 2));
    } catch (e: unknown) {
      setResult(`Error: ${e instanceof Error ? e.message : String(e)}`);
    }
    setLoading(false);
  };

  return (
    <>
      <button type="button" className="docs-btn" onClick={run} disabled={loading}>
        {loading ? '…' : 'Send'}
      </button>
      {result && <pre className="docs-out">{result}</pre>}
    </>
  );
}

function BalancesTryIt() {
  const [account, setAccount] = useState(DEMO_USER);
  const [result, setResult] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const run = async () => {
    setLoading(true);
    setResult(null);
    try {
      const resp = await fetch(`${API_URL}/api/v1/balances?account=${account.trim()}`);
      setResult(JSON.stringify(await resp.json(), null, 2));
    } catch (e: unknown) {
      setResult(`Error: ${e instanceof Error ? e.message : String(e)}`);
    }
    setLoading(false);
  };

  return (
    <>
      <div className="docs-form-row">
        <Field label="Account (0x…)">
          <input
            className="docs-input"
            value={account}
            onChange={(e) => setAccount(e.target.value)}
            spellCheck={false}
          />
        </Field>
        <button type="button" className="docs-btn" onClick={run} disabled={loading}>
          {loading ? '…' : 'Send'}
        </button>
      </div>
      {result && <pre className="docs-out">{result}</pre>}
    </>
  );
}

function QuoteTryIt() {
  const [tokenIn, setTokenIn] = useState('USDC');
  const [tokenOut, setTokenOut] = useState('EURC');
  const [amount, setAmount] = useState('100');
  const [slippageBps, setSlippageBps] = useState('50');
  const [result, setResult] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const run = async () => {
    setLoading(true);
    setResult(null);
    const atomic = (parseFloat(amount) * 1_000_000).toFixed(0);
    const q = new URLSearchParams({
      token_in: TOKENS[tokenIn],
      token_out: TOKENS[tokenOut],
      amount_in: atomic,
      slippage_bps: slippageBps,
    });
    try {
      const resp = await fetch(`${API_URL}/api/v1/quote?${q}`);
      setResult(JSON.stringify(await resp.json(), null, 2));
    } catch (e: unknown) {
      setResult(`Error: ${e instanceof Error ? e.message : String(e)}`);
    }
    setLoading(false);
  };

  return (
    <>
      <div className="docs-form-row">
        <Field label="From">
          <select
            className="docs-input"
            value={tokenIn}
            onChange={(e) => setTokenIn(e.target.value)}
          >
            {Object.keys(TOKENS).map((t) => (
              <option key={t} value={t}>
                {t}
              </option>
            ))}
          </select>
        </Field>
        <Field label="To">
          <select
            className="docs-input"
            value={tokenOut}
            onChange={(e) => setTokenOut(e.target.value)}
          >
            {Object.keys(TOKENS).map((t) => (
              <option key={t} value={t}>
                {t}
              </option>
            ))}
          </select>
        </Field>
        <Field label="Amount">
          <input
            className="docs-input docs-input--narrow"
            value={amount}
            onChange={(e) => setAmount(e.target.value)}
          />
        </Field>
        <Field label="Slippage bps">
          <input
            className="docs-input docs-input--narrow"
            value={slippageBps}
            onChange={(e) => setSlippageBps(e.target.value)}
          />
        </Field>
        <button type="button" className="docs-btn" onClick={run} disabled={loading}>
          {loading ? '…' : 'Send'}
        </button>
      </div>
      {result && <pre className="docs-out">{result}</pre>}
    </>
  );
}

function BuildTxTryIt() {
  const [userKey, setUserKey] = useState(DEMO_USER);
  const [tokenIn, setTokenIn] = useState('USDC');
  const [tokenOut, setTokenOut] = useState('EURC');
  const [amount, setAmount] = useState('100');
  const [result, setResult] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const run = async () => {
    setLoading(true);
    setResult(null);
    const tokenInId = TOKENS[tokenIn];
    const tokenOutId = TOKENS[tokenOut];
    const atomic = (parseFloat(amount) * 1_000_000).toFixed(0);
    try {
      const quoteResp = await fetch(
        `${API_URL}/api/v1/quote?${new URLSearchParams({
          token_in: tokenInId,
          token_out: tokenOutId,
          amount_in: atomic,
          slippage_bps: '50',
        }).toString()}`,
      );
      const quoteJson = await quoteResp.json();
      if (!quoteJson.success || !quoteJson.data?.sub_routes?.length) {
        setResult(JSON.stringify(quoteJson, null, 2));
        setLoading(false);
        return;
      }

      const buildBody = quoteToBuildTxBody(userKey.trim(), tokenInId, tokenOutId, quoteJson.data);
      const buildResp = await fetch(`${API_URL}/api/v1/build_tx`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(buildBody),
      });
      setResult(JSON.stringify(await buildResp.json(), null, 2));
    } catch (e: unknown) {
      setResult(`Error: ${e instanceof Error ? e.message : String(e)}`);
    }
    setLoading(false);
  };

  return (
    <>
      <p className="docs-hint">
        Calls <code>GET /quote</code> first so the sub-route steps match live routing, then POSTs{' '}
        <code>build_tx</code> with <code>user</code>.
      </p>
      <div className="docs-form-row">
        <Field label="User (0x…)">
          <input
            className="docs-input"
            value={userKey}
            onChange={(e) => setUserKey(e.target.value)}
            spellCheck={false}
          />
        </Field>
        <Field label="From">
          <select
            className="docs-input"
            value={tokenIn}
            onChange={(e) => setTokenIn(e.target.value)}
          >
            {Object.keys(TOKENS).map((t) => (
              <option key={t} value={t}>
                {t}
              </option>
            ))}
          </select>
        </Field>
        <Field label="To">
          <select
            className="docs-input"
            value={tokenOut}
            onChange={(e) => setTokenOut(e.target.value)}
          >
            {Object.keys(TOKENS).map((t) => (
              <option key={t} value={t}>
                {t}
              </option>
            ))}
          </select>
        </Field>
        <Field label="Amount">
          <input
            className="docs-input docs-input--narrow"
            value={amount}
            onChange={(e) => setAmount(e.target.value)}
          />
        </Field>
        <button type="button" className="docs-btn" onClick={run} disabled={loading}>
          {loading ? '…' : 'Quote → build_tx'}
        </button>
      </div>
      {result && <pre className="docs-out">{result}</pre>}
    </>
  );
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="docs-field">
      <span>{label}</span>
      {children}
    </label>
  );
}
