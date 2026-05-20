import type { ReactNode } from "react";

export interface ComposerSelectOption {
	id: string;
	label: string;
	icon?: ReactNode;
	sublabel?: string;
	disabled?: boolean;
}

export interface ComposerSelectGroup {
	id: string;
	label?: string;
	options: readonly ComposerSelectOption[];
}
