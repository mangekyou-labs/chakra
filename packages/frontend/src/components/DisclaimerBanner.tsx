export function DisclaimerBanner({ className = '' }: { className?: string }) {
  return (
    <p
      className={`text-[13px] sm:text-[14px] leading-relaxed text-[var(--text-muted)] ${className}`}
      role="status"
    >
      Arc Testnet · Contracts unaudited · Verify amounts before signing · Not financial advice
    </p>
  );
}
