import { describe, expect, it, vi } from 'vitest';
import { executeSwap, type SwapExecutionPorts } from './swap-execution';
import type { BuildTxData } from './aggregator';

const tx: BuildTxData = {
  to: '0x1111111111111111111111111111111111111111',
  data: '0x2e3be0c1' + '00'.repeat(256),
  chain_id: 5042,
  value: '0',
  deadline: 1,
  typed_data: null,
  required_approvals: [
    {
      token: '0x2222222222222222222222222222222222222222',
      spender: '0x3333333333333333333333333333333333333333',
      amount: '17',
    },
  ],
};
function ports(data = tx) {
  return {
    build: vi.fn(async () => ({ success: true, data })),
    send: vi.fn<SwapExecutionPorts['send']>(async () => '0xabc' as const),
    sign: vi.fn(async () => '0x' + 'ab'.repeat(65)),
    receipt: vi.fn<SwapExecutionPorts['receipt']>(async () => ({ status: 'success' })),
    fee: vi.fn(async () => 1n),
    estimate: vi.fn<SwapExecutionPorts['estimate']>(async () => 250_000n),
    balance: vi.fn(async () => 140_000_000_000_000_000n),
    nativeSpend: 100_000_000_000_000_000n,
    progress: vi.fn(),
  } satisfies SwapExecutionPorts;
}

describe('production swap execution', () => {
  it('stops after an unsuccessful approval receipt', async () => {
    const p = ports();
    p.receipt.mockResolvedValue({ status: 'reverted' });
    const result = await executeSwap(p);
    expect(result.state).toBe('error');
    expect(p.send).toHaveBeenCalledTimes(1);
    expect(p.sign).not.toHaveBeenCalled();
    expect(p.fee).not.toHaveBeenCalled();
    expect(result.txHash).toBeUndefined();
    expect(p.progress).not.toHaveBeenCalledWith(expect.objectContaining({ state: 'confirmed' }));
  });
});

it('uses requested spender and amount and waits before signing and swapping', async () => {
  const p = ports({
    ...tx,
    typed_data: { primaryType: 'PermitSingle', domain: { chainId: 5042 }, types: {}, message: {} },
    required_approvals: [...tx.required_approvals, { ...tx.required_approvals[0], amount: '18' }],
  });
  const order: string[] = [];
  p.send.mockImplementation(async () => {
    order.push('send');
    return '0xabc';
  });
  p.receipt.mockImplementation(async () => {
    order.push('receipt');
    return { status: 'success' };
  });
  p.sign.mockImplementation(async () => {
    order.push('sign');
    return '0x' + 'ab'.repeat(65);
  });
  const result = await executeSwap(p);
  expect(order).toEqual(['send', 'receipt', 'send', 'receipt', 'sign', 'send', 'receipt']);
  expect(p.send.mock.calls[0][0]).toEqual({
    to: tx.required_approvals[0].token,
    data: '0x095ea7b3' + '0'.repeat(24) + '3'.repeat(40) + '0'.repeat(62) + '11',
  });
  expect(p.send.mock.calls[1][0].data.endsWith('12')).toBe(true);
  const swap = p.send.mock.calls[2][0];
  expect(swap.to).toBe(tx.to);
  expect(swap.value).toBe(0n);
  expect(swap.maxFeePerGas).toBe(20_000_000_000n);
  expect(swap.data).toContain('ab'.repeat(65));
  expect(result.state).toBe('confirmed');
  expect(p.receipt.mock.calls.every(([request]) => request.confirmations === 1)).toBe(true);
});

it('skips unrequested approvals and signing and keeps higher fees', async () => {
  const p = ports({ ...tx, required_approvals: [] });
  p.fee.mockResolvedValue(30_000_000_000n);
  const result = await executeSwap(p);
  expect(result.state).toBe('confirmed');
  expect(p.send).toHaveBeenCalledExactlyOnceWith({
    to: tx.to,
    data: tx.data,
    value: 0n,
    maxFeePerGas: 30_000_000_000n,
    gas: 300_000n,
  });
  expect(p.sign).not.toHaveBeenCalled();
});

