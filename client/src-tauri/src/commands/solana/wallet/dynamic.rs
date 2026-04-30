//! Dynamic free-tier scaffold.
//!
//! Dynamic's free tier covers onboarding via email + wallet connect
//! with minimal code. The scaffold bakes in `VITE_DYNAMIC_ENVIRONMENT_ID`
//! which the developer pastes after signing up at app.dynamic.xyz.

use super::{escape_ts_string, ScaffoldMeta, WalletScaffoldContext};

pub fn render(ctx: &WalletScaffoldContext) -> (Vec<(String, String)>, ScaffoldMeta) {
    let files = vec![
        ("package.json".into(), package_json(ctx)),
        ("tsconfig.json".into(), tsconfig()),
        ("vite.config.ts".into(), vite_config()),
        ("index.html".into(), index_html(ctx)),
        ("src/main.tsx".into(), main_tsx(ctx)),
        ("src/App.tsx".into(), app_tsx(ctx)),
        ("src/DynamicRoot.tsx".into(), dynamic_root_tsx(ctx)),
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
        api_key_env: Some("DYNAMIC_ENVIRONMENT_ID".into()),
        next_steps: vec![
            "Sign up at https://app.dynamic.xyz and grab your environment id (free tier).".into(),
            "Paste it into .env as VITE_DYNAMIC_ENVIRONMENT_ID.".into(),
            "pnpm install && pnpm dev".into(),
            "Tune `walletConnectors` in src/DynamicRoot.tsx if you want more than the default Solana set.".into(),
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
    "@dynamic-labs/sdk-react-core": "^3.8.0",
    "@dynamic-labs/solana": "^3.8.0",
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
import {{ DynamicRoot }} from "./DynamicRoot"
import {{ App }} from "./App"
import "./app.css"

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <DynamicRoot>
      <App appName="{app_name}" />
    </DynamicRoot>
  </React.StrictMode>,
)
"#,
        app_name = escape_ts_string(&ctx.app_name),
    )
}

fn dynamic_root_tsx(ctx: &WalletScaffoldContext) -> String {
    format!(
        r#"import {{ type ReactNode }} from "react"
import {{ DynamicContextProvider }} from "@dynamic-labs/sdk-react-core"
import {{ SolanaWalletConnectors }} from "@dynamic-labs/solana"

const ENVIRONMENT_ID = import.meta.env.VITE_DYNAMIC_ENVIRONMENT_ID ?? "{env_id}"

export function DynamicRoot({{ children }}: {{ children: ReactNode }}) {{
  if (!ENVIRONMENT_ID || ENVIRONMENT_ID === "replace-me") {{
    console.warn("[dynamic] VITE_DYNAMIC_ENVIRONMENT_ID is not set — auth will fail.")
  }}
  return (
    <DynamicContextProvider
      settings={{
        {{
          environmentId: ENVIRONMENT_ID,
          walletConnectors: [SolanaWalletConnectors],
          overrides: {{
            chainDisplayName: "Solana {cluster}",
          }},
        }}
      }}
    >
      {{children}}
    </DynamicContextProvider>
  )
}}
"#,
        env_id = escape_ts_string(ctx.app_id.as_deref().unwrap_or("replace-me")),
        cluster = ctx.cluster.as_str(),
    )
}

fn auth_panel_tsx() -> String {
    r#"import { DynamicWidget, useDynamicContext } from "@dynamic-labs/sdk-react-core"

export function AuthPanel() {
  const { primaryWallet, user } = useDynamicContext()
  return (
    <div className="auth-panel">
      <DynamicWidget />
      {user ? (
        <p className="meta">
          Logged in as <strong>{user.email ?? user.username ?? user.userId}</strong>
          {primaryWallet ? (
            <>
              {" "}
              via <code>{primaryWallet.address}</code>
            </>
          ) : null}
        </p>
      ) : null}
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
.auth-panel .meta {
  margin-top: 16px;
  font-size: 13px;
  color: rgba(255, 255, 255, 0.7);
}
.auth-panel code {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  word-break: break-all;
}
"#
    .into()
}

fn vite_env_dts() -> String {
    r#"/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_DYNAMIC_ENVIRONMENT_ID?: string
}

interface ImportMeta {
  readonly env: ImportMetaEnv
}
"#
    .into()
}

fn env_example(ctx: &WalletScaffoldContext) -> String {
    format!(
        r#"# Copy to `.env` and paste your Dynamic environment id after signing up
# at https://app.dynamic.xyz. The free tier covers most early-stage dapps.
VITE_DYNAMIC_ENVIRONMENT_ID="{env_id}"
VITE_RPC_URL="{rpc_url}"
"#,
        env_id = ctx.app_id.as_deref().unwrap_or("replace-me"),
        rpc_url = ctx.rpc_url,
    )
}

fn readme(ctx: &WalletScaffoldContext) -> String {
    format!(
        r#"# {slug}

Dynamic (free tier) scaffold generated by Xero's Solana workbench.

## Sign up

1. Create an environment at https://app.dynamic.xyz.
2. Paste the environment id into `.env` as `VITE_DYNAMIC_ENVIRONMENT_ID`.

## Run

```bash
pnpm install
pnpm dev
```

## Cluster

Baked in at scaffold time — **{cluster}** (RPC: `{rpc_url}`).

## What's included

- `DynamicContextProvider` configured with Solana wallet connectors.
- Default `DynamicWidget` onboarding UI.
- Minimal app shell that surfaces the primary wallet address.
"#,
        slug = ctx.project_slug,
        cluster = ctx.cluster.as_str(),
        rpc_url = ctx.rpc_url,
    )
}

fn gitignore() -> String {
    "node_modules\ndist\n.DS_Store\n.env\n.env.local\n".into()
}
