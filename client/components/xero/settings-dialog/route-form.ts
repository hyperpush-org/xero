import { z } from "zod"
import type {
  NotificationRouteKindDto,
  UpsertNotificationRouteRequestDto,
} from "@/src/lib/xero-model"
import {
  composeNotificationRouteTarget,
  decomposeNotificationRouteTarget,
  notificationRouteKindSchema,
} from "@/src/lib/xero-model"

function routeFormErrorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message.trim().length > 0) return error.message
  if (typeof error === "string" && error.trim().length > 0) return error
  return fallback
}

export const routeFormSchema = z
  .object({
    routeId: z.string().trim().min(1, "Give this route an ID."),
    routeKind: notificationRouteKindSchema,
    routeTarget: z.string().trim().min(1, "A target is required."),
    enabled: z.boolean(),
  })
  .strict()
  .superRefine((value, ctx) => {
    try {
      composeNotificationRouteTarget(value.routeKind, value.routeTarget)
    } catch (error) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["routeTarget"],
        message: routeFormErrorMessage(error, "Invalid target format."),
      })
    }
  })

export type RouteFormValues = z.input<typeof routeFormSchema>
export type RouteFormErrors = Partial<Record<"routeId" | "routeKind" | "routeTarget" | "form", string>>

export const ROUTE_KINDS: Array<{
  value: NotificationRouteKindDto
  label: string
  placeholder: string
}> = [
  { value: "telegram", label: "Telegram", placeholder: "Chat ID or @channel" },
  { value: "discord", label: "Discord", placeholder: "Channel ID" },
]

export function defaultRouteForm(kind: NotificationRouteKindDto = "telegram"): RouteFormValues {
  return { routeId: "", routeKind: kind, routeTarget: "", enabled: true }
}

export function parseRouteFormErrors(error: unknown): RouteFormErrors {
  if (!(error instanceof z.ZodError)) return { form: routeFormErrorMessage(error, "Validation failed.") }

  const output: RouteFormErrors = {}
  for (const issue of error.issues) {
    const path = issue.path[0]
    if ((path === "routeId" || path === "routeKind" || path === "routeTarget") && !output[path]) {
      output[path] = issue.message
      continue
    }
    if (!output.form) output.form = issue.message
  }

  return output
}

export function toRouteRequest(
  form: RouteFormValues,
): Omit<UpsertNotificationRouteRequestDto, "projectId" | "updatedAt"> {
  const value = routeFormSchema.parse(form)
  return {
    routeId: value.routeId,
    routeKind: value.routeKind,
    routeTarget: composeNotificationRouteTarget(value.routeKind, value.routeTarget),
    enabled: value.enabled,
    metadataJson: null,
  }
}

export function routeTargetDisplay(kind: NotificationRouteKindDto, target: string): string {
  try {
    return decomposeNotificationRouteTarget(kind, target).channelTarget
  } catch {
    return target || "—"
  }
}

export function toEditableRouteForm(route: {
  routeId: string
  routeKind: NotificationRouteKindDto
  routeTarget: string
  enabled: boolean
}): RouteFormValues {
  let routeTarget = route.routeTarget
  try {
    routeTarget = decomposeNotificationRouteTarget(route.routeKind, route.routeTarget).channelTarget
  } catch {
    // Keep the truthful stored target when decomposition fails.
  }

  return {
    routeId: route.routeId,
    routeKind: route.routeKind,
    routeTarget,
    enabled: route.enabled,
  }
}

export { routeFormErrorMessage }
