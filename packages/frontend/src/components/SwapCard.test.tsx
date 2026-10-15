import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { SwapCard } from './SwapCard';
import { FALLBACK_SWAP_TOKENS } from '@/lib/swap-tokens';

const wallet = vi.hoisted(() => ({
  address: null as string | null,
  onArcMainnet: false,
  connecting: false,
  connect: vi.fn(),
  switchToArc: vi.fn(),
}));
vi.mock('@/lib/wallet-context', () => ({ useWallet: () => wallet }));
vi.mock('wagmi', () => ({ useWalletClient: () => ({}), usePublicClient: () => undefined }));
vi.mock('@/lib/account-balances-context', () => ({
  useAccountBalances: () => ({
    getErc20Balance: () => null,
    nativeBalance: null,
    ready: false,
    loading: false,
  }),
}));
vi.mock('@/lib/hooks', () => ({
  useTokenCatalog: () => ({ tokens: FALLBACK_SWAP_TOKENS }),
  useGasPriceQuery: () => ({}),
  useQuoteQuery: () => ({
    data: {
      amount_in: '3000',
      expected_output: '2671',
      minimum_output: '2657',
      price_impact_bps: 0,
      protocol_fee_bps: 0,
      compute_time_ms: 5,
      sub_routes: [
        {
          source: 'chakra-clmm',
          path: ['a', 'b'],
          amount_in: '3000',
          amount_out: '2671',
          fraction_bps: 10000,
          hop_fees: [1],
        },
      ],
    },
    isLoading: false,
    isStale: false,
  }),
}));
vi.mock('./TokenSelector', () => ({ TokenSelector: () => null }));
vi.mock('./RecentSwaps', () => ({ RecentSwaps: () => null }));
vi.mock('./SwapSettingsModal', () => ({ SwapSettingsModal: () => null }));
vi.mock('./UnauditedModal', () => ({ UnauditedModal: () => null }));
afterEach(cleanup);
beforeEach(() => {
  vi.clearAllMocks();
  wallet.address = null;
  wallet.onArcMainnet = false;
});
it('allows connecting before entering an amount', () => {
  render(<SwapCard />);
  const button = screen.getByRole('button', { name: 'Connect Wallet' }) as HTMLButtonElement;
  expect(button.disabled).toBe(false);
  fireEvent.click(button);
  expect(wallet.connect).toHaveBeenCalledOnce();
});
it('allows switching networks before entering an amount', () => {
  wallet.address = '0x0000000000000000000000000000000000000001';
  render(<SwapCard />);
  const button = screen.getByRole('button', { name: 'Switch to Arc Mainnet' }) as HTMLButtonElement;
  expect(button.disabled).toBe(false);
  fireEvent.click(button);
  expect(wallet.switchToArc).toHaveBeenCalledOnce();
});
it('uses venue names in both summaries and distinguishes pool fees from Chakra fees', () => {
  wallet.address = '0x0000000000000000000000000000000000000001';
  wallet.onArcMainnet = true;
  render(<SwapCard />);
  expect(screen.queryByText('chakra-clmm')).toBeNull();
  expect(screen.getAllByText('UnitFlow V3')).toHaveLength(2);
  expect(screen.getAllByText('0.01%')).toHaveLength(2);
  expect(screen.getAllByText('0%')).toHaveLength(2);
  expect(screen.getAllByText('<0.01%')).toHaveLength(2);
  expect(screen.queryByRole('button', { name: 'Switch to Arc Mainnet' })).toBeNull();
});
