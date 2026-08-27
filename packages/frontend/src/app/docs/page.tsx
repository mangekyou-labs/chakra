import Link from 'next/link';
import { GITHUB_REPO_URL } from '@/lib/site';

const API_URL = process.env.NEXT_PUBLIC_CHAKRA_API_URL || 'http://localhost:8080';

const WHY_CARDS = [
  {
    title: 'Best route',
    body: 'Quotes across xy=k, stable, and CLMM venues on Arc Testnet with split routing when it helps execution.',
  },
  {
    title: 'Integrator-ready',
    body: 'REST quote → build_tx flow, OpenAPI spec, TypeScript SDK, and Permit2 typed-data payloads.',
  },
  {
    title: 'Self-hostable',
    body: 'Open-source aggregator contract and api-server — run your own stack if you need to.',
  },
] as const;

const QUICKSTART = `API=${API_URL}
USDC=0x3600000000000000000000000000000000000000
EURC=0x89b50855aa3be2f677cd6303cec089b5f319d72a

# 1) Quote 1 USDC → EURC (slippage_bps integer, default 50 = 0.5%)
curl -sG "$API/api/v1/quote" \\
  --data-urlencode "token_in=$USDC" \\
  --data-urlencode "token_out=$EURC" \\
  --data-urlencode "amount_in=1000000" \\
  --data-urlencode "slippage_bps=50"

# 2) Map quote sub_routes → POST /api/v1/build_tx (see API reference)`;

export default function DocsOverviewPage() {
  return (
    <article className="docs-page">
      <header className="docs-intro">
        <h1 className="docs-title">Developer documentation</h1>
        <p className="docs-lead">
          Integrate Chakra swap routing on Arc Testnet — quote, build the splitSwap calldata, and
          sign in your wallet or bot.
        </p>
        <dl className="docs-ref">
          <div className="docs-ref-row">
            <dt>Base URL</dt>
            <dd>
              <code>{API_URL}</code>
            </dd>
          </div>
          <div className="docs-ref-row">
            <dt>Flow</dt>
            <dd>
              <code>GET /quote</code> → <code>POST /build_tx</code> → sign → submit
            </dd>
          </div>
        </dl>
      </header>

      <section className="docs-card-grid">
        {WHY_CARDS.map((card) => (
          <div key={card.title} className="docs-card docs-card--flat">
            <h2 className="docs-card-title">{card.title}</h2>
            <p className="docs-desc">{card.body}</p>
          </div>
        ))}
      </section>

      <section className="docs-card">
        <h2 className="docs-section-label">Quickstart</h2>
        <p className="docs-desc">
          Fetch a live quote, then use the interactive{' '}
          <Link href="/docs/api" className="docs-inline-link">
            API reference
          </Link>{' '}
          to try <code>build_tx</code> with the returned <code>sub_routes</code>.
        </p>
        <pre className="docs-code-block">{QUICKSTART}</pre>
        <div className="docs-actions">
          <Link href="/docs/api" className="docs-btn docs-btn--inline">
            Open API reference
          </Link>
          <a
            href={`${GITHUB_REPO_URL}/blob/main/docs/integrator-guide.md`}
            target="_blank"
            rel="noopener noreferrer"
            className="docs-btn docs-btn--ghost"
          >
            Full integrator guide
          </a>
        </div>
      </section>

      <section className="docs-card">
        <h2 className="docs-section-label">Rate limits</h2>
        <div className="docs-table-wrap">
          <table className="docs-table">
            <thead>
              <tr>
                <th>Tier</th>
                <th>Limit</th>
                <th>Auth</th>
              </tr>
            </thead>
            <tbody>
              <tr>
                <td>Anonymous</td>
                <td>10 req/s per IP</td>
                <td>none</td>
              </tr>
            </tbody>
          </table>
        </div>
        <p className="docs-hint">
          <code>/health</code> and <code>/ready</code> are exempt from the rate limit. Native USDC
          (18 dp) is gas only — never a swap token.
        </p>
      </section>

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
