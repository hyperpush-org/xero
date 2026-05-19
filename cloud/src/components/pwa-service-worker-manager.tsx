import { ToastAction } from "@xero/ui/components/ui/toast";
import { toast } from "@xero/ui/components/ui/use-toast";
import { useEffect } from "react";

import {
	activateWaitingXeroCloudServiceWorker,
	registerXeroCloudServiceWorker,
} from "#/lib/pwa/service-worker-registration";

export function PwaServiceWorkerManager() {
	useEffect(() => {
		return registerXeroCloudServiceWorker({
			onUpdateReady: (registration) => {
				toast({
					title: "Xero Cloud update ready",
					description: "Reload when you are ready to use the latest version.",
					duration: 30_000,
					action: (
						<ToastAction
							altText="Reload Xero Cloud"
							onClick={() =>
								activateWaitingXeroCloudServiceWorker(registration)
							}
						>
							Reload
						</ToastAction>
					),
				});
			},
		});
	}, []);

	return null;
}
