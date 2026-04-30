//! Privy free-tier scaffold.
//!
//! Privy ships a free tier with up to 1k MAU; the SDK exposes both
//! social/email login and external wallet connect under a unified
//! provider. We scaffold an `.env` with the required `PRIVY_APP_ID`
//! placeholder so the developer can paste their app id after signing
//! up at https://dashboard.privy.io.

use super::{escape_ts_string, ScaffoldMeta, WalletScaffoldContext};

pub fn render(ctx: &WalletScaffoldContext) -> (Vec<(String, String)>, ScaffoldMeta) {
    let files = vec![
        ("package.json".into(), package_json(ctx)),
        ("tsconfig.json".into(), tsconfig()),
        ("vite.config.ts".into(), vite_config()),
        ("index.html".into(), index_html(ctx)),
        ("src/main.tsx".into(), main_tsx(ctx)),
        ("src/App.tsx".into(), app_tsx(ctx)),
        ("src/PrivyRoot.tsx".into(), privy_root_tsx(ctx)),
        ("src/AuthPanel.tsx".into(), auth_panel_tsx()),
        ("src/app.css".into(), css()),
        ("src/vite-env.d.ts".into(), vite_env_dts()),
        (".env.example".into(), env_example(ctx)),
        ("README.md".into(), readme(ctx)),
        (".gitignore".into(), gitignore()),
    ];
    let meta = ScaffoldMeta {
        entrypoint: Some("src/main.tsx".into()),
        start_command: "pnpm dev".into(),
        api_key_env: Some("PRIVY_APP_ID".into()),
        next_steps: vec![
            "Sign up at https://dashboard.privy.io and create an app (the free tier covers up to 1k MAU).".into(),
            "Copy the App ID into .env as VITE_PRIVY_APP_ID.".into(),
            "pnpm install && pnpm dev".into(),
            "Update login methods in src/PrivyRoot.tsx if you want SMS / passkey / only-external-wallet.".into(),
        ],
    };
    (files, meta)
}

fn package_json(ctx: &WalletScaffoldContext) -> String {
    format!(
        r#"{{
  "name": "{slug}",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {{
    "dev": "vite",
    "build": "tsc --noEmit && vite build",
    "preview": "vite preview",
    "typecheck": "tsc --noEmit"
  }},
  "dependencies": {{
    "@privy-io/react-auth": "^2.7.0",
    "@solana/web3.js": "^1.95.0",
    "react": "^18.3.1",
    "react-dom": "^18.3.1"
  }},
  "devDependencies": {{
    "@types/react": "^18.3.3",
    "@types/react-dom": "^18.3.0",
    "@vitejs/plugin-react": "^4.3.1",
    "typescript": "^5.5.0",
    "vite": "^5.3.0"
  }}
}}
"#,
        slug = ctx.project_slug,
    )
}

fn tsconfig() -> String {
    r#"{
  "compilerOptions": {
    "target": "ES2022",
    "lib": ["ES2022", "DOM", "DOM.Iterable"],
    "module": "ESNext",
    "moduleResolution": "bundler",
    "jsx": "react-jsx",
    "strict": true,
    "esModuleInterop": true,
    "resolveJsonModule": true,
    "skipLibCheck": true,
    "isolatedModules": true
  },
  "include": ["src"]
}
"#
    .into()
}

fn vite_config() -> String {
    r#"import { defineConfig } from "vite"
import react from "@vitejs/plugin-react"

export default defineConfig({ plugins: [react()] })
"#
    .into()
}

fn index_html(ctx: &WalletScaffoldContext) -> String {
    format!(
        r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>{app_name}</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
"#,
        app_name = ctx.app_name,
    )
}

fn main_tsx(ctx: &WalletScaffoldContext) -> String {
    format!(
        r#"import React from "react"
import ReactDOM from "react-dom/client"
import {{ PrivyRoot }} from "./PrivyRoot"
import {{ App }} from "./App"
import "./app.css"

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <PrivyRoot>
      <App appName="{app_name}" />
    </PrivyRoot>
  </React.StrictMode>,
)
"#,
        app_name = escape_ts_string(&ctx.app_name),
    )
}

