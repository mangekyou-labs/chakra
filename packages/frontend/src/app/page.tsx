import { SwapCard } from '@/components/SwapCard';
import { DisclaimerBanner } from '@/components/DisclaimerBanner';

export default function Home() {
  return (
    <div className="w-full">
      <div className="flex flex-col sm:flex-row items-stretch sm:items-start justify-center pt-1 sm:pt-2 md:pt-4 w-full">
        <div className="w-full sm:w-[520px] shrink-0 min-w-0">
          <DisclaimerBanner className="mb-3" />
          <SwapCard />
        </div>
      </div>
    </div>
  );
}
