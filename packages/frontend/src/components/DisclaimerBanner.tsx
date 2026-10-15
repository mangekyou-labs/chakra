export function DisclaimerBanner({ className = '' }: { className?: string }) {
  return (
    <div
      className={`text-[13px] sm:text-[14px] leading-relaxed text-[var(--text-muted)] ${className}`}
      role="status"
    >
      <p>
        Arc Mainnet · Contracts unaudited · Verify amounts before signing · Not financial advice
      </p>
      <p className="mt-1">
        Liquidity can be limited and prices can move quickly. Unavailable venues are shown —
        quotes degrade honestly with NO_ROUTE.
      </p>
    </div>
  );
}
