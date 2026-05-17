/**
 * Wallet utilities for signing and submitting Arc transactions.
 */

const NETWORK = 'PUBLIC';
const HORIZON_URL = 'https://horizon.Arc.org';

/**
 * Sign a transaction XDR using wallet.
 */
export async function signTransaction(xdr: string): Promise<string> {
  const walletApi = await import('@Arc/wallet-api');
  const result = await walletApi.signTransaction(xdr, { network: NETWORK });

  if (typeof result === 'string') {
    return result;
  }
  if (result && typeof result === 'object' && 'signedTxXdr' in result) {
    return (result as any).signedTxXdr;
  }
  throw new Error('Failed to sign transaction');
}

/**
 * Submit a signed transaction to the Arc network.
 */
export async function submitTransaction(signedXdr: string): Promise<{
  hash: string;
  success: boolean;
  error?: string;
}> {
  const resp = await fetch(`${HORIZON_URL}/transactions`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
    body: `tx=${encodeURIComponent(signedXdr)}`,
  });

  const data = await resp.json();

  if (resp.ok) {
    return { hash: data.hash, success: true };
  } else {
    const error = data.extras?.result_codes?.transaction
      || data.extras?.result_codes?.operations?.join(', ')
      || data.title
      || 'Transaction failed';
    return { hash: '', success: false, error };
  }
}
