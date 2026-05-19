export const XERO_CLOUD_PWA_ROUTE_RULES = {
	"/manifest.webmanifest": {
		headers: {
			"content-type": "application/manifest+json; charset=utf-8",
			"cache-control": "public, max-age=3600, must-revalidate",
		},
	},
	"/sw.js": {
		headers: {
			"content-type": "text/javascript; charset=utf-8",
			"cache-control": "no-cache, no-store, must-revalidate",
		},
	},
	"/icons/**": {
		headers: {
			"cache-control": "public, max-age=31536000, immutable",
		},
	},
	"/apple-touch-icon.png": {
		headers: {
			"cache-control": "public, max-age=31536000, immutable",
		},
	},
} as const;
