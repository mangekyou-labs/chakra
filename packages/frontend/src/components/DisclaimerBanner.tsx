export function DisclaimerBanner({ className = '' }: { className?: string }) {
  return (
    <div
      className={`text-[13px] sm:text-[14px] leading-relaxed text-[var(--text-muted)] ${className}`}
      role="status"
    >
      <p>
        Arc Testnet · Contracts unaudited · Verify amounts before signing · Not financial advice
      </p>
      <p className="mt-1">
        Testnet liquidity may deplete or reset at any time. Unavailable venues are never hidden —
        quotes degrade honestly with NO_ROUTE.
      </p>
    </div>
  );
}
