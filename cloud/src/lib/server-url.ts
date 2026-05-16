// Server URL — points at the Phoenix relay/auth API. Override in production via
// the `XERO_SERVER_URL` env var (e.g. https://xeroshell.com). Defaults to the
// local Phoenix dev server.
export function getServerUrl(): string {
	const fromEnv =
		(typeof process !== "undefined"
			? process.env?.XERO_SERVER_URL
			: undefined) ??
		(typeof import.meta !== "undefined"
			? (import.meta as { env?: { VITE_XERO_SERVER_URL?: string } }).env
					?.VITE_XERO_SERVER_URL
			: undefined);
	return fromEnv ?? "http://127.0.0.1:4000";
}
