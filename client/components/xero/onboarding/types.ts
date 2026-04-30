export type OnboardingStepId =
  | "welcome"
  | "providers"
  | "project"
  | "notifications"
  | "environment-access"
  | "confirm"

export type ProviderId =
  | "openai_codex"
  | "openrouter"
  | "anthropic"
  | "openai_api"
  | "azure_openai"
  | "gemini_ai_studio"

export type NotificationChannelId = "telegram" | "discord"