it.each(['reverted', 'unexpected'])(
  'keeps the swap hash and never confirms a %s swap receipt',
  async (status) => {
    const p = ports({ ...tx, required_approvals: [] });
    p.receipt.mockResolvedValue({ status });
    expect(await executeSwap(p)).toEqual({
      state: 'error',
      txHash: '0xabc',
      error: 'Swap transaction failed',
    });
    expect(p.progress).not.toHaveBeenCalledWith(expect.objectContaining({ state: 'confirmed' }));
  },
);

it('keeps the submitted swap hash when receipt lookup fails', async () => {
  const p = ports({ ...tx, required_approvals: [] });
  p.receipt.mockRejectedValue(new Error('RPC unavailable'));
  expect(await executeSwap(p)).toEqual({
    state: 'error',
    txHash: '0xabc',
    error: 'RPC unavailable',
  });
});

it.each(['build', 'send', 'sign'] as const)(
  'reports a rejected %s without sending a later transaction',
  async (operation) => {
    const p = ports({
      ...tx,
      typed_data: { types: {}, domain: {}, message: {}, primaryType: 'PermitSingle' },
    });
    p[operation].mockRejectedValue(new Error('User rejected'));
    const result = await executeSwap(p);
    expect(result.state).toBe('error');
    expect(result.error).toBe('User rejected');
    expect(result.txHash).toBeUndefined();
    expect(p.send).toHaveBeenCalledTimes(operation === 'build' ? 0 : 1);
  },
);

it.each([
  [{ success: false, error: { code: 'PAUSED', message: 'paused' } }, 'Protocol is paused'],
  [{ success: false, error: { code: 'BUILD_FAILED', message: 'No route' } }, 'No route'],
  [{ success: true }, 'Failed to build transaction'],
] as const)(
  'handles unsuccessful build envelopes before wallet prompts',
  async (response, error) => {
    const p: SwapExecutionPorts = { ...ports(), build: async () => response };
    expect(await executeSwap(p)).toEqual({ state: 'error', txHash: undefined, error });
    expect(p.send).not.toHaveBeenCalled();
  },
);

it('rejects an envelope from the historical testnet', async () => {
  const p = ports({ ...tx, chain_id: 5042002 });
  expect((await executeSwap(p)).state).toBe('error');
  expect(p.send).not.toHaveBeenCalled();
});


it('simulates signed permit calldata before the final wallet prompt', async () => {
  const p = ports({ ...tx, required_approvals: [], typed_data: { types: {}, domain: {}, message: {}, primaryType: 'PermitSingle' } });
  await executeSwap(p);
  expect(p.estimate.mock.calls[0][0].data).toContain('ab'.repeat(65));
  expect(p.estimate.mock.invocationCallOrder[0]).toBeLessThan(p.send.mock.invocationCallOrder[0]);
  expect(p.send.mock.calls[0][0].gas).toBe(300_000n);
});

it('stops a reverting swap before asking the wallet to send it', async () => {
  const p = ports({ ...tx, required_approvals: [] });
  p.estimate.mockRejectedValue(new Error('PoolNotFromFactory'));
  expect((await executeSwap(p)).error).toBe('PoolNotFromFactory');
  expect(p.send).not.toHaveBeenCalled();
});

it('reserves gas from the same balance spent as ERC-20 USDC', async () => {
  const p = ports({ ...tx, required_approvals: [] });
  p.balance.mockResolvedValue(104_000_000_000_000_000n);
  expect((await executeSwap(p)).error).toContain('cover the swap and network gas');
  expect(p.send).not.toHaveBeenCalled();
});

it('checks the balance after approvals, not a cached initial balance', async () => {
  const p = ports();
  p.balance.mockResolvedValue(0n);
  expect((await executeSwap(p)).state).toBe('error');
  expect(p.send).toHaveBeenCalledTimes(1);
  expect(p.balance.mock.invocationCallOrder[0]).toBeGreaterThan(p.receipt.mock.invocationCallOrder[0]);
});
