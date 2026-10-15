import type { BuildTxResponse } from './aggregator';
import { ARC_CHAIN_ID, buildSendParams, isPausedEnvelope, spliceSignature } from './swap-send';

export type SwapExecutionState =
  'building' | 'approving' | 'permitting' | 'sending' | 'confirming' | 'confirmed' | 'error';
export interface SwapExecutionProgress {
  state: SwapExecutionState;
  /** Swap submission hash survives receipt failures. Approval hashes are separate. */
  txHash?: `0x${string}`;
  error?: string;
}
export interface PermitTypedData {
  types: Record<string, Array<{ name: string; type: string }>>;
  primaryType: string;
  domain: { name?: string; version?: string; chainId?: number; verifyingContract?: `0x${string}` };
  message: Record<string, unknown>;
}
export interface SwapExecutionPorts {
  build(): Promise<BuildTxResponse>;
  send(params: {
    to: `0x${string}`;
    data: `0x${string}`;
    value?: bigint;
    maxFeePerGas?: bigint;
    gas?: bigint;
  }): Promise<`0x${string}`>;
  estimate(params: { to: `0x${string}`; data: `0x${string}`; value: bigint; maxFeePerGas: bigint }): Promise<bigint>;
  balance(): Promise<bigint>;
  /** Native and ERC-20 USDC share one balance on Arc (18 native decimals). */
  nativeSpend: bigint;
  sign(data: PermitTypedData): Promise<string>;
  receipt(params: { hash: `0x${string}`; confirmations: number }): Promise<{ status: string }>;
  fee(): Promise<bigint>;
  progress(value: SwapExecutionProgress): void;
}

/** Executes exactly the approvals and optional permit requested by build_tx. */
export async function executeSwap(ports: SwapExecutionPorts): Promise<SwapExecutionProgress> {
  let txHash: `0x${string}` | undefined;
  const report = (state: SwapExecutionState, error?: string) => {
    const value = { state, txHash, error };
    ports.progress(value);
    return value;
  };
  try {
    report('building');
    const built = await ports.build();
    if (!built.success || !built.data) {
      throw new Error(
        isPausedEnvelope(built)
          ? 'Protocol is paused'
          : built.error?.message || 'Failed to build transaction',
      );
    }
    const tx = built.data;
    if (tx.chain_id !== ARC_CHAIN_ID) throw new Error('Transaction is for a different chain');
    if (tx.required_approvals.length) {
      report('approving');
      for (const approval of tx.required_approvals) {
        const data =
          `0x095ea7b3${approval.spender.toLowerCase().replace(/^0x/, '').padStart(64, '0')}${BigInt(approval.amount).toString(16).padStart(64, '0')}` as const;
        const hash = await ports.send({ to: approval.token as `0x${string}`, data });
        const receipt = await ports.receipt({ hash, confirmations: 1 });
        if (receipt.status !== 'success') throw new Error('Token approval failed');
      }
    }
    let calldata = tx.data;
    if (tx.typed_data) {
      report('permitting');
      calldata = spliceSignature(calldata, await ports.sign(tx.typed_data as PermitTypedData));
    }
    report('sending');
    const params = buildSendParams(tx, await ports.fee());
    const request = {
      to: params.to,
      data: calldata as `0x${string}`,
      value: 0n,
      maxFeePerGas: params.maxFeePerGas,
    };
    // Estimate the exact signed calldata before opening the final wallet prompt.
    // A revert must surface here rather than as a misleading wallet gas warning.
    const estimated = await ports.estimate(request);
    const gas = (estimated * 120n + 99n) / 100n;
    if (await ports.balance() < ports.nativeSpend + gas * request.maxFeePerGas) {
      throw new Error('Not enough USDC to cover the swap and network gas. Reduce the amount or add USDC.');
    }
    txHash = await ports.send({ ...request, gas });
    report('confirming');
    const receipt = await ports.receipt({ hash: txHash, confirmations: 1 });
    if (receipt.status !== 'success') throw new Error('Swap transaction failed');
    return report('confirmed');
  } catch (error) {
    return report('error', error instanceof Error ? error.message : 'Transaction failed');
  }
}
