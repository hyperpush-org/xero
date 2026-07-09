import { ArrowLeftRight, CreditCard, ExternalLink } from "lucide-react";

import { cn } from "../../lib/utils";
import type {
	CreditLimitBillingLink,
	CreditLimitNoticeView,
} from "../../model/credit-limit";
import { Button } from "../ui/button";

export interface CreditLimitNoticeProps {
	notice: CreditLimitNoticeView;
	/** Open a billing/upgrade URL in the system browser. */
	onOpenLink: (url: string) => void;
	/** Open the model/provider picker so the user can switch models. */
	onSwitchModel?: () => void;
	className?: string;
}

/**
 * Purpose-built card for a provider credit/billing limit, docked above the
 * composer. Presentational only — the caller wires `onOpenLink` (to the system
 * browser) and `onSwitchModel` (to the model picker).
 */
export function CreditLimitNotice({
	notice,
	onOpenLink,
	onSwitchModel,
	className,
}: CreditLimitNoticeProps) {
	const heading = notice.modelLabel
		? `${notice.modelLabel}${notice.providerLabel ? ` (${notice.providerLabel})` : ""}`
		: (notice.providerLabel ?? "");

	return (
		<div
			role="status"
			aria-label="Provider credit limit"
			className={cn(
				"flex flex-col gap-2.5 rounded-xl border border-warning/30 bg-warning/[0.07] px-3.5 py-3 text-foreground",
				className,
			)}
		>
			<div className="flex items-start gap-2.5">
				<span className="mt-[1px] flex size-6 shrink-0 items-center justify-center rounded-md bg-warning/15 text-warning">
					<CreditCard className="size-3.5" aria-hidden="true" />
				</span>
				<div className="min-w-0 flex-1">
					<p className="m-0 text-[13px] font-semibold">
						{notice.title}
						{heading ? (
							<span className="font-medium text-muted-foreground">
								{" — "}
								{heading}
							</span>
						) : null}
					</p>
					<p className="mt-0.5 text-[12.5px] leading-relaxed text-muted-foreground">
						{notice.description}
					</p>
				</div>
			</div>
			<div className="flex flex-wrap items-center gap-2 pl-[34px]">
				{notice.links.map((link: CreditLimitBillingLink) => (
					<Button
						key={link.url}
						type="button"
						size="sm"
						className="h-7 gap-1.5 px-2.5 text-[12px]"
						onClick={() => onOpenLink(link.url)}
					>
						{link.label}
						<ExternalLink className="size-3" aria-hidden="true" />
					</Button>
				))}
				{onSwitchModel ? (
					<Button
						type="button"
						size="sm"
						variant="outline"
						className="h-7 gap-1.5 px-2.5 text-[12px]"
						onClick={onSwitchModel}
					>
						<ArrowLeftRight className="size-3" aria-hidden="true" />
						Switch model
					</Button>
				) : null}
			</div>
		</div>
	);
}
