import { Button } from "@xero/ui/components/ui/button";
import {
	Dialog,
	DialogClose,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@xero/ui/components/ui/dialog";
import { Download, MousePointerClick, Plus, Share } from "lucide-react";
import { type ComponentPropsWithoutRef, useState } from "react";

import { useXeroCloudInstallState } from "#/lib/pwa/use-install-state";

export interface InstallAppActionProps {
	variant?: "primary" | "compact";
	disabled?: boolean;
	className?: string;
}

export function InstallAppAction({
	variant = "primary",
	disabled = false,
	className,
}: InstallAppActionProps) {
	const install = useXeroCloudInstallState();
	const [showInstructions, setShowInstructions] = useState(false);

	if (install.support === "standalone" || install.support === "unsupported") {
		return null;
	}

	const handleClick = async () => {
		if (install.support === "prompt") {
			await install.promptInstall();
			return;
		}
		setShowInstructions(true);
	};

	return (
		<>
			{variant === "compact" ? (
				<CompactInstallButton
					onClick={() => void handleClick()}
					disabled={disabled}
					className={className}
				/>
			) : (
				<PrimaryInstallButton
					onClick={() => void handleClick()}
					disabled={disabled}
					className={className}
				/>
			)}
			{install.support === "manual-ios" ? (
				<IosInstallInstructionsDialog
					open={showInstructions}
					onOpenChange={setShowInstructions}
				/>
			) : (
				<ChromiumInstallInstructionsDialog
					open={showInstructions}
					onOpenChange={setShowInstructions}
				/>
			)}
		</>
	);
}

type ButtonProps = Pick<
	ComponentPropsWithoutRef<typeof Button>,
	"onClick" | "disabled" | "className"
>;

function PrimaryInstallButton({ onClick, disabled, className }: ButtonProps) {
	return (
		<Button
			type="button"
			variant="outline"
			size="sm"
			onClick={onClick}
			disabled={disabled}
			className={className ?? "h-10 w-full gap-2 px-4 text-[13px] font-medium"}
		>
			<Download className="h-3.5 w-3.5" />
			Install Xero Cloud
		</Button>
	);
}

function CompactInstallButton({ onClick, disabled, className }: ButtonProps) {
	return (
		<Button
			type="button"
			variant="ghost"
			size="icon"
			aria-label="Install Xero Cloud"
			onClick={onClick}
			disabled={disabled}
			className={className ?? "text-muted-foreground hover:text-foreground"}
		>
			<Download className="h-3.5 w-3.5" />
		</Button>
	);
}

export function IosInstallInstructionsDialog({
	open,
	onOpenChange,
}: {
	open: boolean;
	onOpenChange: (open: boolean) => void;
}) {
	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent className="max-w-[min(28rem,calc(100vw-2rem))]">
				<DialogHeader>
					<DialogTitle className="font-display text-[20px] font-medium tracking-tight">
						Install <em className="font-display-italic text-primary">Xero</em>
					</DialogTitle>
					<DialogDescription className="text-[13px] leading-relaxed">
						Add Xero Cloud to your home screen for a standalone app experience.
					</DialogDescription>
				</DialogHeader>
				<ol className="flex flex-col gap-3 text-[13px] text-foreground">
					<li className="flex items-start gap-3">
						<span className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md border border-border bg-muted/40 text-muted-foreground">
							<Share className="h-3.5 w-3.5" aria-hidden />
						</span>
						<span>
							Tap the <span className="font-medium">Share</span> button at the
							bottom of Safari.
						</span>
					</li>
					<li className="flex items-start gap-3">
						<span className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md border border-border bg-muted/40 text-muted-foreground">
							<Plus className="h-3.5 w-3.5" aria-hidden />
						</span>
						<span>
							Choose <span className="font-medium">Add to Home Screen</span> (or{" "}
							<span className="font-medium">Open as Web App</span> on recent
							iOS).
						</span>
					</li>
					<li className="flex items-start gap-3">
						<span className="font-display flex h-7 w-7 shrink-0 items-center justify-center rounded-md border border-border bg-muted/40 text-[12px] font-medium text-muted-foreground">
							3
						</span>
						<span>
							Confirm the name and tap <span className="font-medium">Add</span>{" "}
							to install Xero on your home screen.
						</span>
					</li>
				</ol>
				<DialogFooter>
					<DialogClose asChild>
						<Button type="button" size="sm" variant="secondary">
							Got it
						</Button>
					</DialogClose>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}

export function ChromiumInstallInstructionsDialog({
	open,
	onOpenChange,
}: {
	open: boolean;
	onOpenChange: (open: boolean) => void;
}) {
	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent className="max-w-[min(28rem,calc(100vw-2rem))]">
				<DialogHeader>
					<DialogTitle className="font-display text-[20px] font-medium tracking-tight">
						Install <em className="font-display-italic text-primary">Xero</em>
					</DialogTitle>
					<DialogDescription className="text-[13px] leading-relaxed">
						Install Xero Cloud as an app for a faster, full-screen experience.
					</DialogDescription>
				</DialogHeader>
				<ol className="flex flex-col gap-3 text-[13px] text-foreground">
					<li className="flex items-start gap-3">
						<span className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md border border-border bg-muted/40 text-muted-foreground">
							<Download className="h-3.5 w-3.5" aria-hidden />
						</span>
						<span>
							Click the <span className="font-medium">install icon</span> in the
							address bar, or open your browser menu.
						</span>
					</li>
					<li className="flex items-start gap-3">
						<span className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md border border-border bg-muted/40 text-muted-foreground">
							<MousePointerClick className="h-3.5 w-3.5" aria-hidden />
						</span>
						<span>
							Choose <span className="font-medium">Install Xero</span> (or{" "}
							<span className="font-medium">Add to Home screen</span> on
							mobile).
						</span>
					</li>
					<li className="flex items-start gap-3">
						<span className="font-display flex h-7 w-7 shrink-0 items-center justify-center rounded-md border border-border bg-muted/40 text-[12px] font-medium text-muted-foreground">
							3
						</span>
						<span>
							Confirm to add Xero to your apps and launch it in its own window.
						</span>
					</li>
				</ol>
				<DialogFooter>
					<DialogClose asChild>
						<Button type="button" size="sm" variant="secondary">
							Got it
						</Button>
					</DialogClose>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}