fn privy_root_tsx(ctx: &WalletScaffoldContext) -> String {
    format!(
        r##"import {{ type ReactNode }} from "react"
import {{ PrivyProvider }} from "@privy-io/react-auth"

const APP_ID = import.meta.env.VITE_PRIVY_APP_ID ?? "{app_id}"

export function PrivyRoot({{ children }}: {{ children: ReactNode }}) {{
  if (!APP_ID || APP_ID === "replace-me") {{
    console.warn("[privy] VITE_PRIVY_APP_ID is not set — auth will fail.")
  }}
  return (
    <PrivyProvider
      appId={{APP_ID}}
      config={{
        {{
          appearance: {{ theme: "dark", accentColor: "#6366f1" }},
          loginMethods: ["email", "wallet", "google"],
          embeddedWallets: {{
            createOnLogin: "users-without-wallets",
          }},
          defaultChain: {{
            id: -1,
            name: "Solana {cluster}",
            nativeCurrency: {{ name: "Solana", symbol: "SOL", decimals: 9 }},
            rpcUrls: {{ default: {{ http: ["{rpc_url}"] }} }},
          }},
          solanaClusters: [
            {{ name: "{cluster}", rpcUrl: "{rpc_url}" }},
          ],
        }} as any
      }}
    >
      {{children}}
    </PrivyProvider>
  )
}}
"##,
        cluster = ctx.cluster.as_str(),
        rpc_url = escape_ts_string(&ctx.rpc_url),
        app_id = escape_ts_string(ctx.app_id.as_deref().unwrap_or("replace-me")),
    )
}

fn auth_panel_tsx() -> String {
    r#"import { usePrivy, useWallets } from "@privy-io/react-auth"

export function AuthPanel() {
  const { ready, authenticated, login, logout, user } = usePrivy()
  const { wallets } = useWallets()

  if (!ready) return <p>Loading Privy…</p>

  if (!authenticated) {
    return (
      <button type="button" onClick={() => void login()} className="primary">
        Sign in
      </button>
    )
  }

  const solanaWallet = wallets.find((w) =>
    (w as { walletClientType?: string }).walletClientType === "solana" ||
    (w as { chainType?: string }).chainType === "solana",
  )

  return (
    <div className="authed">
      <p>
        Signed in as <strong>{user?.email?.address ?? user?.id ?? "unknown"}</strong>
      </p>
      {solanaWallet ? (
        <p>
          Solana wallet: <code>{solanaWallet.address}</code>
        </p>
      ) : (
        <p>No Solana wallet on this account — create one from the Privy modal.</p>
      )}
      <button type="button" onClick={() => void logout()}>
        Sign out
      </button>
    </div>
  )
}
"#
    .into()
}

fn app_tsx(_ctx: &WalletScaffoldContext) -> String {
    r#"import { AuthPanel } from "./AuthPanel"

interface AppProps {
  appName: string
}

export function App({ appName }: AppProps) {
  return (
    <main className="app">
      <header>
        <h1>{appName}</h1>
      </header>
      <AuthPanel />
    </main>
  )
}
"#
    .into()
}

fn css() -> String {
    r#"body {
  margin: 0;
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
  background: #0a0a0a;
  color: #f5f5f5;
}
.app {
  min-height: 100vh;
  padding: 32px;
}
.app header {
  border-bottom: 1px solid rgba(255, 255, 255, 0.08);
  padding-bottom: 16px;
  margin-bottom: 24px;
}
button.primary {
  background: #6366f1;
  border: 0;
  color: white;
  padding: 10px 18px;
  border-radius: 6px;
  cursor: pointer;
  font-size: 14px;
}
.authed code {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  word-break: break-all;
}
"#
    .into()
}

fn vite_env_dts() -> String {
    r#"/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_PRIVY_APP_ID?: string
}

interface ImportMeta {
  readonly env: ImportMetaEnv
}
"#
    .into()
}

fn env_example(ctx: &WalletScaffoldContext) -> String {
    format!(
        r#"# Copy to `.env` and paste your Privy app id after signing up at
# https://dashboard.privy.io. The free tier covers up to 1k MAU.
VITE_PRIVY_APP_ID="{app_id}"
VITE_RPC_URL="{rpc_url}"
"#,
        app_id = ctx.app_id.as_deref().unwrap_or("replace-me"),
        rpc_url = ctx.rpc_url,
    )
}

fn readme(ctx: &WalletScaffoldContext) -> String {
    format!(
        r#"# {slug}

Privy (free tier) scaffold generated by Xero's Solana workbench.

## Sign up

1. Create an app at https://dashboard.privy.io (free tier: 1k MAU).
2. Copy the App ID into `.env` as `VITE_PRIVY_APP_ID`.

## Run

```bash
pnpm install
pnpm dev
```

## Cluster

Baked in at scaffold time — **{cluster}** (RPC: `{rpc_url}`). Update the
`solanaClusters` entry in `src/PrivyRoot.tsx` to change.

## What's included

- Social login (email + Google) alongside external wallet connect.
- Embedded Solana wallet auto-creation for users who sign in without a
  wallet.
- `useWallets()` exposes both embedded and external Solana wallets so
  you can call `signAndSendTransaction` without extra plumbing.
"#,
        slug = ctx.project_slug,
        cluster = ctx.cluster.as_str(),
        rpc_url = ctx.rpc_url,
    )
}

fn gitignore() -> String {
    "node_modules\ndist\n.DS_Store\n.env\n.env.local\n".into()
}
