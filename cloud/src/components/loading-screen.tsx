import { cn } from "@xero/ui/lib/utils";

import { BrandLogo } from "#/components/brand-logo";

interface LoadingScreenProps {
	className?: string;
}

export function LoadingScreen({ className }: LoadingScreenProps) {
	return (
		<output
			aria-live="polite"
			aria-busy="true"
			aria-label="Loading"
			className={cn(
				"flex flex-1 items-center justify-center bg-background",
				className,
			)}
		>
			<div className="cloud-halo-soft relative flex h-24 w-24 items-center justify-center">
				<span
					aria-hidden
					className="absolute inset-0 rounded-full xero-loading-ring"
					style={{
						border:
							"1px solid color-mix(in oklab, var(--primary) 40%, transparent)",
					}}
				/>
				<BrandLogo className="h-7 w-7 xero-loading-breathe" />
			</div>
		</output>
	);
}
