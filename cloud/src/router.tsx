import {
	createRouter as createTanStackRouter,
	type RouterHistory,
} from "@tanstack/react-router";

import { LoadingScreen } from "./components/loading-screen";
import { routeTree } from "./routeTree.gen";

export function getRouter(options: { history?: RouterHistory } = {}) {
	const router = createTanStackRouter({
		routeTree,
		...(options.history ? { history: options.history } : {}),
		scrollRestoration: true,
		defaultPreload: "intent",
		defaultStaleTime: 30_000,
		defaultPreloadStaleTime: 30_000,
		defaultPendingMs: 0,
		defaultPendingMinMs: 0,
		defaultPendingComponent: () => (
			<LoadingScreen className="min-h-dvh w-screen" />
		),
	});

	return router;
}

declare module "@tanstack/react-router" {
	interface Register {
		router: ReturnType<typeof getRouter>;
	}
}
