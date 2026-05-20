import { Button } from "@xero/ui/components/ui/button";
import { Download, X } from "lucide-react";
import { useState } from "react";

import { useXeroCloudInstallState } from "#/lib/pwa/use-install-state";

import {
	ChromiumInstallInstructionsDialog,
	IosInstallInstructionsDialog,
} from "./install-app-action";

const DISMISSED_STORAGE_KEY = "xero-cloud:install-nudge-dismissed";

export function hasDismissedInstallPrompt(): boolean {
	try {
		return window.localStorage.getItem(DISMISSED_STORAGE_KEY) === "1";
	} catch {
		return false;
	}
}

export function markInstallPromptDismissed(): void {
	try {
		window.localStorage.setItem(DISMISSED_STORAGE_KEY, "1");
	} catch {
		// Storage can be unavailable (private mode, blocked cookies); the nudge
		// simply shows again next visit rather than crashing.
	}
}

export function InstallPromptToast() {
	const install = useXeroCloudInstallState();
	const [dismissed, setDismissed] = useState(() => hasDismissedInstallPrompt());
	const [showInstructions, setShowInstructions] = useState(false);

	const manualKind =
		install.support === "manual-ios"
			? ("ios" as const)
			: install.support === "manual-chromium"
				? ("chromium" as const)
				: null;
	const canPrompt = install.support === "prompt";

	if (dismissed || (!canPrompt && manualKind === null)) return null;

	const dismiss = () => {
		markInstallPromptDismissed();
		setDismissed(true);
	};

	const handleInstall = async () => {
		if (canPrompt) {
			await install.promptInstall();
			dismiss();
			return;
		}
		setShowInstructions(true);
	};

	return (
		<>
			<div className="pointer-events-none fixed inset-x-0 bottom-0 z-[90] flex justify-center px-4 pb-[max(env(safe-area-inset-bottom),1rem)]">
				<div className="animate-in slide-in-from-bottom-4 fade-in pointer-events-auto flex w-full max-w-sm items-center gap-3 rounded-lg border border-border bg-background/95 p-3.5 shadow-lg backdrop-blur">
					<span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-md border border-border bg-muted/40 text-muted-foreground">
						<Download className="h-4 w-4" aria-hidden />
					</span>
					<div className="flex min-w-0 flex-1 flex-col gap-0.5">
						<p className="text-[13px] font-medium text-foreground">
							Install Xero Cloud
						</p>
						<p className="text-[12px] leading-snug text-muted-foreground">
							{manualKind === "ios"
								? "Add Xero to your home screen for a full-screen app."
								: "Install Xero as an app for a faster, full-screen experience."}
						</p>
					</div>
					<div className="flex shrink-0 items-center gap-1">
						<Button
							type="button"
							size="sm"
							onClick={() => void handleInstall()}
							className="h-8 px-3 text-[12px] font-medium"
						>
							Install
						</Button>
						<Button
							type="button"
							size="icon"
							variant="ghost"
							aria-label="Dismiss install prompt"
							onClick={dismiss}
							className="size-8 text-muted-foreground hover:text-foreground"
						>
							<X className="h-4 w-4" />
						</Button>
					</div>
				</div>
			</div>
			{manualKind === "ios" ? (
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
