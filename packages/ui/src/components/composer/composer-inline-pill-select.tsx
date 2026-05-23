import * as SelectPrimitive from "@radix-ui/react-select";
import type { ReactNode } from "react";
import { Select, SelectContent, SelectItem } from "../ui/select";
import { Tooltip, TooltipContent, TooltipTrigger } from "../ui/tooltip";
import {
	ComposerInlineTrigger,
	composerInlineSelectContentClassName,
} from "./composer-inline-trigger";

export interface ComposerInlinePillSelectOption {
	id: string;
	label: string;
}

export interface ComposerInlinePillSelectProps {
	ariaLabel: string;
	icon: ReactNode;
	options: readonly ComposerInlinePillSelectOption[];
	value: string | null;
	onChange: (id: string) => void;
	disabled?: boolean;
	placeholder?: string;
	tooltip?: ReactNode;
	triggerClassName?: string;
}

export function ComposerInlinePillSelect({
	ariaLabel,
	icon,
	options,
	value,
	onChange,
	disabled,
	placeholder,
	tooltip,
	triggerClassName,
}: ComposerInlinePillSelectProps) {
	const selectedLabel =
		options.find((option) => option.id === value)?.label ??
		placeholder ??
		ariaLabel;

	const trigger = (
		<SelectPrimitive.Trigger asChild>
			<ComposerInlineTrigger
				aria-label={ariaLabel}
				className={triggerClassName}
				disabled={disabled}
				icon={icon}
				label={selectedLabel}
			/>
		</SelectPrimitive.Trigger>
	);

	return (
		<Select
			disabled={disabled}
			value={value ?? ""}
			onValueChange={(nextValue) => {
				if (nextValue) onChange(nextValue);
			}}
		>
			{tooltip ? (
				<Tooltip>
					<TooltipTrigger asChild>{trigger}</TooltipTrigger>
					<TooltipContent side="top">{tooltip}</TooltipContent>
				</Tooltip>
			) : (
				trigger
			)}
			<SelectContent
				align="start"
				className={composerInlineSelectContentClassName}
			>
				{options.map((option) => (
					<SelectItem key={option.id} value={option.id}>
						{option.label}
					</SelectItem>
				))}
			</SelectContent>
		</Select>
	);
}
