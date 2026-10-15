import { SwapCard } from '@/components/SwapCard';

export default function Home() {
  return (
    <div className="w-full">
      <div className="flex flex-col sm:flex-row items-stretch sm:items-start justify-center pt-1 sm:pt-2 md:pt-4 w-full">
        <div className="w-full sm:w-[520px] shrink-0 min-w-0">
          <SwapCard />
        </div>
      </div>
    </div>
  );
}
