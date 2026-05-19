import { beforeEach, describe, expect, it, vi } from "vitest";

import {
	activateWaitingXeroCloudServiceWorker,
	registerXeroCloudServiceWorker,
} from "./service-worker-registration";

class MockWorker extends EventTarget {
	state: ServiceWorkerState = "installing";
	postMessage = vi.fn();
}

class MockRegistration extends EventTarget {
	scope = "https://cloud.xeroshell.test/";
	installing: MockWorker | null = null;
	waiting: MockWorker | null = null;
	unregister = vi.fn(async () => true);
}

class MockServiceWorkerContainer extends EventTarget {
	controller: MockWorker | null = null;
	registrations: MockRegistration[] = [];
	register = vi.fn(async () => {
		const registration = new MockRegistration();
		this.registrations.push(registration);
		return registration;
	});
	getRegistrations = vi.fn(async () => this.registrations);
	getRegistration = vi.fn(async () => this.registrations[0]);
}

describe("service worker registration", () => {
	let serviceWorker: MockServiceWorkerContainer;

	beforeEach(() => {
		serviceWorker = new MockServiceWorkerContainer();
	});

	it("does not register in development and unregisters stale local workers", async () => {
		const stale = new MockRegistration();
		const otherOrigin = new MockRegistration();
		otherOrigin.scope = "https://other.example/";
		serviceWorker.registrations = [stale, otherOrigin];

		registerXeroCloudServiceWorker({
			isProduction: false,
			scopeOrigin: "https://cloud.xeroshell.test",
			serviceWorker: serviceWorker as unknown as ServiceWorkerContainer,
		});
		await flushPromises();

		expect(serviceWorker.getRegistrations).toHaveBeenCalled();
		expect(serviceWorker.register).not.toHaveBeenCalled();
		expect(stale.unregister).toHaveBeenCalled();
		expect(otherOrigin.unregister).not.toHaveBeenCalled();
	});

	it("registers the production service worker at the cloud root scope", async () => {
		registerXeroCloudServiceWorker({
			isProduction: true,
			serviceWorker: serviceWorker as unknown as ServiceWorkerContainer,
		});
		await flushPromises();

		expect(serviceWorker.register).toHaveBeenCalledWith("/sw.js", {
			scope: "/",
		});
	});

	it("notifies when an updated worker is waiting behind an active controller", async () => {
		const active = new MockWorker();
		const waiting = new MockWorker();
		const registration = new MockRegistration();
		registration.waiting = waiting;
		serviceWorker.controller = active;
		serviceWorker.register = vi.fn(async () => registration);
		const onUpdateReady = vi.fn();

		registerXeroCloudServiceWorker({
			isProduction: true,
			serviceWorker: serviceWorker as unknown as ServiceWorkerContainer,
			onUpdateReady,
		});
		await flushPromises();

		expect(onUpdateReady).toHaveBeenCalledWith(registration);
	});

	it("reloads only after the user activates a waiting service worker", () => {
		const waiting = new MockWorker();
		const registration = new MockRegistration();
		registration.waiting = waiting;
		const reload = vi.fn();

		activateWaitingXeroCloudServiceWorker(
			registration as unknown as ServiceWorkerRegistration,
			{
				serviceWorker: serviceWorker as unknown as ServiceWorkerContainer,
				reload,
			},
		);

		expect(waiting.postMessage).toHaveBeenCalledWith({ type: "SKIP_WAITING" });
		expect(reload).not.toHaveBeenCalled();

		serviceWorker.dispatchEvent(new Event("controllerchange"));
		expect(reload).toHaveBeenCalledTimes(1);
	});
});

async function flushPromises() {
	await new Promise((resolve) => setTimeout(resolve, 0));
}
